use secrecy::{ExposeSecret, Secret};
use serde::Deserialize;
use std::path::PathBuf;
use anyhow::Context; // Added for better error context in validate

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub network: NetworkConfig,
    pub performance: PerformanceConfig,
    pub trading: TradingConfig,
    pub monitoring: MonitoringConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NetworkConfig {
    pub wss_endpoint: String,
    pub yellowstone_endpoint: String,
    pub jito_endpoint: String,
    pub connection_pool_size: usize,
    pub request_timeout_ms: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PerformanceConfig {
    pub cpu_cores: Vec<usize>,
    pub memory_pool_size_mb: usize,
    pub max_concurrent_operations: usize,
    pub latency_threshold_ns: u64,
    pub use_io_uring: bool,
    pub enable_huge_pages: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TradingConfig {
    pub max_position_size_sol: f64,
    pub min_liquidity_sol: f64,
    pub max_slippage_bps: u16,
    pub priority_fee_cap: u64,
    pub enable_jito_bundles: bool,
    pub tip_amount_lamports: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MonitoringConfig {
    pub metrics_port: u16,
    pub alert_webhook: Option<String>,
    pub log_level: String,
    pub enable_tracing: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    #[serde(rename = "api_key")]
    pub yellowstone_token: Secret<String>, // Renamed to yellowstone_token for clarity
    pub jito_auth_token: Option<Secret<String>>,
    pub private_key_path: PathBuf,
}

impl Config {
    pub fn load() -> Result<Self, config::ConfigError> {
        let config_path = std::env::var("OPS_NODE_CONFIG")
            .unwrap_or_else(|_| "ops-node.toml".to_string()); // Default config file name

        tracing::info!("Loading configuration from: {}", config_path);

        let builder = config::Config::builder()
            .add_source(config::File::with_name(&config_path).required(false)) // Made optional to allow env-only config
            .add_source(config::Environment::with_prefix("OPS_NODE").separator("__").try_parsing(true));

        builder.build()?.try_deserialize()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.performance.cpu_cores.is_empty() {
            // Allow empty cpu_cores to not pin threads if not specified
            // anyhow::bail!("At least one CPU core must be specified in performance.cpu_cores");
        }

        if self.network.connection_pool_size == 0 {
            anyhow::bail!("Network connection_pool_size must be greater than 0");
        }

        if self.trading.max_slippage_bps > 10000 { // Max 100%
            anyhow::bail!("Trading max_slippage_bps cannot exceed 10000 (100%)");
        }

        if self.auth.yellowstone_token.expose_secret().is_empty() {
            anyhow::bail!("Auth yellowstone_token (api_key) cannot be empty");
        }

        if !self.auth.private_key_path.exists() {
            anyhow::bail!("Auth private_key_path does not exist: {:?}", self.auth.private_key_path);
        }
        if !self.auth.private_key_path.is_file() {
            anyhow::bail!("Auth private_key_path is not a file: {:?}", self.auth.private_key_path);
        }

        // Validate WSS and Yellowstone endpoints are valid URLs (basic check)
        url::Url::parse(&self.network.wss_endpoint)
            .with_context(|| format!("Invalid network.wss_endpoint URL: {}", self.network.wss_endpoint))?;
        url::Url::parse(&self.network.yellowstone_endpoint)
            .with_context(|| format!("Invalid network.yellowstone_endpoint URL: {}", self.network.yellowstone_endpoint))?;
        url::Url::parse(&self.network.jito_endpoint)
            .with_context(|| format!("Invalid network.jito_endpoint URL: {}", self.network.jito_endpoint))?;


        tracing::info!("Configuration validated successfully.");
        Ok(())
    }
}
