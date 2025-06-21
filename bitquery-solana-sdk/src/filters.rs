use crate::events::{SolanaEvent, EventType};
use serde::{Deserialize, Serialize};

/// `EventFilter` defines criteria for pre-filtering `SolanaEvent`s before
/// they undergo more intensive processing. This can help in reducing
/// the load on downstream processors by discarding irrelevant events early.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventFilter {
    /// If set, only events matching one of these types will pass.
    pub event_types: Option<Vec<EventType>>,
    /// If set, only events with a slot number greater than or equal to this value will pass.
    pub min_slot: Option<u64>,
    /// If set, only events with a slot number less than or equal to this value will pass.
    pub max_slot: Option<u64>,
    /// If set, only events associated with one of these program IDs will pass.
    /// This typically applies to `DexTrade` or `Transaction` events that have a clear program context.
    pub program_ids: Option<Vec<String>>,
    /// If set, only events (like trades or transfers) with an amount greater than
    /// or equal to this value will pass. The interpretation of "amount" depends
    /// on the `EventType`.
    pub min_amount: Option<f64>,
    /// If set, only events with a signature matching one of these will pass.
    pub signatures: Option<Vec<String>>,
    /// A custom filter function that provides maximum flexibility.
    /// If set, this function is called, and the event only passes if it returns `true`.
    #[serde(skip)] // Custom functions cannot be easily serialized/deserialized.
    pub custom_filter: Option<Box<dyn Fn(&SolanaEvent) -> bool + Send + Sync + 'static>>,
}

impl EventFilter {
    /// Checks if a given `SolanaEvent` matches the defined filter criteria.
    ///
    /// The event must pass all configured filter conditions to be considered a match.
    /// If a filter condition is not set (i.e., `None`), it is not applied.
    ///
    /// # Arguments
    /// * `event`: A reference to the `SolanaEvent` to check.
    ///
    /// # Returns
    /// `true` if the event matches all filter criteria, `false` otherwise.
    pub fn matches(&self, event: &SolanaEvent) -> bool {
        // Event type filter
        if let Some(types) = &self.event_types {
            if !types.is_empty() && !types.contains(&event.event_type()) {
                return false;
            }
        }

        // Slot range filter
        if let Some(min) = self.min_slot {
            if event.slot() < min {
                return false;
            }
        }

        if let Some(max) = self.max_slot {
            if event.slot() > max {
                return false;
            }
        }

        // Program ID filter
        if let Some(programs) = &self.program_ids {
            if !programs.is_empty() { // Only apply if program_ids filter is non-empty
                if let Some(event_program_id) = event.program_id() { // Assuming SolanaEvent has a program_id() helper
                    if !programs.iter().any(|p| p == event_program_id) {
                        return false;
                    }
                } else {
                    // Event doesn't have a program_id, but filter expects one.
                    return false;
                }
            }
        }

        // Amount filter - interpretation depends on event type
        if let Some(min_amount_filter) = self.min_amount {
            let event_amount: Option<f64> = match event.event_type() {
                EventType::DexTrade => event.amount_base_as_f64(), // Assuming amount_base() returns Option<f64>
                EventType::TokenTransfer => {
                    // Assuming amount is stored as u64 in data for TokenTransfer
                    event.get_data_u64_field("amount").map(|a| a as f64)
                }
                _ => None, // Other event types don't have a standard "amount" for this filter
            };

            if let Some(actual_amount) = event_amount {
                if actual_amount < min_amount_filter {
                    return false;
                }
            } else {
                // If min_amount filter is set, but event type doesn't have a comparable amount,
                // it implies it shouldn't pass this specific condition unless amount is irrelevant for type.
                // Depending on strictness, could return false or true.
                // Current: if amount filter is set, event must have comparable amount or it doesn't pass.
                if event.event_type() == EventType::DexTrade || event.event_type() == EventType::TokenTransfer {
                    return false; // Amount filter set, but event of relevant type has no amount.
                }
            }
        }

        // Signature filter
        if let Some(signatures) = &self.signatures {
            if !signatures.is_empty() && !signatures.contains(event.signature()) {
                return false;
            }
        }

        // Custom filter
        if let Some(filter_fn) = &self.custom_filter {
            if !filter_fn(event) {
                return false;
            }
        }

        true // All checks passed
    }
}

/// `FilterBuilder` provides a fluent API for constructing `EventFilter` instances.
#[derive(Default)]
pub struct FilterBuilder {
    filter: EventFilter,
}

impl FilterBuilder {
    /// Creates a new `FilterBuilder` with an empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the event types to filter by.
    /// Only events matching one of the specified types will pass.
    pub fn event_types(mut self, types: Vec<EventType>) -> Self {
        self.filter.event_types = Some(types);
        self
    }

    /// Sets a minimum slot number (inclusive).
    pub fn min_slot(mut self, min_slot: u64) -> Self {
        self.filter.min_slot = Some(min_slot);
        self
    }

    /// Sets a maximum slot number (inclusive).
    pub fn max_slot(mut self, max_slot: u64) -> Self {
        self.filter.max_slot = Some(max_slot);
        self
    }

    /// Sets the program IDs to filter by.
    /// Only events associated with one of these program IDs will pass.
    pub fn program_ids(mut self, ids: Vec<String>) -> Self {
        self.filter.program_ids = Some(ids);
        self
    }

    /// Sets a minimum amount for events like trades or transfers.
    pub fn min_amount(mut self, amount: f64) -> Self {
        self.filter.min_amount = Some(amount);
        self
    }

    /// Sets specific transaction signatures to filter by.
    pub fn signatures(mut self, signatures: Vec<String>) -> Self {
        self.filter.signatures = Some(signatures);
        self
    }

    /// Adds a custom filter function.
    /// The event must satisfy this function (i.e., function returns `true`) to pass.
    pub fn custom<F>(mut self, f: F) -> Self
    where
        F: Fn(&SolanaEvent) -> bool + Send + Sync + 'static, // Ensure closure is Send + Sync
    {
        self.filter.custom_filter = Some(Box::new(f));
        self
    }

    /// Builds and returns the configured `EventFilter`.
    pub fn build(self) -> EventFilter {
        self.filter
    }
}
