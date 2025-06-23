# Bitquery Solana Kafka Streaming Service

A high-performance, production-ready Kafka streaming client specifically designed for consuming and processing Solana blockchain data from Bitquery with advanced features and comprehensive monitoring.

## 🚀 Quick Start

### Using Docker (Recommended)

```bash
# Clone and configure
git clone <repository-url>
cd bitquery-solana-kafka

# Set up configuration
cp config/default.env config/local.env
# Edit config/local.env with your Kafka credentials

# Run with Docker Compose
cd docker
docker-compose up -d

# Check health
curl http://localhost:8080/health
```

### Building from Source

```bash
# Install dependencies (Ubuntu/Debian)
sudo apt-get install pkg-config libssl-dev libsasl2-dev

# Build and run
cargo build --release
cp config/default.env .env
# Edit .env with your configuration
./target/release/bitquery-solana-kafka
```

## ✨ Features

### 🏎️ Performance
- **High Throughput**: Process 50,000+ messages/second
- **Low Latency**: Sub-millisecond message processing
- **Batch Processing**: Optimized batch handling for efficiency
- **Memory Efficient**: LZ4 compression and smart memory management

### 🛡️ Reliability
- **Circuit Breaker**: Automatic failure detection and recovery
- **Retry Logic**: Configurable exponential backoff retry policies
- **Health Monitoring**: Built-in health checks and status reporting
- **Graceful Degradation**: Continues operation during partial failures

### 📊 Monitoring
- **Prometheus Metrics**: Comprehensive metrics for monitoring
- **Health Endpoints**: Real-time health and status information
- **Structured Logging**: JSON logging with configurable levels
- **Performance Tracking**: Detailed performance and latency metrics

### 🔧 Production Ready
- **Docker Support**: Production-ready containerization
- **Configuration Management**: Environment-specific configurations
- **SSL/TLS Support**: Secure connections with certificate authentication
- **Scalability**: Horizontal scaling with consumer groups

## 📁 Project Structure

```
bitquery-solana-kafka/
├── src/                    # Main service source code
│   ├── lib.rs             # Library entry point
│   ├── client.rs          # Kafka client implementation
│   ├── consumer.rs        # Message consumer logic
│   ├── processors/        # Message processors (DEX, transactions)
│   ├── filters.rs         # Data filtering logic
│   ├── events.rs          # Event type definitions
│   └── utils/             # Utility functions
├── core/                   # Shared core utilities
│   ├── src/
│   │   ├── utils/         # Core utilities (compression, retry, etc.)
│   │   └── schemas/       # Protobuf schema definitions
│   └── Cargo.toml
├── tools/                  # Testing and debugging tools
├── examples/               # Usage examples
├── tests/                  # Integration tests
├── benches/                # Performance benchmarks
├── config/                 # Configuration files
│   ├── default.env        # Base configuration
│   ├── development.env    # Development settings
│   ├── production.env     # Production optimizations
│   └── docker.env         # Container settings
├── docker/                 # Container configurations
│   ├── Dockerfile         # Production container
│   ├── Dockerfile.dev     # Development container
│   ├── docker-compose.yml # Local development
│   └── docker-compose.prod.yml # Production deployment
└── docs/                   # Documentation
    ├── README.md          # Main documentation
    ├── API.md             # API reference
    ├── DEPLOYMENT.md      # Deployment guide
    └── CONFIGURATION.md   # Configuration reference
```

## 🔧 Configuration

### Quick Configuration

1. **Copy base configuration**:
   ```bash
   cp config/default.env .env
   ```

2. **Set required variables**:
   ```bash
   KAFKA_BROKERS=rpk0.bitquery.io:9092,rpk1.bitquery.io:9092,rpk2.bitquery.io:9092
   KAFKA_USERNAME=your-username
   KAFKA_PASSWORD=your-password
   KAFKA_TOPIC=solana.dextrades.proto
   KAFKA_GROUP_ID=your-group-id
   ```

3. **Choose security protocol**:
   ```bash
   # For development (no encryption)
   KAFKA_SECURITY_PROTOCOL=SASL_PLAINTEXT
   
   # For production (with SSL)
   KAFKA_SECURITY_PROTOCOL=SASL_SSL
   ```

See [CONFIGURATION.md](docs/CONFIGURATION.md) for complete configuration options.

## 🚀 Deployment

### Development
```bash
cd docker
docker-compose up -d
```

### Production
```bash
cd docker
docker-compose -f docker-compose.prod.yml up -d
```

### Kubernetes
```bash
kubectl apply -f k8s/
```

See [DEPLOYMENT.md](docs/DEPLOYMENT.md) for detailed deployment instructions.

## 📊 Monitoring

### Health Check
```bash
curl http://localhost:8080/health
```

### Metrics
```bash
curl http://localhost:9090/metrics
```

### Key Metrics to Monitor
- `kafka_messages_processed_total`: Total messages processed
- `kafka_consumer_lag`: Consumer lag in messages
- `message_processing_duration_seconds`: Processing latency
- `circuit_breaker_state`: Circuit breaker status

## 🎯 Examples

### Basic Consumer
```rust
use bitquery_solana_kafka::KafkaConsumer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let consumer = KafkaConsumer::new("config.env").await?;
    consumer.start().await?;
    Ok(())
}
```

Run examples:
```bash
cargo run --example basic_consumer
cargo run --example dex_monitor
cargo run --example high_volume_processor
```

## 🧪 Testing

### Unit Tests
```bash
cargo test
```

### Integration Tests
```bash
cargo test --test integration_test
```

### Benchmarks
```bash
cargo bench
```

### Load Testing
```bash
cargo run --bin comprehensive_test
```

## 🔍 API Reference

### Health Endpoint
- **GET** `/health` - Service health status
- **Response**: JSON with health metrics and status

### Metrics Endpoint  
- **GET** `/metrics` - Prometheus-compatible metrics
- **Response**: Prometheus format metrics

See [API.md](docs/API.md) for complete API documentation.

## 🛠️ Development

### Prerequisites
- Rust 1.70+
- Docker (for containerized development)
- Access to Bitquery Kafka endpoints

### Build Dependencies
```bash
# Ubuntu/Debian
sudo apt-get install pkg-config libssl-dev libsasl2-dev

# macOS
brew install pkg-config openssl
```

### Development Workflow
```bash
# Build
cargo build

# Run tests
cargo test

# Run with development config
cargo run

# Build release
cargo build --release
```

## 🏗️ Architecture

### Core Components
- **KafkaConsumer**: Main consumer implementation with retry logic
- **EventProcessors**: Pluggable processors for different event types
- **CircuitBreaker**: Failure detection and recovery mechanism
- **MessageFilters**: Configurable filtering for relevant events
- **MetricsCollector**: Performance and health metrics collection

### Data Flow
1. **Consumer**: Consumes messages from Kafka topic
2. **Deserializer**: Parses protobuf messages
3. **Filters**: Applies filtering criteria  
4. **Processors**: Processes events based on type
5. **Metrics**: Collects performance data
6. **Health Check**: Reports service status

## 🔒 Security

### Authentication
- SASL username/password authentication
- SSL/TLS encryption support
- Certificate-based authentication

### Best Practices
- Store credentials securely (environment variables/secrets)
- Use SSL encryption in production
- Implement proper certificate management
- Regular credential rotation

## 🎛️ Performance Tuning

### High Throughput
```bash
KAFKA_CONSUMER_MAX_POLL_RECORDS=2000
MAX_CONCURRENT_PROCESSORS=32
BATCH_SIZE=2000
FEATURE_HIGH_PERFORMANCE=true
```

### Low Latency
```bash
KAFKA_CONSUMER_FETCH_MAX_WAIT_MS=100
BATCH_SIZE=50
FLUSH_INTERVAL_MS=500
```

See [CONFIGURATION.md](docs/CONFIGURATION.md) for detailed tuning guidelines.

## 🆘 Troubleshooting

### Common Issues

1. **Connection Problems**:
   - Verify broker addresses and ports
   - Check credentials and authentication
   - Ensure network connectivity

2. **High Consumer Lag**:
   - Increase consumer instances
   - Optimize batch processing
   - Review filter configurations

3. **Memory Issues**:
   - Adjust batch sizes
   - Enable compression
   - Monitor memory usage

### Debug Mode
```bash
LOG_LEVEL=debug cargo run
```

## 📈 Scaling

### Horizontal Scaling
- Deploy multiple consumer instances
- Use different consumer group IDs
- Scale based on partition count
- Monitor consumer lag

### Vertical Scaling  
- Increase CPU cores for higher throughput
- Add memory for larger batches
- Optimize JVM settings
- Tune consumer configuration

## 🔄 Updates and Maintenance

### Regular Tasks
- Monitor health and metrics
- Update dependencies monthly
- Rotate SSL certificates
- Review performance metrics

### Upgrade Process
1. Test in staging environment
2. Update configuration if needed
3. Deploy with rolling updates
4. Verify functionality
5. Monitor for issues

## 📄 License

This project is licensed under the MIT OR Apache-2.0 License - see the LICENSE files for details.

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📞 Support

- **Documentation**: [docs/](docs/)
- **Examples**: [examples/](examples/)
- **Issues**: GitHub Issues
- **Discussions**: GitHub Discussions

---

**Built with ❤️ for the Solana ecosystem**
