use crate::{
    events::{SolanaEvent, EventType},
    error::Result as SdkResult, // Using the SDK's unified Result type
};
use super::EventProcessor; // The EventProcessor trait from processors/mod.rs
use async_trait::async_trait;
use tracing::{info, debug}; // Logging utilities

/// `TransactionProcessor` is a basic implementation of `EventProcessor` tailored for
/// handling general `SolanaEvent`s of type `Transaction`.
///
/// Its primary role in this default implementation is to log key information about
/// each transaction it processes. This serves as a foundational example that can be
/// extended for more sophisticated transaction analysis, such as:
/// - Filtering transactions based on involved accounts or programs.
/// - Analyzing instruction data or transaction logs for specific patterns.
/// - Storing transaction metadata for further querying or reporting.
#[derive(Debug, Default, Clone)] // Added Clone and Debug for flexibility
pub struct TransactionProcessor;

#[async_trait]
impl EventProcessor for TransactionProcessor {
    /// Asynchronously processes a `SolanaEvent` that has been identified as a `Transaction`.
    ///
    /// This method logs essential details of the transaction, including its signature,
    /// slot, timestamp, primary signer, fee, and counts of instructions and accounts.
    /// The full event data (which can be extensive) is logged at the DEBUG level.
    async fn process(&self, event: &SolanaEvent) -> SdkResult<()> {
        // Safeguard: Ensure this processor only acts on Transaction events.
        // This check is defensive; `should_process` should be the primary gate.
        if event.event_type() != EventType::Transaction {
            debug!(
                "TransactionProcessor.process was called with a non-Transaction event (Type: {:?}, Sig: {}). This is unexpected if called via SDK framework; skipping.",
                event.event_type(), event.signature()
            );
            return Ok(()); // Do nothing further for non-transaction events.
        }

        // Extract common transaction details using SolanaEvent helper methods.
        let signature = event.signature();
        let slot = event.slot();
        let timestamp = event.timestamp();

        // Access specific fields from the `event.data` JSON value using helpers.
        // These assume the field names used during event creation in `consumer.rs`.
        let signer = event.get_data_string_field("signer").unwrap_or("N/A");
        let fee = event.get_data_u64_field("fee").unwrap_or(0);
        let instructions_count = event.get_data_u64_field("instructions_count").unwrap_or(0);
        let accounts_count = event.get_data_u64_field("accounts_count").unwrap_or(0);

        info!(
            "Transaction Processed by TransactionProcessor: Sig='{}', Slot={}, Timestamp='{}', Signer='{}', Fee={}, Instructions={}, Accounts={}",
            signature, slot, timestamp, signer, fee, instructions_count, accounts_count
        );

        // For more detailed inspection, the full `event.data` is available.
        // Example: Log all transaction logs if present.
        // if let Some(logs) = event.data.get("logs").and_then(|v| v.as_array()) {
        //     for log_entry in logs {
        //         debug!("Log entry for {}: {}", signature, log_entry.as_str().unwrap_or(""));
        //     }
        // }
        debug!("Full data for processed transaction (Sig: '{}'): {:?}", signature, event.data);

        Ok(())
    }

    /// Determines if this `TransactionProcessor` should handle the given `SolanaEvent`.
    ///
    /// This processor is interested only in events where `event.event_type()` is
    /// `EventType::Transaction`.
    fn should_process(&self, event: &SolanaEvent) -> bool {
        event.event_type() == EventType::Transaction
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json; // For easily creating `SolanaEvent.data` in tests

    // Helper function to create a mock Transaction SolanaEvent for testing
    fn create_mock_transaction_event(signature: &str, signer: &str, fee: u64, instructions_count: u64) -> SolanaEvent {
        SolanaEvent {
            event_type: EventType::Transaction,
            slot: 5000, // Example slot
            signature: signature.to_string(),
            timestamp: "2024-01-15T14:00:00Z".to_string(), // Example timestamp
            data: json!({
                "signer": signer,
                "fee": fee,
                "instructions_count": instructions_count,
                "accounts_count": 3, // Example value
                "logs": ["Log 1", "Log 2: success"], // Example logs
            }),
        }
    }

    #[test]
    fn transaction_processor_should_process_only_transaction_events() {
        let processor = TransactionProcessor::default();

        let tx_event = create_mock_transaction_event("sig_tx_process_01", "SignerA", 5000, 2);
        assert!(processor.should_process(&tx_event), "Should process Transaction event type.");

        let dex_event = SolanaEvent { // Create a non-Transaction event
            event_type: EventType::DexTrade,
            slot: 1, signature: "sig_dex_other".to_string(), timestamp: "".to_string(), data: json!({}),
        };
        assert!(!processor.should_process(&dex_event), "Should NOT process DexTrade event type.");
    }

    #[tokio::test]
    async fn transaction_processor_process_method_executes_for_valid_event() {
        let processor = TransactionProcessor::default();
        let tx_event = create_mock_transaction_event("sig_tx_process_02", "SignerB", 10000, 5);

        // Pre-condition check (though `process` has its own guard)
        assert!(processor.should_process(&tx_event), "Event should meet criteria for processing by this processor.");

        let result = processor.process(&tx_event).await;
        assert!(result.is_ok(), "process method should execute successfully for a valid Transaction event. Error: {:?}", result.err());
        // Further testing could involve capturing logs to verify output, but this requires additional test setup.
    }

    #[tokio::test]
    async fn transaction_processor_process_method_skips_non_transaction_event_gracefully() {
        let processor = TransactionProcessor::default();
        let non_tx_event = SolanaEvent {
            event_type: EventType::TokenTransfer, // Not a Transaction
            slot: 2, signature: "sig_token_transfer_skip".to_string(), timestamp: "".to_string(), data: json!({}),
        };

        // `should_process` would return false for this event.
        // This test calls `process` directly to verify its internal type check/guard.
        let result = processor.process(&non_tx_event).await;
        assert!(result.is_ok(), "process should return Ok (and do nothing) for a non-Transaction event type.");
        // Log capture could verify that a debug message about skipping was emitted.
    }
}
