use zola_streams::{
    init_with_config, InitConfig,
    Config,
};
use std::time::Duration;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize SDK with production-level logging and metrics
    let init_config = InitConfig {
        log_filter: "info,bitquery_solana_kafka=info".to_string(),
        enable_metrics: true,
        metrics_port: 9090,
    };
    init_with_config(init_config).await?;

    // Production configuration
    let mut config = Config::default();

    // Kafka optimizations
    config.kafka.session_timeout = Duration::from_secs(45);
    config.kafka.max_poll_records = 5000;
    config.kafka.partition_assignment_strategy = "roundrobin".to_string();

    // Processing optimizations
    config.processing.parallel_workers = num_cpus::get() * 2;
    config.processing.buffer_size = 100_000;
    config.processing.batch_size = 1000;
    config.processing.batch_timeout = Duration::from_millis(100);

    // Resource limits for production
    config.resources.max_memory_bytes = 16 * 1024 * 1024 * 1024; // 16GB
    config.resources.max_messages_in_flight = 100_000;
    config.resources.max_queue_size = 200_000;
    config.resources.backpressure_threshold = 0.85;

    // Retry configuration
    config.retry.max_retries = 10;
    config.retry.initial_delay = Duration::from_millis(100);
    config.retry.max_delay = Duration::from_secs(60);

    // Validate config
    config.validate()?;

    info!("Production config ready: {:#?}", config);
    // You can now use this config to start your client/consumer
    Ok(())
}
