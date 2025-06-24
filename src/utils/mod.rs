//! Utility modules for the Kafka SDK.
//!
//! This module re-exports utilities from the core module
//! and provides SDK-specific utilities.

// Re-export utilities from core module
pub use crate::core::utils::*;

// Kafka SDK-specific utilities 
pub mod metrics;
