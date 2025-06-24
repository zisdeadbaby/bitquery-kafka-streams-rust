use crate::{
    events::{SolanaEvent, EventType},
    error::Result as SdkResult, // Using the SDK's unified Result type
    // error::Error as SdkError, // Import SdkError if needed for creating specific errors
};
use super::EventProcessor; // The EventProcessor trait from processors/mod.rs
use async_trait::async_trait;
use tracing::{debug, trace}; // Logging utilities

/// `DexProcessor` is an `EventProcessor` specialized for handling `SolanaEvent`s
/// that represent Decentralized Exchange (DEX) trades.
///
/// It provides configurable filtering based on:
/// - Minimum USD value of the trade (calculated from `amount_base` and `price`).
/// - A list of target DEX program IDs.
///
/// This allows applications to focus on DEX trades of significant value or those
/// occurring on specific exchanges.
#[derive(Debug, Clone)] // Added Clone for flexibility if needed
pub struct DexProcessor {
    /// The minimum trade value in USD. Trades with an estimated USD value
    /// (amount_base * price) below this will not be processed by `should_process`.
    pub min_amount_usd: f64,
    /// A list of target DEX program IDs. If non-empty, only trades from these
    /// programs will pass the `should_process` check. If empty, trades from any
    /// DEX program (that provides the necessary data fields) can be processed,
    /// subject to other filter criteria.
    pub target_programs: Vec<String>,
}

impl Default for DexProcessor {
    /// Creates a `DexProcessor` with default settings:
    /// - `min_amount_usd`: 1000.0 (filters for trades valued over $1000 USD).
    /// - `target_programs`: Includes program IDs for common Solana DEXs like Raydium and Jupiter.
    fn default() -> Self {
        Self {
            min_amount_usd: 1000.0,
            target_programs: vec![
                // Pump.fun
                "6EF8rrecthR5Dkzon8Nwu78hRRfgKubJ14M5uBEwF6P".to_string(),
                // Raydium AMM V4
                "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(),
                // Raydium CLMM
                "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK".to_string(),
                // PumpSwap
                "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA".to_string(),
                // Orca Whirlpools
                "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc".to_string(),
                // Jupiter Aggregator V6
                "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string(),
            ],
        }
    }
}

#[async_trait]
impl EventProcessor for DexProcessor {
    /// Asynchronously processes a `SolanaEvent` identified as a DEX trade,
    /// provided it meets the criteria defined in `should_process`.
    ///
    /// This implementation logs detailed information about the processed trade.
    /// It can be extended to perform various actions, such as:
    /// - Storing trade data in a database or time-series store.
    /// - Triggering alerts or notifications for trades matching specific patterns.
    /// - Feeding data into further analytical or algorithmic trading systems.
    async fn process(&self, event: &SolanaEvent) -> SdkResult<()> {
        // This initial check is a safeguard. `should_process` is typically called by the SDK
        // framework (e.g., BitqueryClient or BatchProcessor) before invoking `process`.
        if !self.should_process(event) {
            // Log if called directly with an event that should have been filtered.
            debug!("DexProcessor.process invoked for an event that does not meet its criteria (Sig: {}). This is unexpected if called via SDK framework.", event.signature());
            return Ok(()); // Do nothing further.
        }

        // Extract relevant data using SolanaEvent helper methods for safety and clarity.
        let signature = event.signature();
        let program_id = event.program_id().unwrap_or("unknown_program");
        // `amount_base()` and `price()` are specific to DexTrade events, handled by `should_process`.
        let amount_base_val = event.amount_base().unwrap_or(0.0);
        let price_val = event.price().unwrap_or(0.0);
        // Example of accessing other fields that might be in `event.data` for DEX trades.
        let market_address = event.get_data_string_field("market_address").unwrap_or("N/A");
        let side = event.get_data_string_field("side").unwrap_or("N/A");

        let calculated_usd_value = if price_val > 0.0 { amount_base_val * price_val } else { 0.0 };

        // Enhanced terminal presentation for Pump.fun trades
        let is_pumpfun = program_id == "6EF8rrecthR5Dkzon8Nwu78hRvfgKubJ14M5uBEwF6P";
        
        // Update global statistics (simplified for demo)
        static TOTAL_EVENTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static PUMPFUN_TRADES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static OTHER_DEX_TRADES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static LARGE_TRADES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        
        TOTAL_EVENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        if is_pumpfun {
            PUMPFUN_TRADES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            OTHER_DEX_TRADES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        
        if calculated_usd_value > 1000.0 {
            LARGE_TRADES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        
        // Display statistics every 10 events
        if TOTAL_EVENTS.load(std::sync::atomic::Ordering::Relaxed) % 10 == 0 {
            let total = TOTAL_EVENTS.load(std::sync::atomic::Ordering::Relaxed);
            let pumpfun_count = PUMPFUN_TRADES.load(std::sync::atomic::Ordering::Relaxed);
            let other_count = OTHER_DEX_TRADES.load(std::sync::atomic::Ordering::Relaxed);
            let large_count = LARGE_TRADES.load(std::sync::atomic::Ordering::Relaxed);
            
            println!("\n📊 Quick Stats: {} total | 🚀 {} Pump.fun | 🔄 {} Other DEX | 🐋 {} Large\n", 
                total, pumpfun_count, other_count, large_count);
        }
        
        if is_pumpfun {
            // Special formatting for Pump.fun trades
            let side_emoji = match side.to_lowercase().as_str() {
                "buy" => "🟢 BUY",
                "sell" => "🔴 SELL",
                _ => "⚪ TRADE"
            };
            
            let value_emoji = if calculated_usd_value > 10000.0 { "💰" }
                            else if calculated_usd_value > 5000.0 { "💵" }
                            else if calculated_usd_value > 1000.0 { "💲" }
                            else { "🪙" };
            
            println!("\n┌─────────────────────────────────────────────────────────────────────────────────────");
            println!("│ 🚀 PUMP.FUN TRADE DETECTED {}", value_emoji);
            println!("├─────────────────────────────────────────────────────────────────────────────────────");
            println!("│ {} {} | Value: ${:.2} USD", side_emoji, value_emoji, calculated_usd_value);
            println!("│ 🏪 Market: {}", market_address);
            println!("│ 💎 Amount: {:.6} SOL", amount_base_val);
            println!("│ 💰 Price: ${:.8} USD", price_val);
            println!("│ 📋 Signature: {}", &signature[..8]);
            println!("│ ⏰ Time: {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
            println!("└─────────────────────────────────────────────────────────────────────────────────────\n");
        } else {
            // Standard formatting for other DEX trades
            let side_indicator = match side.to_lowercase().as_str() {
                "buy" => "📈",
                "sell" => "📉",
                _ => "🔄"
            };
            
            println!("🔹 {} DEX Trade | {} ${:.2} | Market: {} | Sig: {}", 
                side_indicator, side.to_uppercase(), calculated_usd_value, 
                &market_address[..8], &signature[..8]);
        }

        // Alert for large trades
        if calculated_usd_value > 1_000_000.0 {
            println!("\n🚨🚨🚨 WHALE ALERT! 🐋 🚨🚨🚨");
            println!("💥 MASSIVE TRADE: ${:.2} USD on {}", calculated_usd_value, 
                if is_pumpfun { "PUMP.FUN" } else { "DEX" });
            println!("🚨🚨🚨🚨🚨🚨🚨🚨🚨🚨🚨🚨🚨🚨\n");
        } else if calculated_usd_value > 100_000.0 {
            println!("🐋 Big Trade Alert: ${:.2} USD {}", calculated_usd_value, 
                if is_pumpfun { "on Pump.fun" } else { "" });
        }

        debug!("Full data for processed DEX trade (Sig: '{}'): {:?}", signature, event.data);

        Ok(())
    }

    /// Determines if this `DexProcessor` should handle the given `SolanaEvent`.
    ///
    /// An event is considered processable by this processor if it satisfies all of these conditions:
    /// 1. The `event.event_type()` must be `EventType::DexTrade`.
    /// 2. If `self.target_programs` is not empty, the event's program ID must be present in this list.
    /// 3. If `self.min_amount_usd` is greater than 0, the calculated USD value of the trade
    ///    (event.amount_base() * event.price()) must be greater than or equal to `min_amount_usd`.
    ///    This requires `amount_base` and `price` to be valid and present in the event data.
    fn should_process(&self, event: &SolanaEvent) -> bool {
        if event.event_type() != EventType::DexTrade {
            return false; // Must be a DEX trade event.
        }

        // Filter by target program IDs, if any are specified.
        if !self.target_programs.is_empty() {
            match event.program_id() { // Uses SolanaEvent::program_id() helper
                Some(event_program) => {
                    if !self.target_programs.iter().any(|target_prog_id| target_prog_id == event_program) {
                        return false; // Event's program ID is not in the target list.
                    }
                }
                None => {
                    // Event is a DexTrade but has no program_id, and we have specific targets.
                    return false;
                }
            }
        }

        // Filter by minimum USD value of the trade.
        if self.min_amount_usd > 0.0 { // Only apply this filter if a positive minimum is set.
            if let (Some(base_amount), Some(trade_price)) = (event.amount_base(), event.price()) {
                if trade_price <= 0.0 {
                    // Trades with zero or negative price are usually invalid or data errors.
                    // These cannot satisfy a positive min_amount_usd requirement.
                    // Log this situation if it's unexpected.
                    trace!("Trade (Sig: '{}') has non-positive price ({}), cannot meet min_amount_usd > 0. Skipping.", event.signature(), trade_price);
                    return false;
                }
                let trade_usd_value = base_amount * trade_price;
                if trade_usd_value < self.min_amount_usd {
                    return false; // Trade value is below the configured threshold.
                }
            } else {
                // `amount_base` or `price` (or both) are missing or not parsable from the event data.
                // Cannot determine USD value, so it cannot satisfy a positive min_amount_usd filter.
                trace!("Trade (Sig: '{}') is missing amount_base or price data; cannot apply min_amount_usd filter. Skipping.", event.signature());
                return false;
            }
        }

        // If all checks passed (or were not applicable for non-positive min_amount_usd).
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json; // For easily creating `SolanaEvent.data` for tests

    // Helper function to create a mock DexTrade SolanaEvent for testing
    fn create_mock_dex_event(program_id: &str, amount_base_str: &str, price_str: &str, signature: &str) -> SolanaEvent {
        SolanaEvent {
            event_type: EventType::DexTrade,
            slot: 1000, // Example slot
            signature: signature.to_string(),
            timestamp: "2024-01-15T12:00:00Z".to_string(), // Example timestamp
            data: json!({
                "program_id": program_id, // Field used by SolanaEvent::program_id()
                "amount_base": amount_base_str, // Field used by SolanaEvent::amount_base()
                "price": price_str, // Field used by SolanaEvent::price()
                "market_address": "MarketXYZ123", // Example other field
                "side": "BUY", // Example other field
            }),
        }
    }

    #[test]
    fn dex_processor_filters_by_event_type() {
        let processor = DexProcessor::default();
        let non_dex_event = SolanaEvent {
            event_type: EventType::Transaction, // Incorrect type
            slot: 1, signature: "sig_tx".to_string(), timestamp: "".to_string(), data: json!({}),
        };
        assert!(!processor.should_process(&non_dex_event));
    }

    #[test]
    fn dex_processor_filters_by_program_id() {
        let target_program = "TARGET_DEX_PROGRAM_ID".to_string();
        let processor = DexProcessor {
            min_amount_usd: 0.0, // Disable amount filtering for this test
            target_programs: vec![target_program.clone()],
        };

        let event_matching_program = create_mock_dex_event(&target_program, "10.0", "1.0", "sig_match_prog");
        assert!(processor.should_process(&event_matching_program));

        let event_other_program = create_mock_dex_event("SOME_OTHER_DEX_PROGRAM", "10.0", "1.0", "sig_other_prog");
        assert!(!processor.should_process(&event_other_program));
    }

    #[test]
    fn dex_processor_filters_by_min_amount_usd() {
        let processor = DexProcessor {
            min_amount_usd: 5000.0, // Target trades > $5000
            target_programs: vec![], // No program ID filter for this test
        };

        let event_above_threshold = create_mock_dex_event("any_prog", "100.0", "50.0", "sig_above"); // Value = 5000.0
        assert!(processor.should_process(&event_above_threshold));

        let event_at_threshold = create_mock_dex_event("any_prog", "250.0", "20.0", "sig_at"); // Value = 5000.0
        assert!(processor.should_process(&event_at_threshold));

        let event_below_threshold = create_mock_dex_event("any_prog", "99.0", "50.0", "sig_below"); // Value = 4950.0
        assert!(!processor.should_process(&event_below_threshold));

        // Test cases with data that prevents value calculation
        let event_missing_price = create_mock_dex_event("any_prog", "10000.0", "N/A", "sig_no_price"); // Price unparsable
        assert!(!processor.should_process(&event_missing_price));

        let event_zero_price = create_mock_dex_event("any_prog", "100000.0", "0.0", "sig_zero_price");
        assert!(!processor.should_process(&event_zero_price), "Zero price should not satisfy min_amount_usd > 0");
    }

    #[test]
    fn dex_processor_processes_if_target_programs_list_is_empty() {
        let processor = DexProcessor {
            min_amount_usd: 0.0, // No amount filter
            target_programs: vec![], // Empty list means accept any program
        };
        let event = create_mock_dex_event("ANY_PROGRAM_ID_XYZ", "1.0", "1.0", "sig_any_prog_accepted");
        assert!(processor.should_process(&event));
    }

    #[tokio::test]
    async fn dex_processor_process_method_executes_for_valid_event() {
        // Use default processor settings
        let processor = DexProcessor::default();
        // Create an event that matches default criteria (e.g., Raydium, >$1000)
        let event = create_mock_dex_event(
            &processor.target_programs[0], // A program from default target_programs
            "100.0",  // amount_base
            "15.0",   // price. Value = 1500.0, which is > default min_amount_usd (1000.0)
            "sig_process_test_dex"
        );

        assert!(processor.should_process(&event), "Event should meet default criteria for processing.");
        let result = processor.process(&event).await;
        assert!(result.is_ok(), "process method should execute successfully for a valid event. Error: {:?}", result.err());
        // Further tests could capture logs to verify output, but that requires more setup.
    }
}
