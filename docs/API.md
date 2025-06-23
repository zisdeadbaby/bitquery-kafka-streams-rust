# API Reference

This document provides detailed information about the Bitquery Solana Kafka Streaming Service API endpoints and interfaces.

## HTTP Endpoints

### Health Check

**Endpoint**: `GET /health`
**Port**: 8080 (configurable via `HEALTH_CHECK_PORT`)

Returns the current health status of the service.

#### Response Format
```json
{
  "status": "healthy" | "unhealthy",
  "timestamp": "2025-06-23T12:00:00Z",
  "version": "2.0.0",
  "uptime_seconds": 3600,
  "kafka": {
    "connected": true,
    "last_message_timestamp": "2025-06-23T11:59:30Z",
    "consumer_lag": 125,
    "errors_last_hour": 0
  },
  "circuit_breaker": {
    "state": "closed" | "open" | "half_open",
    "failure_count": 0,
    "last_failure": null
  },
  "memory": {
    "used_mb": 245,
    "peak_mb": 300
  }
}
```

#### Status Codes
- `200 OK`: Service is healthy
- `503 Service Unavailable`: Service is unhealthy

### Metrics

**Endpoint**: `GET /metrics`
**Port**: 9090 (configurable via `PROMETHEUS_PORT`)

Returns Prometheus-compatible metrics for monitoring and alerting.

#### Available Metrics

##### Counter Metrics
- `kafka_messages_processed_total`: Total number of messages processed
- `kafka_messages_failed_total`: Total number of failed message processing attempts
- `kafka_connection_errors_total`: Total number of Kafka connection errors
- `dex_trades_processed_total`: Total number of DEX trades processed
- `events_processed_total{event_type="..."}`: Total events by type

##### Gauge Metrics
- `kafka_consumer_lag`: Current consumer lag in messages
- `active_connections`: Number of active Kafka connections
- `circuit_breaker_state`: Circuit breaker state (0=closed, 1=open, 2=half_open)
- `memory_usage_bytes`: Current memory usage in bytes
- `processing_queue_size`: Number of messages in processing queue

##### Histogram Metrics
- `message_processing_duration_seconds`: Message processing time distribution
- `kafka_fetch_duration_seconds`: Kafka fetch operation duration
- `batch_processing_duration_seconds`: Batch processing time distribution

#### Example Metrics Output
```
# HELP kafka_messages_processed_total Total number of messages processed
# TYPE kafka_messages_processed_total counter
kafka_messages_processed_total 15423

# HELP kafka_consumer_lag Current consumer lag in messages  
# TYPE kafka_consumer_lag gauge
kafka_consumer_lag 125

# HELP message_processing_duration_seconds Message processing time
# TYPE message_processing_duration_seconds histogram
message_processing_duration_seconds_bucket{le="0.001"} 1250
message_processing_duration_seconds_bucket{le="0.005"} 8923
message_processing_duration_seconds_bucket{le="0.01"} 12456
message_processing_duration_seconds_bucket{le="+Inf"} 15423
message_processing_duration_seconds_sum 45.67
message_processing_duration_seconds_count 15423
```

## Configuration API

### Environment Variables

The service is configured through environment variables. All configuration is read at startup.

#### Kafka Configuration
- `KAFKA_BROKERS`: Comma-separated list of Kafka brokers
- `KAFKA_USERNAME`: SASL authentication username
- `KAFKA_PASSWORD`: SASL authentication password
- `KAFKA_TOPIC`: Topic to consume from
- `KAFKA_GROUP_ID`: Consumer group identifier
- `KAFKA_SECURITY_PROTOCOL`: Security protocol (`SASL_PLAINTEXT` or `SASL_SSL`)

#### SSL Configuration (when using SASL_SSL)
- `KAFKA_CA_CERT_PATH`: Path to CA certificate file
- `KAFKA_CLIENT_CERT_PATH`: Path to client certificate file
- `KAFKA_CLIENT_KEY_PATH`: Path to client private key file

#### Service Configuration
- `HEALTH_CHECK_PORT`: Port for health check endpoint (default: 8080)
- `PROMETHEUS_PORT`: Port for metrics endpoint (default: 9090)
- `LOG_LEVEL`: Logging level (`trace`, `debug`, `info`, `warn`, `error`)

#### Performance Tuning
- `KAFKA_CONSUMER_SESSION_TIMEOUT_MS`: Consumer session timeout (default: 30000)
- `KAFKA_CONSUMER_HEARTBEAT_INTERVAL_MS`: Heartbeat interval (default: 3000)
- `KAFKA_CONSUMER_MAX_POLL_RECORDS`: Max records per poll (default: 500)
- `MAX_CONCURRENT_PROCESSORS`: Max concurrent message processors (default: 16)
- `BATCH_SIZE`: Batch processing size (default: 1000)
- `FLUSH_INTERVAL_MS`: Batch flush interval (default: 1000)

### Configuration Validation

The service validates configuration at startup and will exit with an error if:
- Required Kafka connection parameters are missing
- SSL certificates are specified but files don't exist
- Port numbers are invalid or already in use
- Numeric values are out of valid ranges

## Data Structures

### Event Types

The service processes several types of blockchain events:

#### DexTradeEvent
```rust
pub struct DexTradeEvent {
    pub signature: String,
    pub slot: u64,
    pub timestamp: i64,
    pub program_id: String,
    pub market: String,
    pub maker: String,
    pub taker: String,
    pub amount_usd: f64,
    pub base_token: String,
    pub quote_token: String,
    pub side: TradeSide,
}
```

#### TransactionEvent
```rust
pub struct TransactionEvent {
    pub signature: String,
    pub slot: u64,
    pub timestamp: i64,
    pub success: bool,
    pub fee: u64,
    pub instructions: Vec<Instruction>,
}
```

### Message Processing

#### Processors

The service uses a processor pattern for handling different event types:

```rust
pub trait EventProcessor {
    fn should_process(&self, event: &SolanaEvent) -> bool;
    fn process(&self, event: &SolanaEvent) -> ProcessingResult;
}
```

Built-in processors:
- `DexProcessor`: Processes DEX trade events
- `TransactionProcessor`: Processes general transaction events

#### Filtering

Processors support various filtering criteria:

```rust
pub struct DexFilter {
    pub target_programs: Option<Vec<String>>,
    pub min_amount_usd: Option<f64>,
    pub markets: Option<Vec<String>>,
    pub event_types: Option<Vec<EventType>>,
}
```

## Error Handling

### Error Types

The service defines comprehensive error types:

```rust
pub enum KafkaError {
    ConnectionError(String),
    AuthenticationError(String),
    ConfigurationError(String),
    ProcessingError(String),
    CircuitBreakerOpen,
}
```

### Circuit Breaker

The circuit breaker prevents cascading failures:

#### States
- **Closed**: Normal operation, all requests pass through
- **Open**: Circuit is open, requests fail fast
- **Half-Open**: Testing if service has recovered

#### Configuration
- `failure_threshold`: Number of failures to open circuit (default: 5)
- `timeout_duration`: How long to keep circuit open (default: 60s)
- `success_threshold`: Successes needed to close circuit (default: 3)

### Retry Logic

Failed operations are retried with exponential backoff:

```rust
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
}
```

## Integration Examples

### Monitoring Setup

#### Prometheus Configuration
```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'bitquery-kafka'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
    metrics_path: /metrics
```

#### Grafana Dashboard
Key metrics to monitor:
- Message processing rate: `rate(kafka_messages_processed_total[5m])`
- Error rate: `rate(kafka_messages_failed_total[5m])`
- Consumer lag: `kafka_consumer_lag`
- Processing latency: `histogram_quantile(0.95, message_processing_duration_seconds_bucket)`

### Alerting Rules

#### Critical Alerts
```yaml
# High error rate
- alert: HighErrorRate
  expr: rate(kafka_messages_failed_total[5m]) > 0.1
  for: 2m
  
# High consumer lag  
- alert: HighConsumerLag
  expr: kafka_consumer_lag > 1000
  for: 5m

# Circuit breaker open
- alert: CircuitBreakerOpen
  expr: circuit_breaker_state == 1
  for: 1m
```

## Security Considerations

### Authentication
- SASL authentication with username/password
- SSL/TLS encryption for production deployments
- Certificate-based authentication support

### Network Security
- Health check endpoint should be restricted to monitoring systems
- Metrics endpoint should be restricted to monitoring infrastructure
- Consider firewall rules for Kafka broker access

### Data Privacy
- No sensitive data is logged by default
- Configurable log levels to control information disclosure
- Option to redact sensitive fields in logs

## Performance Characteristics

### Throughput
- **Peak throughput**: ~50,000 messages/second (depending on message size)
- **Typical throughput**: 10,000-20,000 messages/second
- **Batch processing**: Improves throughput by 3-5x

### Latency
- **P50 processing latency**: <1ms
- **P95 processing latency**: <5ms  
- **P99 processing latency**: <10ms

### Resource Usage
- **Memory**: 200-500MB typical, 1GB peak
- **CPU**: 0.5-2 cores depending on throughput
- **Network**: Varies with message volume and compression

### Scaling Guidelines
- Scale horizontally by adding consumer instances
- Use different consumer group IDs for parallel processing
- Monitor consumer lag to determine scaling needs
- Consider partition count for optimal distribution
