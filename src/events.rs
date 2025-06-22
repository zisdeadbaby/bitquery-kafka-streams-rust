use serde::{Deserialize, Serialize};
use serde_json::Value; // For the flexible `data` field in SolanaEvent

/// `EventType` enumerates the different kinds of Solana blockchain events
/// that the SDK can parse and produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// Represents a general Solana transaction.
    Transaction,
    /// Represents a transfer of SPL (Solana Program Library) tokens.
    TokenTransfer,
    /// Represents a trade executed on a decentralized exchange (DEX).
    DexTrade,
    /// Represents an update to an account's balance (e.g., SOL or SPL token).
    /// Note: The current parsing logic in `consumer.rs` might not explicitly generate this
    /// type; balance updates are often part of `Transaction` data.
    BalanceUpdate,
}

impl EventType {
    /// Returns a static string representation of the event type.
    /// Useful for logging, metrics labels, or debugging.
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Transaction => "transaction",
            EventType::TokenTransfer => "token_transfer",
            EventType::DexTrade => "dex_trade",
            EventType::BalanceUpdate => "balance_update",
        }
    }
}

/// `SolanaEvent` is the core data structure representing a processed event
/// from the Bitquery Solana Kafka streams.
///
/// It contains common metadata like slot, signature, and timestamp,
/// along with a flexible `data` field (JSON Value) holding event-specific details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaEvent {
    /// The specific type of this event, as defined by `EventType`.
    pub event_type: EventType,
    /// The Solana blockchain slot number in which this event occurred.
    pub slot: u64,
    /// The transaction signature associated with this event.
    pub signature: String,
    /// The timestamp of the block containing this event. The format might vary
    /// (e.g., ISO 8601 string, Unix epoch string) based on the Kafka message source.
    pub timestamp: String,
    /// A `serde_json::Value` holding detailed, event-specific information.
    /// The structure of this JSON object depends on the `event_type`.
    pub data: Value,
}

impl SolanaEvent {
    // Accessor methods for common fields

    /// Returns the transaction signature of the event.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Returns the slot number where the event occurred.
    pub fn slot(&self) -> u64 {
        self.slot
    }

    /// Returns the timestamp string associated with the event's block.
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    /// Returns the `EventType` enum variant for this event.
    pub fn event_type(&self) -> EventType {
        self.event_type
    }

    // Convenience methods for type checking

    /// Returns `true` if this event is a `DexTrade`.
    pub fn is_dex_trade(&self) -> bool {
        self.event_type == EventType::DexTrade
    }

    /// Returns `true` if this event is a `Transaction`.
    pub fn is_transaction(&self) -> bool {
        self.event_type == EventType::Transaction
    }

    /// Returns `true` if this event is a `TokenTransfer`.
    pub fn is_token_transfer(&self) -> bool {
        self.event_type == EventType::TokenTransfer
    }

    // Helper methods for extracting common data fields from `data: Value`.
    // These methods provide convenient, type-safe access to nested JSON data,
    // assuming a known structure for certain event types.

    /// Attempts to get the program ID from the event's `data` field.
    /// Relevant for `DexTrade`, `Transaction` (instruction-level), etc.
    /// Checks for "program_id" and falls back to "program" for flexibility.
    pub fn program_id(&self) -> Option<&str> {
        self.data.get("program_id")
            .or_else(|| self.data.get("program")) // Check alternative field name "program"
            .and_then(Value::as_str)
    }

    /// For `DexTrade` events, attempts to get the base token amount as `f64`.
    /// Assumes `data` contains an "amount_base" field that is a string parsable to `f64`.
    pub fn amount_base(&self) -> Option<f64> {
        if self.event_type == EventType::DexTrade {
            self.data.get("amount_base")?.as_str()?.parse().ok()
        } else {
            None // Not applicable or different field for other types
        }
    }

    /// For `DexTrade` events, attempts to get the price as `f64`.
    /// Assumes `data` contains a "price" field that is a string parsable to `f64`.
    pub fn price(&self) -> Option<f64> {
        if self.event_type == EventType::DexTrade {
            self.data.get("price")?.as_str()?.parse().ok()
        } else {
            None // Not applicable for other types
        }
    }

    // Generic accessors for fields within the `data` JSON Value.

    /// Attempts to get a string value from the `data` field by its key.
    pub fn get_data_string_field(&self, field_name: &str) -> Option<&str> {
        self.data.get(field_name)?.as_str()
    }

    /// Attempts to get a `u64` value from the `data` field by its key.
    pub fn get_data_u64_field(&self, field_name: &str) -> Option<u64> {
        self.data.get(field_name)?.as_u64()
    }

    /// Attempts to get an `i64` value from the `data` field by its key.
    pub fn get_data_i64_field(&self, field_name: &str) -> Option<i64> {
        self.data.get(field_name)?.as_i64()
    }

    /// Attempts to get an `f64` value from the `data` field by its key.
    /// Note: `serde_json` parses all JSON numbers as `f64` initially if not further specified.
    pub fn get_data_f64_field(&self, field_name: &str) -> Option<f64> {
        self.data.get(field_name)?.as_f64()
    }

    /// Attempts to get a boolean value from the `data` field by its key.
    pub fn get_data_bool_field(&self, field_name: &str) -> Option<bool> {
        self.data.get(field_name)?.as_bool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json; // For easily creating `serde_json::Value`

    #[test]
    fn event_type_as_str_conversion() {
        assert_eq!(EventType::Transaction.as_str(), "transaction");
        assert_eq!(EventType::TokenTransfer.as_str(), "token_transfer");
        assert_eq!(EventType::DexTrade.as_str(), "dex_trade");
        assert_eq!(EventType::BalanceUpdate.as_str(), "balance_update");
    }

    #[test]
    fn solana_event_basic_accessors() {
        let event = SolanaEvent {
            event_type: EventType::DexTrade,
            slot: 12345,
            signature: "TestSignature123".to_string(),
            timestamp: "2023-10-26T10:00:00Z".to_string(),
            data: json!({"key": "value"}),
        };
        assert_eq!(event.event_type(), EventType::DexTrade);
        assert_eq!(event.slot(), 12345);
        assert_eq!(event.signature(), "TestSignature123");
        assert_eq!(event.timestamp(), "2023-10-26T10:00:00Z");
        assert!(event.is_dex_trade());
        assert!(!event.is_transaction());
    }

    #[test]
    fn solana_event_data_helpers_dex_trade() {
        let dex_event = SolanaEvent {
            event_type: EventType::DexTrade,
            slot: 100,
            signature: "sig_dex_trade".to_string(),
            timestamp: "ts_dex".to_string(),
            data: json!({
                "program_id": "dex_program_1",
                "amount_base": "150.75",
                "price": "2.5",
                "custom_string": "hello",
                "custom_u64": 123456789012345i64,
                "custom_bool": true
            }),
        };

        assert_eq!(dex_event.program_id(), Some("dex_program_1"));
        assert_eq!(dex_event.amount_base(), Some(150.75));
        assert_eq!(dex_event.price(), Some(2.5));
        assert_eq!(dex_event.get_data_string_field("custom_string"), Some("hello"));
        assert_eq!(dex_event.get_data_u64_field("custom_u64"), Some(123456789012345u64));
        assert_eq!(dex_event.get_data_bool_field("custom_bool"), Some(true));
        assert_eq!(dex_event.get_data_i64_field("non_existent"), None);
    }

    #[test]
    fn solana_event_data_helpers_token_transfer() {
        // Example for TokenTransfer, assuming "amount" is u64.
        // The EventFilter's amount check also assumes this.
        let transfer_event = SolanaEvent {
            event_type: EventType::TokenTransfer,
            slot: 200,
            signature: "sig_token_transfer".to_string(),
            timestamp: "ts_transfer".to_string(),
            data: json!({
                "programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", // Example using different casing
                "amount": 1000000000 // Example u64 amount
            }),
        };
        // Test program_id with alternative key "programId" (not specified in SolanaEvent::program_id() but good to be aware)
        // Current SolanaEvent::program_id() explicitly checks "program_id" then "program".
        // To match "programId", it would need modification or use get_data_string_field("programId").
        assert_eq!(transfer_event.get_data_string_field("programId"), Some("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"));
        assert_eq!(transfer_event.get_data_u64_field("amount"), Some(1000000000u64));

        // Test amount_base and price on non-DEX event
        assert_eq!(transfer_event.amount_base(), None);
        assert_eq!(transfer_event.price(), None);
    }
}
