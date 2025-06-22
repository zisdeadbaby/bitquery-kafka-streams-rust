use thiserror::Error;
use solana_sdk::signature::Signature;
use tonic::Status;

#[derive(Error, Debug)]
pub enum OpsNodeError {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("Network error: {0}")]
    Network(#[from] std::io::Error),

    #[error("gRPC error: {0}")]
    Grpc(#[from] Status),

    #[error("Solana client error: {0}")]
    SolanaClient(#[from] solana_client::client_error::ClientError),

    #[error("Task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("Transaction failed: {signature}")]
    TransactionFailed { signature: Signature },

    #[error("Strategy error: {0}")]
    Strategy(String),

    #[error("Memory allocation error")]
    MemoryAllocation,

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Invalid market conditions: {0}")]
    InvalidMarketConditions(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Build error: {0}")]
    BuildError(String),

    #[error("Channel send error: {0}")]
    ChannelSendError(String),
}

pub type Result<T> = std::result::Result<T, OpsNodeError>;
