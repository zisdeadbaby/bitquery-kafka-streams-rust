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
        attempts: usize,
        #[source] // Enables `err.source()` to get the underlying error
        source: Box<Error> // Boxed to avoid Error enum being infinitely sized if it contains itself directly
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
    /// A helper function to create an `Error::RetryExhausted`.
    /// It takes the number of attempts and the last error encountered, boxing the last error.
    pub fn retry_exhausted(attempts: usize, last_error: Error) -> Self {
        Error::RetryExhausted { attempts, source: Box::new(last_error) }
    }
}

// `From` implementations for `async_channel` errors, converting them into `Error::ChannelError`.
// This allows `?` operator to be used conveniently with channel operations.
impl<T> From<async_channel::SendError<T>> for Error {
    fn from(err: async_channel::SendError<T>) -> Self {
        Error::ChannelError(format!("Async channel send error: {}", err))
    }
}

impl From<async_channel::RecvError> for Error {
    fn from(err: async_channel::RecvError) -> Self {
        Error::ChannelError(format!("Async channel receive error: {}", err))
    }
}

// `From` implementations for `crossbeam_channel` errors, if it's used.
// The new `Cargo.toml` includes `crossbeam-channel`.
impl<T> From<crossbeam_channel::SendError<T>> for Error {
    fn from(err: crossbeam_channel::SendError<T>) -> Self {
        Error::ChannelError(format!("Crossbeam channel send error: {}", err))
    }
}

impl From<crossbeam_channel::RecvError> for Error {
    fn from(err: crossbeam_channel::RecvError) -> Self {
        Error::ChannelError(format!("Crossbeam channel receive error: {}", err))
    }
}

// Note: The `CircuitBreakerOpen` error variant was present in a previous iteration.
// The new "Enhanced SDK" design uses `ResourceManager` and its backpressure mechanism
// instead of a separate `CircuitBreaker` utility. So, `CircuitBreakerOpen` is removed.
// Resource-related unavailability is now signaled by `Error::ResourceError`
// (e.g., from `ResourceManager::check_resources`).
