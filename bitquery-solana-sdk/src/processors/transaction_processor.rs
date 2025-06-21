use crate::{
    events::{SolanaEvent, EventType},
    error::Result as SdkResult, // SDK's Result type
};
use super::EventProcessor; // The trait defined in processors/mod.rs
use async_trait::async_trait;
use tracing::{info, debug};

/// `TransactionProcessor` is a basic event processor for handling general `SolanaEvent`s
/// of type `Transaction`.
///
/// This processor primarily serves as an example. It logs information about each
/// transaction it processes. It can be extended for more specific transaction analysis,
/// filtering based on involved accounts, instruction types, or log messages.
#[derive(Default)] // Allows easy instantiation with `TransactionProcessor::default()`
pub struct TransactionProcessor;

#[async_trait]
impl EventProcessor for TransactionProcessor {
    /// Processes a `Transaction` event.
    ///
    /// This implementation logs key details of the transaction, such as its signature,
    /// slot, timestamp, signer, fee, and instruction count. It can be customized
    /// to perform more complex analysis or actions based on transaction content.
    async fn process(&self, event: &SolanaEvent) -> SdkResult<()> {
        // Safeguard: ensure this processor only handles Transaction events.
        // `should_process` should typically be called by the client/batch_processor first.
        if event.event_type() != EventType::Transaction {
            debug!(
                "TransactionProcessor.process called on a non-Transaction event (Type: {:?}, Sig: {}). Skipping.",
                event.event_type(), event.signature()
            );
            return Ok(());
        }

        // Extract common data for logging using SolanaEvent helper methods
        let signature = event.signature();
        let slot = event.slot();
        let timestamp = event.timestamp();

        // Access specific fields from the `event.data` JSON value
        // Using helpers like `get_data_string_field` for safe access.
        let signer = event.get_data_string_field("signer").unwrap_or("N/A");
        // Assuming these fields exist in `data` as per consumer.rs parsing logic
        let fee = event.get_data_u64_field("fee").unwrap_or(0);
        let instructions_count = event.get_data_u64_field("instructions_count").unwrap_or(0);
        let accounts_count = event.get_data_u64_field("accounts_count").unwrap_or(0);


        info!(
            "Transaction Processed: Sig: {}, Slot: {}, Timestamp: {}, Signer: {}, Fee: {}, Instructions: {}, Accounts: {}",
            signature, slot, timestamp, signer, fee, instructions_count, accounts_count
        );

        // Placeholder for more detailed transaction processing:
        // - Inspect `event.data.get("logs")` for specific messages.
        // - Check `event.data.get("instructions")` for particular program calls.
        // - Correlate with account data if available.
        debug!("Full transaction event data for {}: {:?}", signature, event.data);

        Ok(())
    }

    /// Determines if this `TransactionProcessor` should handle the given `SolanaEvent`.
    ///
    /// An event is considered processable if its `event_type` is `EventType::Transaction`.
    fn should_process(&self, event: &SolanaEvent) -> bool {
        event.event_type() == EventType::Transaction
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json; // For easily creating `SolanaEvent.data`

    // Helper to create a mock Transaction SolanaEvent
    fn mock_transaction_event(signature: &str, signer: &str, fee: u64) -> SolanaEvent {
        SolanaEvent {
            event_type: EventType::Transaction,
            slot: 123456,
            signature: signature.to_string(),
            timestamp: "2023-01-01T00:00:00Z".to_string(),
            data: json!({
                "signer": signer,
                "fee": fee,
                "instructions_count": 2, // Example value
                "accounts_count": 5,     // Example value
                "logs": ["Log entry 1", "Program execution successful"],
            }),
        }
    }

    #[test]
    fn test_transaction_processor_should_process_correct_type() {
        let processor = TransactionProcessor::default();
        let tx_event = mock_transaction_event("sig_tx_1", "SomeSigner", 5000);
        assert!(processor.should_process(&tx_event));
    }

    #[test]
    fn test_transaction_processor_should_not_process_incorrect_type() {
        let processor = TransactionProcessor::default();
        let dex_event = SolanaEvent { // Create a non-Transaction event
            event_type: EventType::DexTrade,
            slot: 1, signature: "sig_dex_1".to_string(), timestamp: "".to_string(), data: json!({}),
        };
        assert!(!processor.should_process(&dex_event));
    }

    #[tokio::test]
    async fn test_transaction_processor_process_method_runs() {
        let processor = TransactionProcessor::default();
        let tx_event = mock_transaction_event("sig_tx_process", "AnotherSigner", 10000);

        assert!(processor.should_process(&tx_event), "Event should meet criteria for processing");

        let result = processor.process(&tx_event).await;
        assert!(result.is_ok(), "process method should run successfully for a valid Transaction event");
    }

    #[tokio::test]
    async fn test_transaction_processor_process_skips_wrong_type_gracefully() {
        let processor = TransactionProcessor::default();
        let dex_event = SolanaEvent {
            event_type: EventType::DexTrade,
            slot: 1, signature: "sig_dex_skip".to_string(), timestamp: "".to_string(), data: json!({}),
        };

        // `should_process` would be false. This test calls `process` directly to check its internal guard.
        let result = processor.process(&dex_event).await;
        assert!(result.is_ok(), "process should return Ok for a non-matching event type (and do nothing)");
        // Further testing could involve log capture to ensure a debug message was logged about skipping.
    }
}
