# Configuration Reference

This document provides comprehensive information about configuring the Bitquery Solana Kafka Streaming Service.

## Configuration Overview

The service uses environment variables for configuration, organized into several categories:
- **Kafka Connection**: Core Kafka broker settings
- **Authentication**: SASL and SSL authentication
- **Performance**: Throughput and resource optimization  
- **Monitoring**: Health checks and metrics
- **Processing**: Message processing behavior
- **Logging**: Log levels and formats

## Configuration Files

### File Locations
- `config/default.env`: Base configuration with sensible defaults
- `config/development.env`: Development environment overrides
- `config/production.env`: Production environment optimizations
- `config/docker.env`: Container-specific settings

### Loading Priority
1. Environment variables (highest priority)
2. `.env` file in current directory
3. `config/docker.env` (in containers)
4. `config/production.env` (production mode)
5. `config/development.env` (development mode)
6. `config/default.env` (fallback)

## Kafka Configuration

### Connection Settings

#### `KAFKA_BROKERS` (Required)
**Type**: String (comma-separated)
**Default**: None
**Example**: `rpk0.bitquery.io:9092,rpk1.bitquery.io:9092,rpk2.bitquery.io:9092`

Comma-separated list of Kafka broker addresses with ports.

#### `KAFKA_TOPIC` (Required)
**Type**: String
**Default**: None
**Example**: `solana.dextrades.proto`

The Kafka topic to consume messages from.

#### `KAFKA_GROUP_ID` (Required)
**Type**: String
**Default**: None
**Example**: `solana_113-dex-monitor-prod-01`

Consumer group identifier. Multiple consumers with the same group ID will share message consumption.

### Authentication

#### `KAFKA_USERNAME` (Required)
**Type**: String
**Default**: None
**Example**: `solana_113`

SASL authentication username provided by Bitquery.

#### `KAFKA_PASSWORD` (Required)
**Type**: String
**Default**: None
**Example**: `cuDDLAUguo75blkNdbCBSHvNCK1udw`

SASL authentication password provided by Bitquery.

**Security Note**: Store passwords securely using secrets management.

#### `KAFKA_SECURITY_PROTOCOL`
**Type**: String
**Default**: `SASL_PLAINTEXT`
**Options**: `SASL_PLAINTEXT`, `SASL_SSL`
**Example**: `SASL_SSL`

Security protocol for Kafka connections:
- `SASL_PLAINTEXT`: SASL authentication without encryption (development)
- `SASL_SSL`: SASL authentication with SSL/TLS encryption (production)

### SSL Configuration

Required when `KAFKA_SECURITY_PROTOCOL=SASL_SSL`:

#### `KAFKA_CA_CERT_PATH`
**Type**: File Path
**Default**: `./certs/kafka-ca.pem`
**Example**: `/app/certs/kafka-ca.pem`

Path to the Certificate Authority (CA) certificate file.

#### `KAFKA_CLIENT_CERT_PATH`
**Type**: File Path
**Default**: `./certs/kafka-cert.pem`
**Example**: `/app/certs/kafka-cert.pem`

Path to the client certificate file for mutual TLS authentication.

#### `KAFKA_CLIENT_KEY_PATH`
**Type**: File Path
**Default**: `./certs/kafka-key.pem`
**Example**: `/app/certs/kafka-key.pem`

Path to the client private key file.

**Security Note**: Ensure certificate files have proper permissions (600).

## Consumer Configuration

### Session Management

#### `KAFKA_CONSUMER_SESSION_TIMEOUT_MS`
**Type**: Integer
**Default**: `30000` (30 seconds)
**Range**: `6000` - `300000`
**Example**: `45000`

Maximum time the consumer may be out of contact with brokers before being considered dead.

**Tuning Guidelines**:
- Lower values: Faster failure detection, more sensitive to network issues
- Higher values: More tolerant of network issues, slower failure detection

#### `KAFKA_CONSUMER_HEARTBEAT_INTERVAL_MS`
**Type**: Integer
**Default**: `3000` (3 seconds)
**Range**: `1000` - `session_timeout/3`
**Example**: `3000`

How often to send heartbeat signals to the Kafka coordinator.

**Rule**: Must be less than 1/3 of session timeout.

### Performance Tuning

#### `KAFKA_CONSUMER_MAX_POLL_RECORDS`
**Type**: Integer
**Default**: `500`
**Range**: `1` - `10000`
**Example**: `1000`

Maximum number of records returned in a single poll() call.

**Tuning Guidelines**:
- Higher values: Better throughput, higher memory usage
- Lower values: Lower latency, more CPU overhead

#### `KAFKA_CONSUMER_FETCH_MIN_BYTES`
**Type**: Integer
**Default**: `1024` (1 KB)
**Range**: `1` - `52428800` (50 MB)
**Example**: `4096`

Minimum amount of data the server should return for a fetch request.

#### `KAFKA_CONSUMER_FETCH_MAX_WAIT_MS`
**Type**: Integer
**Default**: `500`
**Range**: `0` - `30000`
**Example**: `1000`

Maximum time the server will block before answering a fetch request.

#### `KAFKA_CONSUMER_AUTO_OFFSET_RESET`
**Type**: String
**Default**: `latest`
**Options**: `earliest`, `latest`, `none`
**Example**: `earliest`

What to do when there is no initial offset in Kafka:
- `earliest`: Start from the beginning of the partition
- `latest`: Start from the end of the partition (skip existing messages)
- `none`: Throw an exception

## Service Configuration

### Network Ports

#### `HEALTH_CHECK_PORT`
**Type**: Integer
**Default**: `8080`
**Range**: `1024` - `65535`
**Example**: `8080`

Port for the health check HTTP endpoint (`/health`).

#### `PROMETHEUS_PORT`
**Type**: Integer
**Default**: `9090`
**Range**: `1024` - `65535`
**Example**: `9090`

Port for the Prometheus metrics HTTP endpoint (`/metrics`).

### Logging

#### `LOG_LEVEL`
**Type**: String
**Default**: `info`
**Options**: `trace`, `debug`, `info`, `warn`, `error`
**Example**: `warn`

Logging verbosity level:
- `trace`: Very detailed debugging information
- `debug`: Detailed debugging information
- `info`: General information messages
- `warn`: Warning messages only
- `error`: Error messages only

#### `RUST_LOG`
**Type**: String
**Default**: Uses `LOG_LEVEL`
**Example**: `bitquery_solana_kafka=debug,rdkafka=warn`

Fine-grained logging control using Rust's env_logger format.

#### `RUST_BACKTRACE`
**Type**: String
**Default**: `0`
**Options**: `0`, `1`, `full`
**Example**: `1`

Controls backtrace printing on panics:
- `0`: No backtraces
- `1`: Short backtraces
- `full`: Full backtraces with line numbers

## Processing Configuration

### Concurrency

#### `MAX_CONCURRENT_PROCESSORS`
**Type**: Integer
**Default**: `16`
**Range**: `1` - `1000`
**Example**: `32`

Maximum number of concurrent message processors.

**Tuning Guidelines**:
- More processors: Higher throughput, more resource usage
- Fewer processors: Lower resource usage, potential bottleneck

#### `BATCH_SIZE`
**Type**: Integer
**Default**: `1000`
**Range**: `1` - `10000`
**Example**: `500`

Number of messages to process in a single batch.

#### `FLUSH_INTERVAL_MS`
**Type**: Integer
**Default**: `1000`
**Range**: `100` - `60000`
**Example**: `2000`

Maximum time to wait before flushing a partial batch.

### Circuit Breaker

#### `CIRCUIT_BREAKER_FAILURE_THRESHOLD`
**Type**: Integer
**Default**: `5`
**Range**: `1` - `100`
**Example**: `10`

Number of consecutive failures before opening the circuit breaker.

#### `CIRCUIT_BREAKER_TIMEOUT_DURATION_MS`
**Type**: Integer
**Default**: `60000` (60 seconds)
**Range**: `1000` - `600000`
**Example**: `30000`

How long to keep the circuit breaker open before testing recovery.

#### `CIRCUIT_BREAKER_SUCCESS_THRESHOLD`
**Type**: Integer
**Default**: `3`
**Range**: `1` - `10`
**Example**: `5`

Number of consecutive successes needed to close the circuit breaker.

### Retry Configuration

#### `RETRY_MAX_ATTEMPTS`
**Type**: Integer
**Default**: `3`
**Range**: `1` - `10`
**Example**: `5`

Maximum number of retry attempts for failed operations.

#### `RETRY_INITIAL_DELAY_MS`
**Type**: Integer
**Default**: `1000`
**Range**: `100` - `10000`
**Example**: `500`

Initial delay before the first retry attempt.

#### `RETRY_MAX_DELAY_MS`
**Type**: Integer
**Default**: `30000`
**Range**: `1000` - `300000`
**Example**: `60000`

Maximum delay between retry attempts.

#### `RETRY_BACKOFF_MULTIPLIER`
**Type**: Float
**Default**: `2.0`
**Range**: `1.0` - `10.0`
**Example**: `1.5`

Multiplier for exponential backoff between retries.

## Filter Configuration

### DEX Trade Filtering

#### `DEX_FILTER_TARGET_PROGRAMS`
**Type**: String (comma-separated)
**Default**: None (process all)
**Example**: `675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8,EhpK2uyoWaAg1vNfTVMcgbFwP7ZnBRAq2QNmtpZDZYJJ`

Comma-separated list of Solana program IDs to filter for. Only trades from these programs will be processed.

#### `DEX_FILTER_MIN_AMOUNT_USD`
**Type**: Float
**Default**: None (no minimum)
**Example**: `100.0`

Minimum trade amount in USD to process. Trades below this amount will be filtered out.

#### `DEX_FILTER_MARKETS`
**Type**: String (comma-separated)
**Default**: None (all markets)
**Example**: `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v,So11111111111111111111111111111111111111112`

Comma-separated list of market addresses to process.

#### `DEX_FILTER_EVENT_TYPES`
**Type**: String (comma-separated)
**Default**: None (all types)
**Example**: `swap,trade`
**Options**: `swap`, `trade`, `add_liquidity`, `remove_liquidity`

Types of DEX events to process.

## Compression Configuration

#### `COMPRESSION_ENABLED`
**Type**: Boolean
**Default**: `true`
**Example**: `false`

Enable/disable LZ4 compression for message processing.

#### `COMPRESSION_LEVEL`
**Type**: Integer
**Default**: `1`
**Range**: `1` - `12`
**Example**: `6`

LZ4 compression level:
- Lower values: Faster compression, larger size
- Higher values: Slower compression, smaller size

## Metrics Configuration

#### `METRICS_ENABLED`
**Type**: Boolean
**Default**: `true`
**Example**: `false`

Enable/disable Prometheus metrics collection.

#### `METRICS_COLLECTION_INTERVAL_MS`
**Type**: Integer
**Default**: `10000` (10 seconds)
**Range**: `1000` - `300000`
**Example**: `5000`

How often to collect and update metrics.

## Memory Management

#### `MEMORY_LIMIT_MB`
**Type**: Integer
**Default**: None (system default)
**Range**: `256` - `16384`
**Example**: `2048`

Maximum memory usage in megabytes before triggering garbage collection.

#### `GC_THRESHOLD_MB`
**Type**: Integer
**Default**: `512`
**Range**: `64` - `4096`
**Example**: `1024`

Memory threshold for triggering garbage collection.

## Feature Flags

#### `FEATURE_HIGH_PERFORMANCE`
**Type**: Boolean
**Default**: `false`
**Example**: `true`

Enable high-performance mode with jemalloc allocator and optimizations.

#### `FEATURE_METRICS`
**Type**: Boolean
**Default**: `true`
**Example**: `false`

Enable metrics collection and reporting.

#### `FEATURE_HEALTH_CHECKS`
**Type**: Boolean
**Default**: `true`
**Example**: `false`

Enable health check endpoint.

## Environment-Specific Examples

### Development Configuration
```bash
# config/development.env
KAFKA_BROKERS=localhost:9092
KAFKA_SECURITY_PROTOCOL=SASL_PLAINTEXT
LOG_LEVEL=debug
MAX_CONCURRENT_PROCESSORS=4
BATCH_SIZE=100
METRICS_COLLECTION_INTERVAL_MS=5000
```

### Staging Configuration
```bash
# config/staging.env
KAFKA_BROKERS=staging-kafka-1:9092,staging-kafka-2:9092
KAFKA_SECURITY_PROTOCOL=SASL_SSL
LOG_LEVEL=info
MAX_CONCURRENT_PROCESSORS=8
BATCH_SIZE=500
CIRCUIT_BREAKER_FAILURE_THRESHOLD=3
```

### Production Configuration
```bash
# config/production.env
KAFKA_BROKERS=rpk0.bitquery.io:9092,rpk1.bitquery.io:9092,rpk2.bitquery.io:9092
KAFKA_SECURITY_PROTOCOL=SASL_SSL
LOG_LEVEL=warn
MAX_CONCURRENT_PROCESSORS=16
BATCH_SIZE=1000
FEATURE_HIGH_PERFORMANCE=true
MEMORY_LIMIT_MB=2048
```

## Configuration Validation

### Startup Validation
The service validates configuration at startup and will exit with errors if:
- Required parameters are missing
- Values are outside valid ranges
- SSL certificate files don't exist
- Network ports are already in use

### Runtime Validation
Some settings are validated during runtime:
- Consumer configuration compatibility
- SSL certificate expiration
- Memory limits and usage

### Configuration Testing
```bash
# Test configuration without starting the service
./bitquery-solana-kafka --config-check

# Validate specific environment file
./bitquery-solana-kafka --config-file config/production.env --validate

# Dry run with full validation
./bitquery-solana-kafka --dry-run
```

## Security Best Practices

### Credential Management
- Store passwords in environment variables or secrets management
- Rotate credentials regularly
- Use principle of least privilege
- Audit credential usage

### SSL Configuration
- Use strong cipher suites
- Keep certificates updated
- Secure certificate storage
- Validate certificate chains

### Network Security
- Restrict port access with firewalls
- Use TLS for all external connections
- Implement network segmentation
- Monitor network traffic

## Performance Tuning Guidelines

### High Throughput Scenarios
```bash
KAFKA_CONSUMER_MAX_POLL_RECORDS=2000
KAFKA_CONSUMER_FETCH_MIN_BYTES=8192
MAX_CONCURRENT_PROCESSORS=32
BATCH_SIZE=2000
FEATURE_HIGH_PERFORMANCE=true
```

### Low Latency Scenarios
```bash
KAFKA_CONSUMER_MAX_POLL_RECORDS=100
KAFKA_CONSUMER_FETCH_MAX_WAIT_MS=100
MAX_CONCURRENT_PROCESSORS=8
BATCH_SIZE=50
FLUSH_INTERVAL_MS=500
```

### Resource-Constrained Environments
```bash
KAFKA_CONSUMER_MAX_POLL_RECORDS=200
MAX_CONCURRENT_PROCESSORS=4
BATCH_SIZE=200
MEMORY_LIMIT_MB=512
COMPRESSION_ENABLED=true
```

## Troubleshooting Configuration Issues

### Common Problems

1. **Authentication Failures**:
   - Verify `KAFKA_USERNAME` and `KAFKA_PASSWORD`
   - Check credential format and special characters
   - Ensure credentials haven't expired

2. **SSL Connection Issues**:
   - Verify certificate paths and permissions
   - Check certificate validity and expiration
   - Ensure CA certificate matches broker

3. **Performance Issues**:
   - Review consumer configuration settings
   - Check for resource bottlenecks
   - Monitor consumer lag and processing times

4. **Memory Issues**:
   - Adjust `MEMORY_LIMIT_MB` setting
   - Review batch sizes and concurrent processors
   - Enable compression to reduce memory usage

### Debug Configuration
```bash
# Enable verbose logging
RUST_LOG=debug
LOG_LEVEL=debug

# Enable performance metrics
METRICS_COLLECTION_INTERVAL_MS=1000

# Reduce batch sizes for debugging
BATCH_SIZE=10
MAX_CONCURRENT_PROCESSORS=1
```
