use crate::{
    config::Config as SdkMainConfig, // Renamed to avoid clashes
    error::{Error, Result as SdkResult},
    events::{SolanaEvent, EventType},
    filters::EventFilter,
    resource_manager::ResourceManager,
    utils::{decompress_lz4, metrics as sdk_metrics},
};
use bitquery_solana_core::schemas::{BlockMessage, DexParsedBlockMessage, TokenBlockMessage};
use bytes::Bytes; // For handling protobuf `bytes` fields
use prost::Message as ProstMessage; // Alias for prost's Message trait
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer as RdKafkaStreamConsumer},
    message::{BorrowedMessage, Message as KafkaMessage}, // KafkaMessage trait and BorrowedMessage type
};
use std::collections::HashSet; // For simplified signature deduplication
use std::sync::Arc;
use std::time::Duration; // For sleep durations
use tokio::sync::Mutex as TokioMutex; // Tokio's Mutex for async-compatible locking
use tracing::{debug, error, info, trace, warn}; // Logging macros

/// `StreamConsumer` wraps the `rdkafka::StreamConsumer` to provide a tailored experience
/// for consuming Solana event data from Bitquery's Kafka streams.
///
/// It handles:
/// - Connection to Kafka and topic subscription.
/// - Message deserialization (Protobuf) and decompression (LZ4).
/// - Deduplication of events based on transaction signatures.
/// - Optional pre-filtering of events using `EventFilter`.
/// - Integration with `ResourceManager` for backpressure and resource monitoring.
/// - Kafka offset management (committing offsets after successful processing).
pub struct StreamConsumer {
    inner_kafka_consumer: Arc<RdKafkaStreamConsumer>,
    sdk_config: Arc<SdkMainConfig>, // Full SDK configuration
    // Deduplication cache for seen transaction signatures.
    // Uses a simple HashSet; for very long runs, might need eviction or bloom filter.
    seen_signatures_cache: Arc<TokioMutex<HashSet<String>>>,
    // Optional event filter applied before returning events.
    active_event_filter: Arc<TokioMutex<Option<EventFilter>>>,
    resource_manager: Arc<ResourceManager>, // Shared resource manager instance
}

impl StreamConsumer {
    /// Creates a new `StreamConsumer`.
    ///
    /// # Arguments
    /// * `rdkafka_consumer`: The underlying `rdkafka::StreamConsumer` instance.
    /// * `sdk_config`: The complete SDK configuration.
    /// * `resource_manager`: An `Arc`-wrapped `ResourceManager` for shared resource control.
    pub fn new(
        rdkafka_consumer: RdKafkaStreamConsumer,
        sdk_config: SdkMainConfig,
        resource_manager: Arc<ResourceManager>,
    ) -> Self {
        let buffer_capacity = sdk_config.processing.buffer_size.max(100_000);
        info!("StreamConsumer initialized. Deduplication cache capacity (example): 100,000. Max processing buffer from config: {}", sdk_config.processing.buffer_size);
        Self {
            inner_kafka_consumer: Arc::new(rdkafka_consumer),
            sdk_config: Arc::new(sdk_config),
            // Initialize HashSet with a capacity based on processing config's buffer_size or a fixed large number.
            seen_signatures_cache: Arc::new(TokioMutex::new(HashSet::with_capacity(
                buffer_capacity // Ensure a decent minimum capacity
            ))),
            active_event_filter: Arc::new(TokioMutex::new(None)), // No filter active by default
            resource_manager,
        }
    }

    /// Sets or updates the `EventFilter` used by this consumer.
    /// Events fetched via `next_event()` will be passed through this filter.
    pub async fn set_filter(&self, filter: EventFilter) {
        let mut filter_guard = self.active_event_filter.lock().await;
        *filter_guard = Some(filter);
        info!("StreamConsumer: Event filter has been set/updated.");
    }

    /// Subscribes the underlying Kafka consumer to the specified list of topics.
    pub fn subscribe(&self, topics: &[String]) -> SdkResult<()> {
        let topic_str_slices: Vec<&str> = topics.iter().map(String::as_str).collect();
        self.inner_kafka_consumer.subscribe(&topic_str_slices)
            .map_err(|e| {
                error!("Failed to subscribe to Kafka topics ({:?}): {}", topics, e);
                Error::Kafka(e)
            })
    }

    /// Fetches the next `SolanaEvent` from the Kafka stream after applying all processing steps.
    ///
    /// This is the primary method for consuming events. It incorporates:
    /// 1. Backpressure checks via `ResourceManager`.
    /// 2. Receiving raw messages from Kafka.
    /// 3. Decompression (LZ4) and Deserialization (Protobuf).
    /// 4. Deduplication based on event signatures.
    /// 5. Application of the configured `EventFilter` (if any).
    /// 6. Committing Kafka offsets for processed/skipped messages.
    ///
    /// # Returns
    /// - `Ok(Some(SolanaEvent))`: If a new, valid, non-duplicate, and non-filtered event is available.
    /// - `Ok(None)`: If no event is available right now (e.g., due to backpressure delay,
    ///   empty Kafka poll, or if a message was skipped due to deduplication/filtering).
    ///   The caller should typically retry after a short delay if `None` is returned.
    /// - `Err(Error)`: If an unrecoverable error occurs (e.g., Kafka connection failure,
    ///   critical deserialization error not handled by skipping).
    pub async fn next_event(&self) -> SdkResult<Option<SolanaEvent>> { // Renamed from next_message for clarity
        loop { // Loop to skip over filtered/duplicate messages and fetch the next valid one.
            // --- Resource and Backpressure Checks ---
            if self.resource_manager.is_backpressure_active() {
                warn!("StreamConsumer: Backpressure is active. Delaying event fetching by {}ms.",
                    self.sdk_config.retry.initial_delay.as_millis().max(100)); // Log actual delay
                tokio::time::sleep(self.sdk_config.retry.initial_delay.max(Duration::from_millis(100))).await;
                return Ok(None); // Signal temporary unavailability
            }
            if let Err(e) = self.resource_manager.check_resources().await {
                warn!("StreamConsumer: Resource check failed (pre-receive): {}. Delaying fetch.", e);
                tokio::time::sleep(Duration::from_millis(100)).await; // Brief pause
                return Ok(None); // Signal temporary unavailability
            }

            // --- Receive Message from Kafka ---
            let borrowed_kafka_msg: BorrowedMessage<'_> = match self.inner_kafka_consumer.recv().await {
                Ok(msg) => msg,
                Err(e) => {
                    error!("StreamConsumer: Kafka receive error: {}", e);
                    return Err(Error::Kafka(e)); // Propagate Kafka-level errors
                }
            };

            // --- Process Kafka Message (Decompress, Parse, Deduplicate) ---
            // This inner function handles the transformation from Kafka message to potential SolanaEvent.
            match self.internal_process_kafka_message(&borrowed_kafka_msg).await {
                Ok(Some(event)) => {
                    // --- Apply Event Filter (if any) ---
                    let filter_guard = self.active_event_filter.lock().await;
                    if let Some(ref current_filter) = *filter_guard {
                        if !current_filter.matches(&event) {
                            trace!("StreamConsumer: Event (Sig: {}) was filtered out by the active EventFilter.", event.signature());
                            self.commit_kafka_offset(&borrowed_kafka_msg)?; // Commit offset for filtered message
                            continue; // Loop to get the next message
                        }
                    }
                    // Event is valid, not a duplicate, and passed filters. Commit and return.
                    self.commit_kafka_offset(&borrowed_kafka_msg)?;
                    return Ok(Some(event));
                }
                Ok(None) => { // Message processed but yielded no event (e.g., duplicate, known unhandled topic, parse logic yields no event)
                    debug!("StreamConsumer: Kafka message (Offset: {}) processed but yielded no SolanaEvent. Committing and continuing.", borrowed_kafka_msg.offset());
                    self.commit_kafka_offset(&borrowed_kafka_msg)?; // Ensure commit for skipped messages
                    continue; // Loop to get the next message
                }
                Err(e) => { // Error during internal_process_kafka_message (e.g., decompression, protobuf parse)
                    error!("StreamConsumer: Error processing Kafka message (Topic: {}, Offset: {}): {}. Skipping message.",
                        borrowed_kafka_msg.topic(), borrowed_kafka_msg.offset(), e);
                    sdk_metrics::record_event_processed(borrowed_kafka_msg.topic(), false); // Metric for processing failure
                    self.commit_kafka_offset(&borrowed_kafka_msg)?; // Commit to skip problematic message (poison pill handling)
                    continue; // Loop to get the next message
                }
            }
        }
    }

    /// Helper function to commit Kafka message offset.
    fn commit_kafka_offset(&self, kafka_msg: &BorrowedMessage<'_>) -> SdkResult<()> {
        self.inner_kafka_consumer.commit_message(kafka_msg, CommitMode::Async)
            .map_err(|e| {
                warn!("StreamConsumer: Failed to async commit Kafka offset {}: {}. This might lead to reprocessing.", kafka_msg.offset(), e);
                Error::Kafka(e)
            })
    }

    /// Internal function to process a raw Kafka message:
    /// Decompresses, decodes Protobuf, and checks for duplicates.
    async fn internal_process_kafka_message(&self, kafka_msg: &BorrowedMessage<'_>) -> SdkResult<Option<SolanaEvent>> {
        let _timer = sdk_metrics::Timer::new("consumer_internal_process_kafka_message");
        let topic = kafka_msg.topic();
        let payload = kafka_msg.payload().ok_or_else(|| Error::Processing("Kafka message has empty payload.".to_string()))?;

        trace!("StreamConsumer: Internally processing message. Topic: '{}', Partition: {}, Offset: {}, Size: {} bytes",
            topic, kafka_msg.partition(), kafka_msg.offset(), payload.len());

        #[cfg(feature = "metrics")] // Record raw message received from Kafka
        sdk_metrics::counter!("bitquery_sdk_kafka_messages_raw_total", 1, "topic" => topic.to_string());
        #[cfg(feature = "metrics")]
        sdk_metrics::histogram!("bitquery_sdk_kafka_message_raw_size_bytes", payload.len() as f64, "topic" => topic.to_string());

        // Decompress payload (assuming LZ4)
        let decompressed_payload = match decompress_lz4(payload) {
            Ok(dp) => dp,
            Err(e) => {
                error!("LZ4 decompression failed for message on topic '{}': {}", topic, e);
                return Err(Error::Other(format!("Compression error: {}", e))); // Wrap compression error
            }
        };
        let decompressed_bytes = Bytes::from(decompressed_payload); // `Bytes` for efficient Prost decoding

        // Parse Protobuf based on topic
        // The parse_* methods now return Result<SolanaEvent>, not Vec<SolanaEvent>.
        // This means they select one "primary" event from the message or return an error if none are suitable.
        let event_result: SdkResult<SolanaEvent> = match topic {
            "solana.transactions.proto" => {
                BlockMessage::decode(decompressed_bytes).map_err(Error::from)
                    .and_then(|msg| self.parse_block_message_content(msg)) // Renamed for clarity
            }
            "solana.tokens.proto" => { // This topic name was assumed in earlier versions
                TokenBlockMessage::decode(decompressed_bytes).map_err(Error::from)
                    .and_then(|msg| self.parse_token_message_content(msg)) // Renamed
            }
            "solana.dextrades.proto" => {
                DexParsedBlockMessage::decode(decompressed_bytes).map_err(Error::from)
                    .and_then(|msg| self.parse_dex_message_content(msg)) // Renamed
            }
            unknown_topic => {
                warn!("StreamConsumer: Received message from unhandled topic: '{}'. Skipping.", unknown_topic);
                return Ok(None); // Valid scenario, not an error. No event to produce.
            }
        };

        match event_result {
            Ok(event) => {
                // Deduplication check
                let mut seen_signatures_guard = self.seen_signatures_cache.lock().await;
                if !seen_signatures_guard.insert(event.signature().to_string()) { // `insert` returns false if value was already present
                    debug!("StreamConsumer: Duplicate signature '{}' detected. Skipping event.", event.signature());
                    #[cfg(feature = "metrics")]
                    sdk_metrics::counter!("bitquery_sdk_duplicate_events_filtered_total", 1, "event_type" => event.event_type().as_str().to_string());
                    return Ok(None); // Is a duplicate
                }

                // Manage deduplication cache size (simple eviction strategy)
                if seen_signatures_guard.len() > self.sdk_config.processing.buffer_size.max(100_000) {
                    warn!("StreamConsumer: Signature deduplication cache reached size {}. Clearing to manage memory.", seen_signatures_guard.len());
                    seen_signatures_guard.clear();
                    seen_signatures_guard.insert(event.signature().to_string()); // Re-add current event
                }

                // Successfully parsed and not a duplicate
                sdk_metrics::record_event_processed(event.event_type().as_str(), true);
                Ok(Some(event))
            }
            Err(e) => {
                // Error during parsing of a specific message type (e.g., block with no target transactions)
                // This is considered a processing error for this message.
                debug!("StreamConsumer: Failed to parse content for topic '{}': {}. This message will not yield an event.", topic, e);
                sdk_metrics::record_event_processed(topic, false); // Use topic as type if event type indeterminate
                Err(e) // Propagate the parsing error
            }
        }
    }

    // Renamed parse methods to `parse_*_content` to distinguish from any higher-level parse calls.
    // These now extract a single representative `SolanaEvent` from the Kafka message body.

    fn parse_block_message_content(&self, msg: BlockMessage) -> SdkResult<SolanaEvent> {
        if let Some(header) = msg.header {
            // Logic to select ONE transaction to represent this BlockMessage as a SolanaEvent.
            // The prompt's version implies taking the *first successful* transaction.
            for tx in msg.transactions {
                if tx.success { // Process only successful transactions
                    return Ok(SolanaEvent {
                        event_type: EventType::Transaction,
                        slot: header.slot as u64,
                        signature: tx.signature.clone(),
                        timestamp: header.block_time.clone(),
                        data: serde_json::json!({
                            "signer": tx.signer,
                            "fee": tx.fee,
                            "instructions_count": tx.instructions.len(),
                            "accounts_count": tx.accounts.len(),
                            "logs": tx.logs, // Caution: logs can be very large.
                        }),
                    });
                }
            }
        }
        // If no successful transaction was found or header was missing.
        Err(Error::Processing("No successful transaction found in BlockMessage to form a SolanaEvent.".to_string()))
    }

    fn parse_token_message_content(&self, msg: TokenBlockMessage) -> SdkResult<SolanaEvent> {
        if let Some(header) = msg.header {
            if let Some(transfer) = msg.transfers.first() { // Select the first transfer
                return Ok(SolanaEvent {
                    event_type: EventType::TokenTransfer,
                    slot: header.slot as u64,
                    signature: transfer.signature.clone(), // Assuming TokenTransfer has a signature
                    timestamp: transfer.block_time.clone(),
                    data: serde_json::json!({
                        "from_account": transfer.from,
                        "to_account": transfer.to,
                        "mint": transfer.mint,
                        "amount": transfer.amount.to_string(), // Convert amount to string for precision
                        "decimals": transfer.decimals,
                    }),
                });
            }
        }
        Err(Error::Processing("No transfers found in TokenBlockMessage to form a SolanaEvent.".to_string()))
    }

    fn parse_dex_message_content(&self, msg: DexParsedBlockMessage) -> SdkResult<SolanaEvent> {
        if let Some(header) = msg.header {
            if let Some(trade) = msg.trades.first() { // Select the first DEX trade
                return Ok(SolanaEvent {
                    event_type: EventType::DexTrade,
                    slot: header.slot as u64,
                    signature: trade.signature.clone(),
                    timestamp: trade.block_time.clone(),
                    data: serde_json::json!({
                        "program_id": trade.program_id, // Ensure SolanaEvent::program_id() uses this
                        "market_address": trade.market,
                        "side": trade.side,
                        "price": trade.price, // Price and amounts as strings for precision
                        "amount_base": trade.amount_base,
                        "amount_quote": trade.amount_quote,
                        "base_mint": trade.base_mint,
                        "quote_mint": trade.quote_mint,
                        "maker": trade.maker,
                        "taker": trade.taker,
                    }),
                });
            }
        }
        Err(Error::Processing("No DEX trades found in DexParsedBlockMessage to form a SolanaEvent.".to_string()))
    }
}
