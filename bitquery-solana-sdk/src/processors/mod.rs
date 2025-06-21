//! Event processors for handling specific types of Solana events.
//!
//! Processors define logic for reacting to or transforming events consumed
//! from the Bitquery Kafka streams. Users can implement the `EventProcessor`
//! trait to create custom event handling routines, which can then be used
//! with the `BitqueryClient` directly or via the `BatchProcessor`.

// Re-export specific processor implementations for easier access.
mod dex_processor;
mod transaction_processor;

pub use dex_processor::DexProcessor;
pub use transaction_processor::TransactionProcessor;

use crate::events::SolanaEvent;
use crate::error::Result as SdkResult; // Use the SDK's unified Result type
use async_trait::async_trait;

/// A trait for asynchronous processing of `SolanaEvent`s.
///
/// Implementors of this trait can define custom logic to handle different
/// types of events, filter them, or trigger actions based on their content.
/// Processors are typically used in conjunction with the `BitqueryClient`'s
/// event stream or `BatchProcessor`.
#[async_trait]
pub trait EventProcessor: Send + Sync + 'static { // Added 'static bound for Arc<dyn EventProcessor>
    /// Processes a single `SolanaEvent`.
    ///
    /// This method is called by the consuming application (e.g., via `BitqueryClient`
    /// or `BatchProcessor`) when an event is received and `should_process`
    /// returns true for this processor.
    ///
    /// # Arguments
    /// * `event`: A reference to the `SolanaEvent` to be processed.
    ///
    /// # Returns
    /// An `SdkResult<()>` indicating success or failure of the processing step.
    async fn process(&self, event: &SolanaEvent) -> SdkResult<()>;

    /// Determines if this processor should handle the given event.
    ///
    /// This method allows processors to selectively act on events based on
    /// their type, content, or other criteria, before the more expensive `process`
    /// method is called.
    ///
    /// # Arguments
    /// * `event`: A reference to the `SolanaEvent` to be checked.
    ///
    /// # Returns
    /// `true` if the processor should process this event, `false` otherwise.
    fn should_process(&self, event: &SolanaEvent) -> bool;
}
