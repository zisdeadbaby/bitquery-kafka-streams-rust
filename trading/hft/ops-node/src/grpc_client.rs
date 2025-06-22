use crate::config::Config;
use crate::error::{OpsNodeError, Result};
use crate::memory::MarketDataPacket; // Assuming MarketDataPacket is relevant for callbacks
use futures::StreamExt;
use secrecy::{ExposeSecret, Secret};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tonic::metadata::{Ascii, MetadataValue};
use tonic::transport::{Channel, ClientTlsConfig, Endpoint}; // Added Endpoint
use tonic::{Request, Status};
// use tower::ServiceBuilder; // Not explicitly used in the provided snippet, can be added if interceptors are built with ServiceBuilder

// Import generated protobuf code.
// Include the generated files directly since they're in src/generated
pub mod proto {
    pub mod yellowstone {
        include!("generated/yellowstone.rs");
    }
    pub mod jito {
        include!("generated/jito.rs");
    }
}

// Using aliases for long generated type names
use proto::yellowstone::{
    geyser_client::GeyserClient, GetSlotRequest, SubscribeRequest,
    SubscribeRequestFilterAccounts, /* SubscribeRequestFilterAccountsFilter, SubscribeRequestFilterAccountsFilterOneof, */ // These seem unused
    CommitmentLevel, subscribe_update::UpdateOneof, // Added UpdateOneof for processing
};
use proto::jito::shred_stream_client::ShredStreamClient;
// use proto::jito::GetShredsRequest; // This seems unused

const USER_AGENT_PREFIX: &str = "ops-node";
const DEFAULT_KEEPALIVE_TIMEOUT_SECS: u64 = 30; // Increased keepalive
const DEFAULT_KEEPALIVE_INTERVAL_SECS: u64 = 10;
const DEFAULT_HTTP2_INITIAL_STREAM_WINDOW_SIZE: u32 = 4 * 1024 * 1024; // 4MB
const DEFAULT_HTTP2_INITIAL_CONNECTION_WINDOW_SIZE: u32 = 4 * 1024 * 1024; // 4MB


pub struct GrpcManager {
    // Each pool is now Arc<ConnectionPool<...>> to allow sharing the pool itself
    // if GrpcManager needs to be cloned or shared across threads directly.
    // However, GrpcManager itself is usually Arc-wrapped if shared.
    yellowstone_pool: ConnectionPool<GeyserClient<Channel>>,
    _jito_pool: ConnectionPool<ShredStreamClient<Channel>>,
    // API keys/tokens can be stored if they are static per manager instance
    // Or passed into methods if they can change per call.
    // For this structure, assuming they are tied to the manager's lifetime.
    yellowstone_api_key: Secret<String>, // Specific to Yellowstone
    _jito_auth_token: Option<Secret<String>>, // Specific to Jito
}

impl GrpcManager {
    pub async fn new(config: &Config) -> Result<Self> {
        let yellowstone_api_key = config.auth.yellowstone_token.clone();
        let jito_auth_token = config.auth.jito_auth_token.clone();

        // Create Yellowstone connection pool
        let yellowstone_pool = ConnectionPool::new(
            &config.network.yellowstone_endpoint,
            config.network.connection_pool_size,
            |channel| GeyserClient::new(channel), // No auth interceptor at pool level, applied per call
            "Yellowstone",
            config.network.request_timeout_ms,
        ).await?;

        // Create Jito connection pool
        let jito_pool = ConnectionPool::new(
            &config.network.jito_endpoint,
            config.network.connection_pool_size,
            |channel| ShredStreamClient::new(channel), // No auth interceptor at pool level
            "Jito",
            config.network.request_timeout_ms,
        ).await?;

        Ok(Self {
            yellowstone_pool,
            _jito_pool: jito_pool,
            yellowstone_api_key,
            _jito_auth_token: jito_auth_token,
        })
    }

    // Example: Subscribe to Yellowstone account updates
    pub async fn subscribe_to_yellowstone_accounts(
        &self, // Changed to &self as it doesn't need to modify manager state
        accounts_to_monitor: std::collections::HashMap<String, SubscribeRequestFilterAccounts>,
        callback: impl Fn(MarketDataPacket) + Send + Sync + 'static + Clone, // Clone for multiple streams if needed
    ) -> Result<()> {
        // In a pooled setup, you might want to run subscriptions on multiple connections
        // for redundancy or load balancing. Here, we'll use one client from the pool.
        let mut client = self.yellowstone_pool.get_client().clone(); // Clone client for this task

        let token_metadata = create_auth_metadata(self.yellowstone_api_key.expose_secret())?;

        let subscription_request_inner = SubscribeRequest {
            accounts: accounts_to_monitor,
            commitment: Some(CommitmentLevel::Processed as i32), // Example commitment
            // Populate other fields as necessary, e.g., slots, transactions, blocks
            slots: Default::default(),
            transactions: Default::default(),
            blocks: Default::default(),
            blocks_meta: Default::default(),
            entry: Default::default(), // Added entry
            ping: None, // Added ping
            commitment_v2: None, // Added commitment_v2
        };

        let mut request = Request::new(subscription_request_inner);
        request.metadata_mut().insert("x-token", token_metadata); // Common header for Yellowstone

        let stream = client.subscribe(request).await?.into_inner();

        tokio::spawn(async move {
            let mut stream = stream; // Move stream into the spawned task
            while let Some(message_result) = stream.next().await {
                match message_result {
                    Ok(update) => {
                        // Process based on the type of update
                        if let Some(ref update_oneof) = update.update_oneof {
                            match update_oneof {
                                UpdateOneof::Account(account_update) => {
                                    // Example: Convert to MarketDataPacket
                                    // This requires parsing account_update.account.data, which is specific to the account type
                                    let key_array: [u8; 32] = account_update.pubkey.as_slice().try_into().unwrap_or_default();

                                    let packet = MarketDataPacket {
                                        timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
                                        slot: account_update.slot,
                                        price: 0, // TODO: Extract from account_update.account.data
                                        volume: 0, // TODO: Extract from account_update.account.data
                                        account_key: key_array,
                                    };
                                    callback(packet);
                                }
                                UpdateOneof::Slot(slot_info) => {
                                    tracing::trace!("Received slot update via account stream: {:?}", slot_info);
                                    // Handle slot updates if necessary, or use dedicated slot stream
                                }
                                UpdateOneof::Pong(_) => {
                                    tracing::trace!("Received ping on Yellowstone stream");
                                    // Respond to ping or handle keepalive
                                }
                                // Handle other UpdateOneof variants (Transaction, Block, etc.)
                                _ => {
                                    tracing::trace!("Received unhandled update type on Yellowstone stream: {:?}", update.update_oneof);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Yellowstone account subscription stream error: {}. Attempting to resubscribe might be needed.", e);
                        // Implement reconnection logic here if desired
                        break; // Exit loop on error, or implement retry
                    }
                }
            }
            tracing::info!("Yellowstone account subscription stream ended.");
        });

        Ok(())
    }

    // Example: Subscribe to Yellowstone slot updates
    pub async fn subscribe_to_yellowstone_slots(
        &self, // Changed to &self
        callback: impl Fn(u64) + Send + Sync + 'static + Clone,
    ) -> Result<()> {
        let mut client = self.yellowstone_pool.get_client().clone();
        let token_metadata = create_auth_metadata(self.yellowstone_api_key.expose_secret())?;

        let _request = Request::new(GetSlotRequest {}); // This is for unary GetSlot, not streaming
        // For streaming slots, you'd typically use the SubscribeRequest with a filter for slots.
        // The proto definition for GetSlot does not seem to indicate a streaming response.
        // Assuming the intention was to use the Subscribe stream for slots:
        let slot_subscription = SubscribeRequest {
            slots: Default::default(), // Enable slot updates
             // Set other filters to empty or specific values if needed
            accounts: Default::default(),
            transactions: Default::default(),
            blocks: Default::default(),
            blocks_meta: Default::default(),
            entry: Default::default(),
            commitment: Some(CommitmentLevel::Processed as i32),
            ping: None,
            commitment_v2: None,
        };
        let mut stream_request = Request::new(slot_subscription);
        stream_request.metadata_mut().insert("x-token", token_metadata.clone());

        let stream = client.subscribe(stream_request).await?.into_inner();

        tokio::spawn(async move {
            let mut stream = stream;
            while let Some(message_result) = stream.next().await {
                match message_result {
                    Ok(update) => {
                        if let Some(UpdateOneof::Slot(slot_info)) = update.update_oneof {
                             callback(slot_info.slot);
                        } else if let Some(UpdateOneof::Pong(_)) = update.update_oneof {
                            tracing::trace!("Received ping on slot stream");
                        }
                    }
                    Err(e) => {
                        tracing::error!("Yellowstone slot subscription stream error: {}", e);
                        break;
                    }
                }
            }
            tracing::info!("Yellowstone slot subscription stream ended.");
        });

        Ok(())
    }

    // Add methods for Jito ShredStream client if needed
    // pub async fn get_jito_shreds(&self, ...) -> Result<...> {
    //     let mut client = self.jito_pool.get_client().clone();
    //     // ... similar logic for request, auth, and call
    // }
}

fn create_auth_metadata(token_str: &str) -> Result<MetadataValue<Ascii>> {
    MetadataValue::try_from(token_str)
        .map_err(|e| OpsNodeError::Config(config::ConfigError::Message(
            format!("Invalid API key/token format for metadata: {}", e))
        ))
}


struct ConnectionPool<T: Clone> { // Added T: Clone constraint
    clients: Vec<T>,
    current_idx: AtomicUsize, // Renamed for clarity
    _pool_name: String, // For logging
}

impl<T: Clone> ConnectionPool<T> {
    async fn new<F>(
        endpoint_str: &str,
        size: usize,
        client_factory: F,
        pool_name: &str,
        connect_timeout_ms: u64,
    ) -> Result<Self>
    where
        F: Fn(Channel) -> T,
    {
        if size == 0 {
            return Err(OpsNodeError::Config(config::ConfigError::Message(format!("Connection pool size for {} cannot be 0", pool_name))));
        }
        let mut clients = Vec::with_capacity(size);

        let endpoint = Endpoint::from_shared(endpoint_str.to_string())
            .map_err(|e| OpsNodeError::Network(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("Invalid endpoint format '{}': {}", endpoint_str, e))))?
            .initial_stream_window_size(Some(DEFAULT_HTTP2_INITIAL_STREAM_WINDOW_SIZE))
            .initial_connection_window_size(Some(DEFAULT_HTTP2_INITIAL_CONNECTION_WINDOW_SIZE))
            .tcp_keepalive(Some(Duration::from_secs(DEFAULT_KEEPALIVE_INTERVAL_SECS)))
            .keep_alive_while_idle(true) // Tonic's default is true
            .http2_keep_alive_interval(Duration::from_secs(DEFAULT_KEEPALIVE_INTERVAL_SECS))
            .keep_alive_timeout(Duration::from_secs(DEFAULT_KEEPALIVE_TIMEOUT_SECS))
            .connect_timeout(Duration::from_millis(connect_timeout_ms))
            .tcp_nodelay(true)
            .user_agent(format!("{}/{}", USER_AGENT_PREFIX, pool_name))
            .map_err(|e| OpsNodeError::Config(config::ConfigError::Message(format!("Failed to set user agent: {}",e))))?;
            // .http2_adaptive_window(true) // This is a client-side setting, typically enabled by default if server supports it.

        // TLS Configuration (assuming default roots)
        let tls_config = ClientTlsConfig::new(); // Add domain name if needed: .domain_name("example.com")

        for i in 0..size {
            tracing::debug!("Connecting client {}/{} for pool '{}' to {}", i + 1, size, pool_name, endpoint_str);
            let channel = endpoint.clone() // Clone endpoint for each connection attempt
                .tls_config(tls_config.clone())
                .map_err(|e| OpsNodeError::Network(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("TLS config error: {}", e))))?
                .connect()
                .await
                .map_err(|e| {
                    tracing::error!("Failed to connect to {} (client {}/{} for pool '{}'): {}", endpoint_str, i+1, size, pool_name, e);
                    OpsNodeError::Network(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, format!("gRPC connect error to {}: {}", endpoint_str, e)))
                })?;

            clients.push(client_factory(channel));
            tracing::info!("Client {}/{} for pool '{}' connected successfully to {}.", i + 1, size, pool_name, endpoint_str);
        }

        Ok(Self {
            clients,
            current_idx: AtomicUsize::new(0),
            _pool_name: pool_name.to_string(),
        })
    }

    // Returns a reference to a client. For tonic clients, they often need to be mutable
    // or cloned to make calls.
    fn get_client(&self) -> &T {
        let idx = self.current_idx.fetch_add(1, Ordering::Relaxed) % self.clients.len();
        &self.clients[idx]
    }
}

// General AuthInterceptor - can be used if a single token is applied to all requests on a channel.
// However, for multiple services (Yellowstone, Jito) with different tokens,
// it's often better to add metadata per-request or use different channels if tokens are static per endpoint.
// The current GrpcManager design adds auth metadata per call.
pub struct AuthInterceptor {
    token: MetadataValue<Ascii>,
}

impl AuthInterceptor {
    pub fn new(token_str: String) -> Result<Self> {
        let token = create_auth_metadata(&token_str)?;
        Ok(Self { token })
    }
}

impl tonic::service::Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> std::result::Result<Request<()>, Status> {
        // Choose appropriate header name, e.g., "authorization" for Bearer tokens,
        // or "x-token" as used in the example.
        request.metadata_mut().insert("x-token", self.token.clone());
        Ok(request)
    }
}
