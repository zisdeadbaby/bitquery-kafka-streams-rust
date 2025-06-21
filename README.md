# Zolca Operations

High-performance Rust infrastructure for Solana blockchain operations, including real-time data streaming, high-frequency trading, and comprehensive monitoring.

## Architecture Overview

```
zolca-ops/
├── shared/
│   └── bitquery-solana-core/          # 🔧 Shared core utilities
├── bitquery-solana-sdk/               # 📡 Main Solana SDK  
├── streams/kafka/bitquery-solana-kafka/  # 🌊 Specialized Kafka streaming
├── trading/hft/ops-node/              # ⚡ High-frequency trading node
└── deployment/                        # 🚀 Unified deployment system
```

## Components

### 🔧 Core Library (`bitquery-solana-core`)
Shared utilities and core functionality:
- **Data Compression**: LZ4 compression for Kafka messages
- **Base58 Caching**: LRU-cached Base58 encoding/decoding
- **Message Deduplication**: Time-window based duplicate filtering  
- **Circuit Breaker**: Fault tolerance patterns
- **Retry Logic**: Configurable retry strategies with backoff
- **Protobuf Schemas**: Unified Solana data structures

### 📡 Main SDK (`bitquery-solana-sdk`)
Production-ready SDK for consuming Bitquery Solana data:
- **High-throughput**: Batch processing with configurable workers
- **Resource Management**: Memory and CPU usage controls
- **Event Filtering**: Flexible filtering for transactions/DEX trades
- **Metrics**: Built-in Prometheus metrics
- **Backpressure**: Automatic rate limiting

### 🌊 Kafka Streaming (`bitquery-solana-kafka`)  
Specialized SDK for high-volume Kafka streaming:
- **Enhanced Features**: Additional utilities for stream processing
- **Deduplication**: Built-in message deduplication
- **Circuit Breaker**: Automatic failure recovery
- **Advanced Caching**: Optimized for high-frequency access patterns

### ⚡ HFT Trading Node (`ops-node`)
Ultra-low latency trading operations:
- **Sub-millisecond**: Target <0.2ms latency to RPC providers
- **Network Optimization**: Custom network stack with io_uring
- **Memory Management**: NUMA-aware allocation strategies
- **Frankfurt VPS**: Optimized for European trading

### 🚀 Deployment System
Unified deployment and configuration management:
- **Multi-Environment**: Development, staging, production configs
- **System Optimization**: Automated OS tuning for performance
- **Template-based**: Environment variable substitution
- **Dry-run**: Safe deployment testing

## Quick Start

### Prerequisites
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install system dependencies (Ubuntu/Debian)
sudo apt update && sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libsasl2-dev \
    cmake
```

### Build All Components
```bash
git clone https://github.com/yourusername/zolca-ops.git
cd zolca-ops
cargo build --workspace --release
```

### Run Tests
```bash
cargo test --workspace
```

## Usage Examples

### Main SDK Usage
```rust
use bitquery_solana_sdk::{BitqueryClient, Config, FilterBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure client
    let config = Config::builder()
        .kafka_brokers(vec!["your-kafka-broker:9092".to_string()])
        .kafka_topics(vec!["solana.transactions".to_string()])
        .build();
    
    // Create event filter for DEX trades > $10k
    let filter = FilterBuilder::new()
        .event_types(vec!["DexTrade"])
        .min_amount(10000.0)
        .build();
    
    // Create and start client
    let mut client = BitqueryClient::new(config).await?;
    client.set_filter(filter).await?;
    client.start().await?;
    
    // Process events
    client.process_events(|event| {
        println!("Received trade: {} SOL", event.amount());
        Ok(())
    }).await?;
    
    Ok(())
}
```

### Deployment
```bash
# Deploy to production Frankfurt VPS
cd deployment
./scripts/deploy.sh ops-node production --config frankfurt-vps.conf

# Deploy SDK for development
./scripts/deploy.sh sdk development --dry-run
```

## Performance

### Benchmarks
- **Main SDK**: 50,000+ events/second per worker
- **Kafka SDK**: 100,000+ messages/second with deduplication
- **ops-node**: <0.2ms latency to Frankfurt RPC providers
- **Memory**: <100MB baseline, configurable limits

### Optimization Features
- **Jemalloc**: Optional high-performance allocator
- **Batch Processing**: Configurable batch sizes and timeouts  
- **Connection Pooling**: Reusable Kafka connections
- **CPU Affinity**: NUMA-aware thread placement
- **Network Tuning**: BBR congestion control, optimized buffers

## Configuration

### Environment Variables
```bash
# Core settings
export RUST_LOG="info,bitquery_solana=debug"
export KAFKA_BROKERS="localhost:9092"

# Performance tuning
export BATCH_SIZE="1000"
export WORKER_THREADS="8"
export MEMORY_LIMIT_MB="512"

# Trading node
export YELLOWSTONE_API_KEY="your-api-key"
export JITO_AUTH_TOKEN="your-token"
export TARGET_LATENCY_MS="0.2"
```

### Configuration Files
```toml
# ops-node.toml
[server]
host = "0.0.0.0"
port = 8080

[rpc]
endpoint = "https://rpc-frankfurt.rpcfast.com"
timeout_ms = 1000

[performance]
target_latency_ms = 0.2
max_connections = 1000
```

## Monitoring

### Metrics
The system exposes Prometheus metrics:
- `bitquery_events_processed_total` - Total events processed
- `bitquery_batch_processing_duration_ms` - Batch processing latency
- `bitquery_memory_usage_bytes` - Memory consumption
- `ops_node_latency_ms` - RPC latency measurements

### Health Checks
```bash
# Check SDK health
curl http://localhost:8080/health

# Check metrics
curl http://localhost:8080/metrics
```

## Development

### Project Structure
```
├── shared/bitquery-solana-core/    # Shared utilities
│   ├── src/utils/                  # Compression, caching, etc.
│   └── src/schemas/                # Protobuf definitions
├── bitquery-solana-sdk/            # Main SDK
│   ├── src/client.rs               # Main client logic
│   ├── src/consumer.rs             # Kafka consumer
│   └── examples/                   # Usage examples
└── trading/hft/ops-node/           # Trading node
    ├── src/strategies/             # Trading strategies
    └── src/network.rs              # Network optimization
```

### Contributing
1. Fork the repository
2. Create feature branch: `git checkout -b feature/amazing-feature`
3. Run tests: `cargo test --workspace`
4. Commit changes: `git commit -m 'Add amazing feature'`
5. Push branch: `git push origin feature/amazing-feature`
6. Open Pull Request

### Code Standards
- **Rust 2021 Edition**: Modern Rust practices
- **Documentation**: All public APIs must be documented
- **Testing**: Minimum 80% test coverage
- **Performance**: Benchmark regressions not allowed
- **Safety**: No `unsafe` code without justification

## License

This project is licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Changelog

### Version 2.0.0 (Current)
- ✅ **Major Refactor**: Eliminated ~60% code duplication
- ✅ **Shared Core**: Created unified utility library
- ✅ **Workspace**: Converted to Cargo workspace
- ✅ **Deployment**: Unified deployment system
- ✅ **Performance**: Enhanced caching and deduplication
- ✅ **Documentation**: Comprehensive migration guide

### Version 1.x (Legacy)
- Basic SDK functionality
- Separate ops-node implementation
- Individual deployment scripts

## Support

- **Documentation**: See [MIGRATION_GUIDE.md](MIGRATION_GUIDE.md)
- **Issues**: GitHub Issues for bug reports
- **Discussions**: GitHub Discussions for questions
- **Security**: Send security issues to security@zolca.com