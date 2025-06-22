//! Event processors for handling specific types of Solana events.
//!
//! Processors define logic for reacting to or transforming events consumed
//! from the Bitquery Kafka streams. Users can implement the `EventProcessor`
//! trait to create custom event handling routines, which can then be used
//! with the `BitqueryClient` directly or via the `BatchProcessor`.

// Re-export specific processor implementations for easier access by SDK users.
mod dex_processor;
mod transaction_processor;

pub use dex_processor::DexProcessor;
pub use transaction_processor::TransactionProcessor;

use crate::events::SolanaEvent;
use crate::error::Result as SdkResult; // Use the SDK's unified Result type: crate::error::Result
use async_trait::async_trait; // For enabling async methods in traits

/// `EventProcessor` is a trait for defining asynchronous logic to process `SolanaEvent`s.
///
/// Implementors of this trait can create custom handlers for different types of
/// Solana events, allowing for flexible event filtering, transformation, analysis,
/// or triggering of external actions based on event content.
///
/// Processors must be `Send + Sync + 'static` to be safely shared across threads,
/// especially when used with `Arc<dyn EventProcessor>` in components like `BatchProcessor`.
#[async_trait]
pub trait EventProcessor: Send + Sync + 'static {
    /// Asynchronously processes a single `SolanaEvent`.
    ///
    /// This method is invoked by the SDK (e.g., by `BitqueryClient` in direct mode,
    /// or by a worker in `BatchProcessor`) for events that this processor
    /// has indicated it should handle (via the `should_process` method).
    ///
    /// # Arguments
    /// * `event`: A reference to the `SolanaEvent` to be processed.
    ///
    /// # Returns
    /// An `SdkResult<()>`: `Ok(())` if processing was successful, or an `Error`
    /// if processing failed. This result influences error handling and metrics.
    async fn process(&self, event: &SolanaEvent) -> SdkResult<()>;

    /// Determines whether this processor should attempt to process the given `SolanaEvent`.
    ///
    /// This method acts as a preliminary filter, allowing processors to quickly decide
    /// if an event is relevant to their specific logic, before the potentially more
    /// resource-intensive `process` method is called.
    ///
    /// # Arguments
    /// * `event`: A reference to the `SolanaEvent` to be evaluated.
    ///
    /// # Returns
    /// `true` if this processor should attempt to process the event, `false` otherwise.
    fn should_process(&self, event: &SolanaEvent) -> bool;
}
