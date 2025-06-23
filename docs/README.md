# Bitquery Solana Kafka Streaming Service

A high-performance, specialized Kafka streaming client for Bitquery Solana data with enhanced features for real-time blockchain data processing.

## Overview

This service provides a production-ready Kafka streaming solution specifically designed for consuming and processing Solana blockchain data from Bitquery. It includes advanced features like circuit breakers, retry logic, compression, deduplication, and comprehensive monitoring.

## Features

### Core Capabilities
- **High-Performance Streaming**: Optimized Kafka consumer for high-throughput blockchain data
- **Advanced Filtering**: DEX trade filtering with customizable criteria
- **Circuit Breaker**: Automatic failure detection and recovery
- **Retry Logic**: Configurable retry policies with exponential backoff
- **Compression**: LZ4 compression for efficient data transfer
- **Deduplication**: Message deduplication to prevent duplicate processing
- **Monitoring**: Comprehensive metrics and health checking

### Data Processing
- **DEX Trade Processing**: Specialized processors for decentralized exchange data
- **Transaction Processing**: General transaction data processing
- **Real-time Filtering**: Filter by program ID, minimum amounts, event types
- **Batch Processing**: Efficient batch processing capabilities

### Production Features
- **Health Checks**: Built-in health monitoring endpoints
- **Metrics**: Prometheus-compatible metrics
- **Logging**: Structured logging with configurable levels
- **Docker Support**: Production-ready containerization
- **Environment Configuration**: Multi-environment configuration management

## Quick Start

### Prerequisites
- Rust 1.70+ (for building from source)
- Docker (for containerized deployment)
- Access to Bitquery Kafka endpoints

### Using Docker (Recommended)

1. **Basic Setup**:
   ```bash
   # Clone the repository
   git clone <repository-url>
   cd zola-streams
   
   # Configure environment
   cp config/default.env config/local.env
   # Edit config/local.env with your Kafka credentials
   
   # Run with Docker Compose
   cd docker
   docker-compose up -d
   ```

2. **Production Deployment**:
   ```bash
   # Use production configuration
   docker-compose -f docker-compose.prod.yml up -d
   ```

### Building from Source

1. **Install Dependencies**:
   ```bash
   # Ubuntu/Debian
   sudo apt-get install pkg-config libssl-dev libsasl2-dev
   
   # macOS
   brew install pkg-config openssl
   ```

2. **Build and Run**:
   ```bash
   # Build the project
   cargo build --release
   
   # Set up configuration
   cp config/default.env .env
   # Edit .env with your configuration
   
   # Run the service
   ./target/release/zola-streams
   ```

## Configuration

### Environment Variables

The service uses environment variables for configuration. See [CONFIGURATION.md](CONFIGURATION.md) for detailed documentation.

Key variables:
- `KAFKA_BROKERS`: Kafka broker endpoints
- `KAFKA_USERNAME`: Authentication username
- `KAFKA_PASSWORD`: Authentication password
- `KAFKA_TOPIC`: Topic to consume from
- `KAFKA_GROUP_ID`: Consumer group ID

### Configuration Files

- `config/default.env`: Base configuration
- `config/development.env`: Development overrides
- `config/production.env`: Production optimizations
- `config/docker.env`: Container-specific settings

## API Reference

### Health Check Endpoint
```
GET /health
```
Returns service health status and metrics.

### Metrics Endpoint
```
GET /metrics
```
Returns Prometheus-compatible metrics.

See [API.md](API.md) for complete API documentation.

## Examples

The `examples/` directory contains various usage examples:

- `basic_consumer.rs`: Simple consumer setup
- `dex_monitor.rs`: DEX trade monitoring
- `high_volume_processor.rs`: High-throughput processing
- `connection_recovery.rs`: Error handling and recovery
- `health_check.rs`: Health monitoring setup

Run examples with:
```bash
cargo run --example basic_consumer
```

## Development

### Project Structure
```
zola-streams/
├── src/                    # Main service source
├── core/                   # Shared core utilities
├── tools/                  # Testing and debugging tools
├── examples/               # Usage examples
├── tests/                  # Integration tests
├── benches/                # Performance benchmarks
├── config/                 # Configuration files
├── docker/                 # Container configurations
└── docs/                   # Documentation
```

### Running Tests
```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration_test

# All tests
cargo test --all
```

### Benchmarks
```bash
# Run performance benchmarks
cargo bench
```

## Deployment

See [DEPLOYMENT.md](DEPLOYMENT.md) for detailed deployment instructions including:
- Docker deployment
- Kubernetes deployment
- Production configuration
- Monitoring setup
- Scaling guidelines

## Monitoring

The service provides comprehensive monitoring through:
- **Health checks**: `/health` endpoint
- **Metrics**: Prometheus-compatible `/metrics` endpoint
- **Logging**: Structured JSON logging
- **Circuit breaker status**: Connection health monitoring

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

This project is licensed under the MIT OR Apache-2.0 License.

## Support

For issues and questions:
- Check the [documentation](docs/)
- Review [examples](examples/)
- Open an issue on the repository
