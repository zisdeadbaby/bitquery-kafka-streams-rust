use ops_node::{
    Config, OpsNodeClient, OpsNodeResult, TradingStrategy, // Assuming OpsNodeResult is the error type from lib.rs
    print_version_info, log_active_features, // Utility functions from lib.rs
};
use ops_node::strategies::sniper::{TokenSnipingStrategy, TokenSniperConfig}; // Specific strategy and its config
use std::sync::Arc;
use tokio::sync::Notify; // For shutdown signaling
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, fmt}; // For logging

#[tokio::main(worker_threads = 4)] // Example: Configure worker threads for Tokio runtime
async fn main() -> anyhow::Result<()> {
    // 1. Initialize Logging (using tracing-subscriber)
    // Reads RUST_LOG environment variable, or defaults.
    // Example: RUST_LOG="ops_node=debug,warp=info,h2=warn"
    let default_log_level = "info,ops_node=debug"; // Default if RUST_LOG is not set
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_log_level));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_thread_ids(true).with_thread_names(true)) // Example: include thread info
        .init();

    // Print application version and active features
    print_version_info();
    log_active_features(); // Log compile-time features

    tracing::info!("OpsNode process starting...");

    // 2. Load Configuration
    let config = match Config::load() {
        Ok(cfg) => {
            tracing::info!("Configuration loaded successfully from '{}' or environment variables.",
                std::env::var("OPS_NODE_CONFIG").unwrap_or_else(|_| "ops-node.toml".to_string()));
            // Optionally log parts of the config (be careful with secrets)
            tracing::debug!("Loaded config (partial): rpc_endpoint={}, metrics_port={}", config.network.rpc_endpoint, config.monitoring.metrics_port);
            cfg
        }
        Err(e) => {
            tracing::error!("Failed to load configuration: {}", e);
            // Depending on severity, might print to stderr as well if tracing isn't fully up.
            eprintln!("Fatal: Failed to load configuration. Error: {}", e);
            return Err(anyhow::anyhow!("Configuration loading failed: {}", e));
        }
    };

    // 3. Create Trading Strategy Instance
    // This is where you would select and configure your strategy.
    // For this example, using TokenSnipingStrategy with a default or derived config.
    // TODO: Make strategy selection and its config more dynamic if needed (e.g., from main config file)
    let sniper_config = TokenSniperConfig {
        min_liquidity_sol_equivalent: config.trading.min_liquidity_sol, // Assuming f64 directly
        max_position_size_sol_equivalent: config.trading.max_position_size_sol, // Assuming f64
        take_profit_bps: 2000, // Example: 20% (override with config if available)
        stop_loss_bps: 1000,   // Example: 10% (override with config if available)
        min_score_threshold: 0.65, // Example
        max_token_age_slots: 7200, // Example: ~1 hour
        max_holding_duration_secs: 600, // Example: 10 minutes
        position_size_risk_factor: 0.7, // Example
    };
    tracing::info!("Using TokenSnipingStrategy with config: {:?}", sniper_config);
    let strategy: Arc<dyn TradingStrategy> = Arc::new(TokenSnipingStrategy::new(sniper_config));

    // 4. Setup Graceful Shutdown Mechanism
    let shutdown_notify = Arc::new(Notify::new());
    let shutdown_clone_for_ctrlc = shutdown_notify.clone();

    if let Err(e) = ctrlc::set_handler(move || {
        tracing::info!("Ctrl+C (SIGINT) received. Initiating graceful shutdown...");
        shutdown_clone_for_ctrlc.notify_waiters(); // Notify all tasks waiting on this signal
    }) {
        tracing::error!("Failed to set Ctrl+C handler: {}. Shutdown might not be graceful.", e);
        return Err(anyhow::anyhow!("Failed to set signal handler: {}", e));
    }
    tracing::info!("Ctrl+C handler set. Press Ctrl+C to initiate graceful shutdown.");

    // 5. Initialize OpsNodeClient
    let mut client = match OpsNodeClient::new(config, strategy, shutdown_notify.clone()).await {
        Ok(c) => {
            tracing::info!("OpsNodeClient initialized successfully.");
            c
        }
        Err(e) => {
            tracing::error!("Failed to initialize OpsNodeClient: {}", e);
            eprintln!("Fatal: OpsNodeClient initialization failed. Error: {}", e);
            return Err(anyhow::anyhow!("OpsNodeClient initialization failed: {}", e));
        }
    };

    // 6. Run the Client
    tracing::info!("Starting OpsNodeClient main event loop...");
    if let Err(e) = client.run().await {
        tracing::error!("OpsNodeClient exited with an error: {}", e);
        eprintln!("Error: OpsNodeClient run failed. Error: {}", e);
        // The client might have already printed latency stats on shutdown.
        // If not, or for a final check:
        // client.latency_monitor.print_all_stats_to_console();
        Err(anyhow::anyhow!("OpsNodeClient run failed: {}", e))
    } else {
        tracing::info!("OpsNodeClient shutdown completed successfully.");
        // client.latency_monitor.print_all_stats_to_console(); // If client.run() doesn't do it on graceful exit
        Ok(())
    }
}
