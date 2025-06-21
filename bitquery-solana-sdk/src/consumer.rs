use crate::{
    config::Config as SdkConfig, // Aliased to avoid conflict with rdkafka types
    error::{Error, Result},
    events::{SolanaEvent, EventType},
    filters::EventFilter,
    resource_manager::ResourceManager,
    schemas::{solana::{BlockMessage, TokenBlockMessage, DexParsedBlockMessage}}, // Corrected path to generated types
    utils::{compression::decompress_lz4, metrics},
};
use bytes::Bytes;
use prost::Message as ProstMessage; // Alias to avoid conflict with rdkafka::Message
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer as RdKafkaStreamConsumer},
    message::Message as KafkaMessage, // Trait for Kafka messages
};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration; // Added for sleep
use tokio::sync::Mutex as TokioMutex; // Using tokio's Mutex for async/await compatibility
use tracing::{debug, error, warn, trace};

/// `StreamConsumer` wraps the `rdkafka::StreamConsumer` to provide a stream
/// of `SolanaEvent`s. It handles message deserialization, decompression,
/// deduplication, filtering, and integrates with `ResourceManager` for backpressure.
pub struct StreamConsumer {
    inner_consumer: Arc<RdKafkaStreamConsumer>,
    #[allow(dead_code)] // Config might be used for more things later
    sdk_config: Arc<SdkConfig>, // Keep the full SDK config accessible
    // Simplified deduplication: uses a HashSet to store seen signatures.
    // Note: This doesn't have a time window like the previous LRU deduplicator.
    // It will prevent processing any signature ever seen before until cache is cleared.
    seen_signatures: Arc<TokioMutex<HashSet<String>>>,
    event_filter: Arc<TokioMutex<Option<EventFilter>>>, // Filter is now optional and behind a Mutex
    resource_manager: Arc<ResourceManager>,
}

impl StreamConsumer {
    /// Creates a new `StreamConsumer`.
    pub fn new(
        rdkafka_consumer: RdKafkaStreamConsumer,
        sdk_config: SdkConfig, // Pass the whole Config
        resource_manager: Arc<ResourceManager>,
    ) -> Self {
        Self {
            inner_consumer: Arc::new(rdkafka_consumer),
            sdk_config: Arc::new(sdk_config),
            seen_signatures: Arc::new(TokioMutex::new(HashSet::with_capacity(100_000))), // Pre-allocate
            event_filter: Arc::new(TokioMutex::new(None)),
            resource_manager,
        }
    }

    /// Sets or updates the event filter for this consumer.
    pub async fn set_filter(&self, filter: EventFilter) {
        let mut current_filter = self.event_filter.lock().await;
        *current_filter = Some(filter);
        info!("Event filter has been set/updated for the StreamConsumer.");
    }

    /// Subscribes the underlying Kafka consumer to the specified topics.
    pub fn subscribe(&self, topics: &[String]) -> Result<()> {
        let topic_refs: Vec<&str> = topics.iter().map(|s| s.as_str()).collect();
        self.inner_consumer.subscribe(&topic_refs)
            .map_err(Error::Kafka)
    }

    /// Fetches and processes the next `SolanaEvent` from the Kafka stream.
    ///
    /// This method handles:
    /// - Backpressure checks via `ResourceManager`.
    /// - Receiving messages from Kafka.
    /// - Decompression and deserialization.
    /// - Deduplication of events by signature.
    /// - Applying the configured `EventFilter`.
    /// - Committing Kafka offsets.
    ///
    /// # Returns
    /// `Ok(Some(SolanaEvent))` if an event is successfully processed.
    /// `Ok(None)` if a message was skipped (e.g., duplicate, filtered out, or backpressure delay).
    /// `Err(Error)` if an unrecoverable error occurs (e.g., Kafka connection issue).
    pub async fn next_message(&self) -> Result<Option<SolanaEvent>> {
        // Loop to retry if a message is skipped (e.g. duplicate, filtered)
        loop {
            // 1. Check for backpressure before attempting to receive or process.
            if self.resource_manager.is_backpressure_active() {
                warn!("Backpressure active, delaying message consumption.");
                tokio::time::sleep(Duration::from_millis(
                    self.sdk_config.retry.initial_delay.as_millis().max(100) as u64
                )).await; // Use a short delay from config
                // Return Ok(None) to signal the caller that no event is available *right now* due to backpressure.
                // The caller can decide to retry or wait.
                return Ok(None);
            }

            // 2. Check general resources (this might also activate backpressure).
            // This check is important to prevent pulling messages if we are already near limits.
            if let Err(e) = self.resource_manager.check_resources().await {
                warn!("Resource check failed (pre-receive): {}. Delaying.", e);
                 tokio::time::sleep(Duration::from_millis(100)).await;
                return Ok(None); // Similar to backpressure, signal temporary unavailability.
            }

            // 3. Receive a message from Kafka.
            let kafka_message = match self.inner_consumer.recv().await {
                Ok(msg) => msg,
                Err(e) => {
                    error!("Kafka receive error: {}", e);
                    // This is likely a more serious Kafka issue, propagate it.
                    return Err(Error::Kafka(e));
                }
            };

            // 4. Process the Kafka message to potentially get a SolanaEvent.
            // The process_message method now handles decompression, parsing, and deduplication.
            // It returns Result<Option<SolanaEvent>> where None can mean duplicate.
            match self.process_kafka_message(&kafka_message).await {
                Ok(Some(event)) => {
                    // 5. Apply pre-filter if configured.
                    let filter_guard = self.event_filter.lock().await;
                    if let Some(ref active_filter) = *filter_guard {
                        if !active_filter.matches(&event) {
                            debug!("Event (Sig: {}) filtered out by pre-filter.", event.signature());
                            // Message was valid but filtered. Commit offset and continue to next message.
                            self.commit_kafka_message(&kafka_message)?;
                            continue; // Get next message
                        }
                    }
                    // Event passed all checks and filters.
                    self.commit_kafka_message(&kafka_message)?; // Commit before returning event
                    return Ok(Some(event));
                }
                Ok(None) => { // Message was processed but resulted in no event (e.g., duplicate, parse error handled inside)
                    // Offset should have been committed by process_kafka_message if it was a duplicate
                    // or if it was successfully parsed but yielded no event (e.g. block with no target txs).
                    // If process_kafka_message handled the commit for duplicates, great. Otherwise, commit here.
                    // The new prompt's process_message commits on successful parse before deduplication check.
                    // This is fine.
                    self.commit_kafka_message(&kafka_message)?; // Ensure commit for skipped valid messages.
                    debug!("Message processed but yielded no event (e.g. duplicate, non-target content). Continuing.");
                    continue; // Get next message
                }
                Err(e) => {
                    // An error occurred during message processing (decompression, protobuf parse).
                    // This message is likely problematic (poison pill).
                    // Log the error. Decide on error strategy: skip (commit) or halt.
                    // For now, log and skip by committing, to avoid blocking the consumer.
                    error!("Error processing Kafka message (Offset: {}): {}. Skipping.", kafka_message.offset(), e);
                    self.commit_kafka_message(&kafka_message)?; // Commit problematic message to skip
                    metrics::record_event_processed("unknown_message_processing_error", false);
                    continue; // Attempt to process the next message
                }
            }
        }
    }

    /// Helper to commit Kafka message offset.
    fn commit_kafka_message(&self, kafka_message: &rdkafka::message::BorrowedMessage<'_>) -> Result<()> {
        self.inner_consumer.commit_message(kafka_message, CommitMode::Async)
            .map_err(Error::Kafka)
    }

    /// Processes a single Kafka message: decompresses, decodes, and checks for duplicates.
    /// Returns `Ok(Some(SolanaEvent))` if successful and not a duplicate.
    /// Returns `Ok(None)` if it's a duplicate or if the message type is unknown/unhandled.
    /// Returns `Err(Error)` for parsing/decompression failures.
    async fn process_kafka_message(&self, kafka_msg: &rdkafka::message::BorrowedMessage<'_>) -> Result<Option<SolanaEvent>> {
        let _timer = metrics::Timer::new("kafka_message_processing_internal");
        let topic = kafka_msg.topic();
        let payload = kafka_msg.payload().ok_or_else(|| Error::Processing("Empty Kafka message payload".to_string()))?;

        trace!("Internal processing of message from topic: {}, partition: {}, offset: {}, size: {} bytes",
            topic, kafka_msg.partition(), kafka_msg.offset(), payload.len());

        #[cfg(feature = "metrics")]
        {
            metrics::counter!("bitquery_sdk_kafka_messages_received_total", 1, "topic" => topic.to_string());
            metrics::histogram!("bitquery_sdk_kafka_message_size_bytes", payload.len() as f64, "topic" => topic.to_string());
        }

        let decompressed_payload = decompress_lz4(payload)?;
        let decompressed_bytes = Bytes::from(decompressed_payload);

        // The new prompt's parse_..._message functions return Result<SolanaEvent>, not Vec.
        // This implies one event per Kafka message or a specific selection logic.
        let event_result = match topic {
            "solana.transactions.proto" => {
                BlockMessage::decode(decompressed_bytes)
                    .map_err(Error::from)
                    .and_then(|msg| self.parse_block_message(msg))
            }
            "solana.tokens.proto" => { // Assuming this topic for TokenBlockMessage
                TokenBlockMessage::decode(decompressed_bytes)
                    .map_err(Error::from)
                    .and_then(|msg| self.parse_token_message(msg))
            }
            "solana.dextrades.proto" => {
                DexParsedBlockMessage::decode(decompressed_bytes)
                    .map_err(Error::from)
                    .and_then(|msg| self.parse_dex_message(msg))
            }
            unknown_topic => {
                warn!("Received message from unknown topic: {}. Skipping.", unknown_topic);
                return Ok(None); // Not an error, just an unhandled topic.
            }
        };

        match event_result {
            Ok(event) => {
                // Deduplication using simple HashSet
                let mut seen_sigs_guard = self.seen_signatures.lock().await;
                if !seen_sigs_guard.insert(event.signature().to_string()) {
                    debug!("Duplicate signature detected: {}. Skipping event.", event.signature());
                    #[cfg(feature = "metrics")]
                    metrics::counter!("bitquery_sdk_duplicate_events_total", 1, "event_type" => event.event_type().as_str().to_string());
                    return Ok(None); // Duplicate
                }

                // Simple cache size management for seen_signatures
                if seen_sigs_guard.len() > self.sdk_config.processing.buffer_size.max(100_000) { // Use config buffer_size as rough guide
                    warn!("Seen signatures cache reached size {}. Clearing to save memory.", seen_sigs_guard.len());
                    seen_sigs_guard.clear();
                    // Re-insert current event as cache was just cleared
                    seen_sigs_guard.insert(event.signature().to_string());
                }
                metrics::record_event_processed(event.event_type().as_str(), true); // Record as successfully processed at this stage
                Ok(Some(event))
            }
            Err(e) => {
                // If parsing specific message type failed (e.g., "No valid transactions in block")
                // This is treated as a processing error for this specific message.
                // The caller (next_message) will decide how to handle it (e.g., skip and commit).
                 metrics::record_event_processed(topic, false); // Use topic as type if event type unknown
                Err(e)
            }
        }
    }

    // Parsing functions now return Result<SolanaEvent> directly,
    // implying they select one primary event or fail.

    fn parse_block_message(&self, msg: BlockMessage) -> Result<SolanaEvent> {
        // The prompt's version implies selecting the *first* successful transaction.
        if let Some(header) = msg.header {
            for tx in msg.transactions { // Iterate through transactions
                if tx.success {
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
                            "logs": tx.logs, // Be mindful of log size in JSON
                        }),
                    });
                }
            }
        }
        Err(Error::Processing("No successful/targetable transaction found in block".to_string()))
    }

    fn parse_token_message(&self, msg: TokenBlockMessage) -> Result<SolanaEvent> {
        if let Some(header) = msg.header {
            if let Some(transfer) = msg.transfers.first() { // Takes the first transfer
                return Ok(SolanaEvent {
                    event_type: EventType::TokenTransfer,
                    slot: header.slot as u64,
                    signature: transfer.signature.clone(), // Assuming signature is part of TokenTransfer
                    timestamp: transfer.block_time.clone(),
                    data: serde_json::json!({
                        "from_account": transfer.from,
                        "to_account": transfer.to,
                        "mint": transfer.mint,
                        "amount": transfer.amount.to_string(), // Amounts as strings for precision
                        "decimals": transfer.decimals,
                    }),
                });
            }
        }
        Err(Error::Processing("No token transfers found in message".to_string()))
    }

    fn parse_dex_message(&self, msg: DexParsedBlockMessage) -> Result<SolanaEvent> {
        if let Some(header) = msg.header {
            if let Some(trade) = msg.trades.first() { // Takes the first trade
                return Ok(SolanaEvent {
                    event_type: EventType::DexTrade,
                    slot: header.slot as u64,
                    signature: trade.signature.clone(),
                    timestamp: trade.block_time.clone(),
                    data: serde_json::json!({
                        "program_id": trade.program_id,
                        "market_address": trade.market,
                        "side": trade.side,
                        "price": trade.price,
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
        Err(Error::Processing("No DEX trades found in message".to_string()))
    }
}
