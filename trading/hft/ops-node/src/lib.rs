// Main library file for ops-node

// Declare modules that should be part of the library's public API or internal structure.
pub mod benchmarks;
pub mod client;
pub mod config;
pub mod error;
pub mod grpc_client; // Contains generated proto code, might not need to be pub itself if grpc_manager is the interface
pub mod memory;
pub mod network; // Network utilities, might be internal or pub depending on use case
pub mod state;
pub mod strategies;
pub mod tx_sender;

// Re-export key types and functions for easier use by the binary or other consumers.
pub use client::OpsNodeClient;
pub use config::Config;
pub use error::{OpsNodeError, Result as OpsNodeResult}; // Renamed Result to OpsNodeResult for clarity
pub use strategies::TradingStrategy; // Base trait for strategies

// Re-export commonly used Solana types if they are frequently needed by users of this library.
pub use solana_sdk::pubkey::Pubkey;
pub use solana_sdk::signature::{Signature, Keypair};
pub use solana_sdk::signer::Signer; // Re-export Signer trait

// Application version and name, derived from Cargo.toml at compile time.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

// Optional: A function to print version information.
pub fn print_version_info() {
    println!("{} v{}", APP_NAME, APP_VERSION);
    // Can add more build info here if needed, e.g., git commit hash, build date.
    // For example, using the `vergen` crate can embed more detailed build info.
}

// Global setup function (example)
// This could be used to initialize things that are needed before the client starts,
// like global allocators, logging, or other environment setup, though much of this
// is often handled in main.rs.
// pub fn setup_global_environment() -> OpsNodeResult<()> {
//     // E.g., Initialize a global logger if not done by tracing-subscriber in main
//     // Or set up a global panic hook
//     Ok(())
// }

// Feature flags check (example, if you want to log active features)
pub fn log_active_features() {
    tracing::info!("Active compile-time features:");
    #[cfg(feature = "io_uring")]
    tracing::info!("- io_uring: enabled");
    #[cfg(not(feature = "io_uring"))]
    tracing::info!("- io_uring: disabled");

    #[cfg(feature = "af_xdp")]
    tracing::info!("- af_xdp: enabled");
    #[cfg(not(feature = "af_xdp"))]
    tracing::info!("- af_xdp: disabled");

    #[cfg(all(target_arch = "x86_64", feature = "tsc"))]
    tracing::info!("- tsc: enabled (on x86_64)");
    #[cfg(not(all(target_arch = "x86_64", feature = "tsc")))]
    tracing::info!("- tsc: disabled or not on x86_64");
}
