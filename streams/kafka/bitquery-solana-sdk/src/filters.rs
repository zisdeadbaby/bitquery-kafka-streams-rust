use crate::events::{SolanaEvent, EventType};
use serde::{Deserialize, Serialize}; // For potential serialization of filter configurations

/// `EventFilter` defines a set of criteria to selectively process `SolanaEvent`s.
///
/// This structure allows for pre-filtering events before they are passed to more
/// computationally intensive processing stages. Filters can be combined to create
/// specific views of the blockchain data stream.
///
/// All filter conditions are optional. If a condition is `None`, it is not applied.
/// For an event to pass the filter, it must satisfy all specified (non-`None`) conditions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventFilter {
    /// A list of `EventType`s. If set, the event's type must be one of these.
    pub event_types: Option<Vec<EventType>>,
    /// Minimum slot number (inclusive). If set, event's slot must be >= this value.
    pub min_slot: Option<u64>,
    /// Maximum slot number (inclusive). If set, event's slot must be <= this value.
    pub max_slot: Option<u64>,
    /// A list of program IDs. If set, the event's program ID (if applicable)
    /// must be one of these.
    pub program_ids: Option<Vec<String>>,
    /// Minimum amount. For `DexTrade`, this typically refers to `amount_base`.
    /// For `TokenTransfer`, it refers to the transfer amount.
    /// The interpretation is context-dependent based on `EventType`.
    pub min_amount: Option<f64>,
    /// A list of transaction signatures. If set, the event's signature must be one of these.
    pub signatures: Option<Vec<String>>,
    /// A user-defined custom filter function. If set, this function is executed,
    /// and the event passes only if the function returns `true`.
    /// This field is skipped during serialization/deserialization.
    #[serde(skip)]
    pub custom_filter: Option<Box<dyn Fn(&SolanaEvent) -> bool + Send + Sync + 'static>>,
}

impl EventFilter {
    /// Determines if a `SolanaEvent` matches all configured filter criteria.
    ///
    /// # Arguments
    /// * `event`: The `SolanaEvent` to evaluate against the filter.
    ///
    /// # Returns
    /// `true` if the event satisfies all active filter conditions, `false` otherwise.
    pub fn matches(&self, event: &SolanaEvent) -> bool {
        // Apply event type filter
        if let Some(ref types) = self.event_types {
            if !types.is_empty() && !types.contains(&event.event_type()) {
                return false;
            }
        }

        // Apply slot range filters
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

        // Apply program ID filter
        if let Some(ref programs) = self.program_ids {
            if !programs.is_empty() {
                match event.program_id() { // Uses SolanaEvent::program_id() helper
                    Some(event_program_id) => {
                        if !programs.iter().any(|p_id| p_id == event_program_id) {
                            return false;
                        }
                    }
                    None => {
                        // Event doesn't have a program_id, but filter expects one.
                        return false;
                    }
                }
            }
        }

        // Apply minimum amount filter
        if let Some(min_amount_val) = self.min_amount {
            let event_value_option: Option<f64> = match event.event_type() {
                EventType::DexTrade => event.amount_base(), // Uses SolanaEvent::amount_base()
                EventType::TokenTransfer => {
                    // Assumes "amount" is a u64 field in TokenTransfer's data that can be parsed to f64
                    event.get_data_u64_field("amount").map(|val| val as f64)
                }
                _ => None, // Other types don't have a standard "amount" for this filter
            };

            match event_value_option {
                Some(actual_event_amount) => {
                    if actual_event_amount < min_amount_val {
                        return false;
                    }
                }
                None => {
                    // If a min_amount filter is set, but the event type doesn't have a relevant amount
                    // or the amount is missing/invalid, then the event does not pass this condition.
                    // This applies only if the event is of a type where amount is relevant (DexTrade, TokenTransfer).
                    if event.event_type() == EventType::DexTrade || event.event_type() == EventType::TokenTransfer {
                        return false;
                    }
                }
            }
        }

        // Apply signature filter
        if let Some(ref sigs) = self.signatures {
            if !sigs.is_empty() && !sigs.contains(event.signature()) {
                return false;
            }
        }

        // Apply custom filter function
        if let Some(ref custom_fn) = self.custom_filter {
            if !custom_fn(event) {
                return false;
            }
        }

        // If all checks passed (or were not applicable)
        true
    }
}

/// `FilterBuilder` provides a fluent API for constructing `EventFilter` instances.
///
/// This builder pattern allows for a more readable and chainable way to define
/// complex filter criteria.
#[derive(Default)]
pub struct FilterBuilder {
    filter: EventFilter,
}

impl FilterBuilder {
    /// Creates a new `FilterBuilder` with default (empty) filter settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a filter for specific `EventType`s.
    /// If set, only events of these types will pass.
    pub fn event_types(mut self, types: Vec<EventType>) -> Self {
        self.filter.event_types = Some(types);
        self
    }

    /// Sets a minimum slot number for the filter (inclusive).
    pub fn min_slot(mut self, min_slot_val: u64) -> Self {
        self.filter.min_slot = Some(min_slot_val);
        self
    }

    /// Sets a maximum slot number for the filter (inclusive).
    pub fn max_slot(mut self, max_slot_val: u64) -> Self {
        self.filter.max_slot = Some(max_slot_val);
        self
    }

    /// Convenience method to set both minimum and maximum slot numbers.
    pub fn slot_range(mut self, min: u64, max: u64) -> Self {
        self.filter.min_slot = Some(min);
        self.filter.max_slot = Some(max);
        self
    }

    /// Adds a filter for specific program IDs.
    /// If set, only events associated with these program IDs will pass.
    pub fn program_ids(mut self, ids: Vec<String>) -> Self {
        self.filter.program_ids = Some(ids);
        self
    }

    /// Sets a minimum amount for the filter.
    /// Applicable to event types like `DexTrade` (amount_base) or `TokenTransfer`.
    pub fn min_amount(mut self, amount: f64) -> Self {
        self.filter.min_amount = Some(amount);
        self
    }

    /// Adds a filter for specific transaction signatures.
    /// If set, only events with these signatures will pass.
    pub fn signatures(mut self, signatures_list: Vec<String>) -> Self {
        self.filter.signatures = Some(signatures_list);
        self
    }

    /// Adds a custom, user-defined filter function.
    /// The event must satisfy this function (i.e., the function returns `true`) to pass.
    /// The function must be `Send + Sync + 'static`.
    pub fn custom<F>(mut self, custom_fn_impl: F) -> Self
    where
        F: Fn(&SolanaEvent) -> bool + Send + Sync + 'static,
    {
        self.filter.custom_filter = Some(Box::new(custom_fn_impl));
        self
    }

    /// Consumes the builder and returns the configured `EventFilter`.
    pub fn build(self) -> EventFilter {
        self.filter
    }
}
