use crate::{
    events::{SolanaEvent, EventType},
    error::Result as SdkResult, // SDK's Result type
    // error::Error as SdkError, // Not directly used here, but good to have in mind for error creation
};
use super::EventProcessor; // The trait defined in processors/mod.rs
use async_trait::async_trait;
use tracing::{info, debug, warn};

/// `DexProcessor` specializes in processing `SolanaEvent`s related to DEX trades.
///
/// It can be configured to filter trades based on criteria such as minimum USD value
/// and specific DEX program IDs. This allows focusing on significant or relevant trades.
pub struct DexProcessor {
    /// The minimum trade amount in USD for an event to be processed.
    /// Trades with a calculated value (amount_base * price) below this threshold
    /// will be skipped by `should_process`.
    pub min_amount_usd: f64,
    /// A list of DEX program IDs. If this list is not empty, only trades originating
    /// from these programs will be processed. If empty, trades from any DEX program
    /// (that provides necessary data) can be processed, subject to other criteria.
    pub target_programs: Vec<String>,
}

impl Default for DexProcessor {
    /// Creates a `DexProcessor` with default settings:
    /// - `min_amount_usd`: 1000.0 (trades over $1000).
    /// - `target_programs`: Includes common DEX program IDs like Raydium and Jupiter.
    fn default() -> Self {
        Self {
            min_amount_usd: 1000.0,
            target_programs: vec![
                "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(), // Raydium Program ID
                "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB".to_string(),  // Jupiter Aggregator Program ID
            ],
        }
    }
}

#[async_trait]
impl EventProcessor for DexProcessor {
    /// Processes a DEX trade event if it meets the configured criteria.
    ///
    /// This implementation logs detailed information about the trade. It can be extended
    /// to perform other actions, such as triggering alerts, storing data, or executing
    /// further trading logic based on the event.
    async fn process(&self, event: &SolanaEvent) -> SdkResult<()> {
        // This check is a safeguard. Typically, `should_process` is called first by the client/batch_processor.
        if !self.should_process(event) {
            debug!("DexProcessor.process called on an event that should_process would filter. Sig: {}", event.signature());
            return Ok(());
        }

        // Extract data using helper methods from SolanaEvent for clarity and safety.
        let program_id = event.program_id().unwrap_or("unknown_program");
        let amount_base = event.amount_base().unwrap_or(0.0); // amount_base() is specific to DexTrade
        let price = event.price().unwrap_or(0.0); // price() is specific to DexTrade
        let signature = event.signature();
        let market_address = event.get_data_string_field("market_address").unwrap_or("N/A"); // Example of generic field access

        let calculated_usd_value = if price > 0.0 { amount_base * price } else { 0.0 };

        info!(
            "DEX Trade Processed: Sig: {}, Program: {}, Market: {}, AmountBase: {}, Price: {}, ApproxValueUSD: {:.2}",
            signature,
            program_id,
            market_address,
            amount_base,
            price,
            calculated_usd_value
        );

        // Placeholder for more complex logic:
        // - Store trade data in a database.
        // - Send notifications for large trades.
        // - Integrate with other financial analysis tools.
        debug!("Full DEX trade event data for {}: {:?}", signature, event.data);

        Ok(())
    }

    /// Determines if this `DexProcessor` should handle the given `SolanaEvent`.
    ///
    /// An event is considered processable if:
    /// 1. It is of `EventType::DexTrade`.
    /// 2. If `target_programs` is not empty, the event's program ID matches one in the list.
    /// 3. The trade's value (amount_base * price) is greater than or equal to `min_amount_usd`.
    ///    This check requires `amount_base` and `price` to be available and valid in the event data.
    fn should_process(&self, event: &SolanaEvent) -> bool {
        if event.event_type() != EventType::DexTrade {
            return false;
        }

        // Program ID filter
        if !self.target_programs.is_empty() {
            match event.program_id() {
                Some(event_program) => {
                    if !self.target_programs.iter().any(|target_prog| target_prog == event_program) {
                        return false; // Program not in the target list
                    }
                }
                None => return false, // Event has no program_id, but filter requires specific programs
            }
        }

        // Minimum amount in USD filter
        if self.min_amount_usd > 0.0 { // Only apply if a positive minimum is set
            if let (Some(amount), Some(price)) = (event.amount_base(), event.price()) {
                if price <= 0.0 {
                    // Trades with zero or negative price are usually invalid or data errors.
                    // Skip value calculation for these to avoid issues like division by zero if price is used elsewhere.
                    warn!("Trade {} has a non-positive price ({}), cannot calculate USD value for filtering. Skipping.", event.signature(), price);
                    return false;
                }
                let usd_value = amount * price;
                if usd_value < self.min_amount_usd {
                    return false; // Value is below the configured threshold
                }
            } else {
                // Amount or price data is missing or not parsable from the event.
                // Cannot determine USD value, so skip if min_amount_usd filter is active.
                debug!("Trade {} is missing amount_base or price, cannot apply min_amount_usd filter. Skipping.", event.signature());
                return false;
            }
        }

        true // Event meets all criteria for processing by this DexProcessor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json; // For easily creating `SolanaEvent.data`

    // Helper to create a mock DexTrade SolanaEvent
    fn mock_dex_event(program_id: &str, amount_base_str: &str, price_str: &str, signature: &str) -> SolanaEvent {
        SolanaEvent {
            event_type: EventType::DexTrade,
            slot: 123456,
            signature: signature.to_string(),
            timestamp: "2023-01-01T00:00:00Z".to_string(),
            data: json!({
                "program_id": program_id, // or "program" depending on SolanaEvent impl
                "amount_base": amount_base_str,
                "price": price_str,
                "market_address": "SomeMarketAddress",
                // other fields as needed by processor or event definition
            }),
        }
    }

    #[test]
    fn test_dex_processor_should_process_correct_type_and_program() {
        let target_program = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
        let processor = DexProcessor {
            min_amount_usd: 0.0, // Disable amount filtering for this test
            target_programs: vec![target_program.to_string()],
        };

        let event_correct = mock_dex_event(target_program, "10.0", "1.0", "sig_correct");
        assert!(processor.should_process(&event_correct));

        let event_wrong_program = mock_dex_event("OTHER_PROGRAM", "10.0", "1.0", "sig_wrong_prog");
        assert!(!processor.should_process(&event_wrong_program));

        let event_wrong_type = SolanaEvent {
            event_type: EventType::Transaction, // Not a DexTrade
            slot: 1, signature: "sig_tx".to_string(), timestamp: "".to_string(), data: json!({}),
        };
        assert!(!processor.should_process(&event_wrong_type));
    }

    #[test]
    fn test_dex_processor_should_process_min_amount_usd() {
        let processor = DexProcessor {
            min_amount_usd: 1000.0,
            target_programs: vec![], // Disable program filtering for this test
        };

        let event_above_min = mock_dex_event("any_prog", "100.0", "10.0", "sig_above"); // Value = 1000.0
        assert!(processor.should_process(&event_above_min));

        let event_at_min = mock_dex_event("any_prog", "50.0", "20.0", "sig_at"); // Value = 1000.0
        assert!(processor.should_process(&event_at_min));

        let event_below_min = mock_dex_event("any_prog", "99.0", "10.0", "sig_below"); // Value = 990.0
        assert!(!processor.should_process(&event_below_min));

        // Test cases with invalid data for amount/price
        let event_bad_amount = mock_dex_event("any_prog", "not_a_number", "10.0", "sig_bad_amount");
        assert!(!processor.should_process(&event_bad_amount));

        let event_bad_price = mock_dex_event("any_prog", "100.0", "not_a_price", "sig_bad_price");
        assert!(!processor.should_process(&event_bad_price));

        let event_zero_price = mock_dex_event("any_prog", "10000.0", "0.0", "sig_zero_price");
        assert!(!processor.should_process(&event_zero_price), "Trades with zero price should typically be filtered if min_amount_usd > 0");
    }

    #[test]
    fn test_dex_processor_no_target_programs() {
        let processor = DexProcessor {
            min_amount_usd: 0.0, // Disable amount filtering
            target_programs: vec![], // No specific programs targeted
        };
        let event = mock_dex_event("ANY_PROGRAM_ID", "1.0", "1.0", "sig_any_prog");
        assert!(processor.should_process(&event), "Should process if target_programs is empty and other criteria met");
    }

    #[tokio::test]
    async fn test_dex_processor_process_method_runs() {
        let processor = DexProcessor::default(); // Uses default target programs and min_amount_usd
        let event = mock_dex_event(
            &DexProcessor::default().target_programs[0], // Use a default target program
            "200.0", // amount_base
            "10.0",  // price. Value = 2000.0, which is > default min_amount_usd (1000.0)
            "sig_process_test"
        );

        assert!(processor.should_process(&event), "Event should meet default criteria for processing");
        let result = processor.process(&event).await;
        assert!(result.is_ok(), "process method should run successfully for a valid event");
    }
}
