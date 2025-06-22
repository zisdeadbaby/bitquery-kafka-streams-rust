use thiserror::Error;

/// SDK result type, wrapping the SDK's [`Error`] enum.
pub type Result<T> = std::result::Result<T, Error>;

/// SDK error types, categorized for clarity and handling.
#[derive(Error, Debug)]
pub enum Error {
    /// Errors originating from the underlying `rdkafka` library (e.g., connection, protocol issues).
    #[error("Kafka client error: {0}")]
    Kafka(#[from] rdkafka::error::KafkaError),

    /// Errors related to SDK configuration problems (e.g., invalid values, missing settings).
    #[error("Configuration error: {0}")]
    Config(String),

    /// General connection errors not covered by `rdkafka::KafkaError`.
    #[error("Connection error: {0}")]
    Connection(String),

    /// Errors during the decoding of Protocol Buffer messages.
    #[error("Protobuf decoding error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    /// Errors related to data compression or decompression (e.g., LZ4).
    #[error("Compression error: {0}")]
    Compression(String),

    /// Errors during Base58 encoding or decoding operations.
    #[error("Base58 encoding/decoding error: {0}")]
    Base58(#[from] bs58::decode::Error), // Covers bs58::decode::Error

    /// Errors encountered during event processing stages like filtering, batching, or custom logic.
    #[error("Event processing error: {0}")]
    Processing(String),

    /// Errors related to resource management, such as exceeding configured limits or backpressure activation.
    #[error("Resource management error: {0}")]
    ResourceError(String),

    /// Indicates that an operation failed after exhausting all configured retry attempts.
    /// The `source` field contains the last error that occurred.
    #[error("Operation failed after {attempts} retry attempts: {source}")]
    RetryExhausted {
        /// Number of retry attempts that were made before giving up
        attempts: usize,
        #[source] // Enables `err.source()` to get the underlying error
        /// The underlying error that caused the final failure
        source: Box<Error>, // Boxed to avoid Error enum being infinitely sized if it contains itself directly
    },

    /// Errors related to standard I/O operations (e.g., file access for SSL certs).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Errors from asynchronous channel operations (e.g., send/receive failures if a channel is closed or full).
    /// This is relevant for components like `BatchProcessor`.
    #[error("Async channel error: {0}")]
    ChannelError(String),

    /// A placeholder for any other types of errors not fitting into the specific categories above.
    #[error("An unexpected or other error occurred: {0}")]
    Other(String),
}

impl Error {
    /// Creates a Config error.
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Creates a Connection error.
    pub fn connection(msg: impl Into<String>) -> Self {
        Self::Connection(msg.into())
    }

    /// Creates a Compression error.
    pub fn compression(msg: impl Into<String>) -> Self {
        Self::Compression(msg.into())
    }

    /// Creates a Processing error.
    pub fn processing(msg: impl Into<String>) -> Self {
        Self::Processing(msg.into())
    }

    /// Creates a ResourceError.
    pub fn resource(msg: impl Into<String>) -> Self {
        Self::ResourceError(msg.into())
    }

    /// Creates a ChannelError.
    pub fn channel(msg: impl Into<String>) -> Self {
        Self::ChannelError(msg.into())
    }

    /// Creates an Other error.
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    /// Creates a RetryExhausted error.
    pub fn retry_exhausted(attempts: usize, source: Error) -> Self {
        Self::RetryExhausted {
            attempts,
            source: Box::new(source),
        }
    }
}

// Automatically convert various error types as needed.

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Processing(format!("JSON serialization/deserialization error: {}", err))
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::Other(format!("Task join error: {}", err))
    }
}
