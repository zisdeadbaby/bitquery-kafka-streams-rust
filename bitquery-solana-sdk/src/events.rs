use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Defines the types of Solana events that the SDK can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// Represents a general Solana transaction.
    Transaction,
    /// Represents a transfer of SPL tokens.
    TokenTransfer,
    /// Represents a trade on a decentralized exchange (DEX).
    DexTrade,
    /// Represents an update to an account's balance.
    BalanceUpdate,
    // Add other specific event types here as the SDK evolves.
}

impl EventType {
    /// Get a string representation of the event type, useful for metrics or logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Transaction => "transaction",
            EventType::TokenTransfer => "token_transfer",
            EventType::DexTrade => "dex_trade",
            EventType::BalanceUpdate => "balance_update",
        }
    }
}

/// Represents a processed event from the Solana blockchain data streams.
/// This is the primary data structure users of the SDK will interact with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaEvent {
    /// The type of this event (e.g., Transaction, DexTrade).
    pub event_type: EventType,
    /// The blockchain slot number where this event occurred.
    pub slot: u64,
    /// The transaction signature associated with this event.
    pub signature: String,
    /// The timestamp of the block containing this event, typically in ISO 8601 format or Unix epoch string.
    pub timestamp: String,
    /// Additional data specific to the event type, structured as JSON.
    /// The content of `data` varies significantly based on `event_type`.
    pub data: Value,
}

impl SolanaEvent {
    /// Returns the transaction signature of the event.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Returns the slot number of the event.
    pub fn slot(&self) -> u64 {
        self.slot
    }

    /// Returns the timestamp string of the event.
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    /// Returns the `EventType` of the event.
    pub fn event_type(&self) -> EventType {
        self.event_type
    }

    // Convenience methods for common event types & data extraction
    // These make assumptions about the structure of `data` for specific event types.

    /// Checks if this event is a `DexTrade`.
    pub fn is_dex_trade(&self) -> bool {
        self.event_type == EventType::DexTrade
    }

    /// Checks if this event is a `Transaction`.
    pub fn is_transaction(&self) -> bool {
        self.event_type == EventType::Transaction
    }

    /// Checks if this event is a `TokenTransfer`.
    pub fn is_token_transfer(&self) -> bool {
        self.event_type == EventType::TokenTransfer
    }

    /// Attempts to get the program ID from the event's data.
    /// Typically relevant for `DexTrade` or `Transaction` events.
    /// Assumes the field name in `data` is "program_id" or "program".
    pub fn program_id(&self) -> Option<&str> {
        self.data.get("program_id")
            .or_else(|| self.data.get("program")) // Some contexts might use "program"
            .and_then(Value::as_str)
    }

    /// For `DexTrade` events, attempts to get the base token amount as `f64`.
    /// Assumes `data` contains an "amount_base" field that is a string parsable to `f64`.
    pub fn amount_base(&self) -> Option<f64> { // Renamed from amount_base_as_f64 for consistency
        if self.event_type == EventType::DexTrade {
            self.data.get("amount_base")?.as_str()?.parse().ok()
        } else {
            None
        }
    }

    /// For `DexTrade` events, attempts to get the price as `f64`.
    /// Assumes `data` contains a "price" field that is a string parsable to `f64`.
    pub fn price(&self) -> Option<f64> { // Renamed from price_as_f64
        if self.event_type == EventType::DexTrade {
            self.data.get("price")?.as_str()?.parse().ok()
        } else {
            None
        }
    }

    // Generic field accessors from `data: Value`

    /// Attempts to get a string field from the `data` JSON value.
    pub fn get_data_string_field(&self, field_name: &str) -> Option<&str> {
        self.data.get(field_name)?.as_str()
    }

    /// Attempts to get an unsigned integer (u64) field from the `data` JSON value.
    pub fn get_data_u64_field(&self, field_name: &str) -> Option<u64> {
        self.data.get(field_name)?.as_u64()
    }

    /// Attempts to get a signed integer (i64) field from the `data` JSON value.
    pub fn get_data_i64_field(&self, field_name: &str) -> Option<i64> {
        self.data.get(field_name)?.as_i64()
    }

    /// Attempts to get a float (f64) field from the `data` JSON value.
    /// Note: JSON numbers are often parsed as f64 by `serde_json`.
    pub fn get_data_f64_field(&self, field_name: &str) -> Option<f64> {
        self.data.get(field_name)?.as_f64()
    }

    /// Attempts to get a boolean field from the `data` JSON value.
    pub fn get_data_bool_field(&self, field_name: &str) -> Option<bool> {
        self.data.get(field_name)?.as_bool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_event_type_as_str() {
        assert_eq!(EventType::Transaction.as_str(), "transaction");
        assert_eq!(EventType::DexTrade.as_str(), "dex_trade");
    }

    #[test]
    fn test_solana_event_helpers() {
        let dex_event = SolanaEvent {
            event_type: EventType::DexTrade,
            slot: 100,
            signature: "sig123".to_string(),
            timestamp: "ts123".to_string(),
            data: json!({
                "program_id": "prog1",
                "amount_base": "123.45",
                "price": "1.2",
                "custom_field": "custom_value"
            }),
        };

        assert!(dex_event.is_dex_trade());
        assert!(!dex_event.is_transaction());
        assert_eq!(dex_event.signature(), "sig123");
        assert_eq!(dex_event.slot(), 100);
        assert_eq!(dex_event.program_id(), Some("prog1"));
        assert_eq!(dex_event.amount_base(), Some(123.45));
        assert_eq!(dex_event.price(), Some(1.2));
        assert_eq!(dex_event.get_data_string_field("custom_field"), Some("custom_value"));

        let tx_event = SolanaEvent {
            event_type: EventType::Transaction,
            slot: 101,
            signature: "sig456".to_string(),
            timestamp: "ts456".to_string(),
            data: json!({"fee": 5000}),
        };
        assert!(tx_event.is_transaction());
        assert_eq!(tx_event.get_data_u64_field("fee"), Some(5000));
        assert_eq!(tx_event.amount_base(), None); // Not a DexTrade
    }
}
