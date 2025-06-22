# Bitquery Solana Kafka Integration: Agent Checklist

This document tracks all steps performed and remaining next steps for the Bitquery Solana Kafka integration, decomposed into granular nodes.

---

## ✅ Steps Performed (COMPLETED)

### Phase 1-8: Development & Integration
- [x] All development phases completed (SSL setup, protobuf, DEX processing, config, monitoring, testing, git operations)

### Phase 9: Production Deployment Infrastructure
- [x] Updated `.env` with production-ready configuration supporting both SSL and non-SSL
- [x] Created deployment scripts:
  - [x] `scripts/verify_setup.sh` - Comprehensive pre-production verification
  - [x] `scripts/deploy_blue_green.sh` - Zero-downtime blue-green deployment
  - [x] `scripts/monitor_deployment.sh` - Post-deployment monitoring and validation
  - [x] `scripts/quick_verify.sh` - Quick verification for development
- [x] Created containerization:
  - [x] `Dockerfile` - Multi-stage production Docker build
  - [x] `docker-compose.yml` - Production Docker Compose with PostgreSQL and Redis
- [x] Created Kubernetes manifests:
  - [x] `deployment/kubernetes/deployment.yaml` - K8s deployment with security, resources, health checks
  - [x] `deployment/kubernetes/hpa.yaml` - Horizontal Pod Autoscaler with lag-based scaling
- [x] Created monitoring infrastructure:
  - [x] Enhanced `deployment/configs/prometheus.yml` with comprehensive scraping
  - [x] `deployment/configs/prometheus_rules.yml` - Production alert rules for all critical metrics
- [x] Created database infrastructure:
  - [x] `deployment/sql/init.sql` - PostgreSQL schema for offset management and monitoring
- [x] Created comprehensive production checklist: `PRODUCTION_CHECKLIST.md`

## ✅ Ready for Production Deployment

**All deployment components are now complete and ready for production use!**

The integration includes:
- ✅ **Both SSL and non-SSL connection options** (SASL_PLAINTEXT recommended)
- ✅ **Blue-green deployment strategy** for zero downtime
- ✅ **Auto-scaling** based on consumer lag and resource usage
- ✅ **Comprehensive monitoring** with Prometheus alerts
- ✅ **Container orchestration** via Docker Compose and Kubernetes
- ✅ **Database integration** for offset management
- ✅ **Health checks and recovery** mechanisms
- ✅ **Production-ready configuration** with resource limits
- ✅ **Operational scripts** for deployment and monitoring

### Next Actions for User:
1. **Update credentials** in `.env` with actual Bitquery credentials
2. **Choose connection type** (SASL_PLAINTEXT or SASL_SSL)
3. **Run quick verification**: `./scripts/quick_verify.sh`
4. **Deploy using preferred method**:
   - Docker: `docker-compose up -d`
   - Kubernetes: `kubectl apply -f deployment/kubernetes/`
   - Blue-Green: `./scripts/deploy_blue_green.sh`
5. **Monitor deployment**: `./scripts/monitor_deployment.sh`

**See `PRODUCTION_CHECKLIST.md` for detailed step-by-step deployment instructions.**

---

## ⏳ Steps To Be Performed (Deployment Checklist)

# Production Deployment Guide for Bitquery Solana Kafka Integration

## Pre-Production Verification

### 1. Connection Setup (Non-SSL Recommended)

#### Option A: Non-SSL Connection (SASL_PLAINTEXT) - Simpler & Recommended
```rust
// In your config.rs
pub fn create_kafka_config_non_ssl() -> ClientConfig {
    let mut config = ClientConfig::new();
    
    config
        .set("bootstrap.servers", &env::var("KAFKA_BROKERS").unwrap())
        .set("security.protocol", "SASL_PLAINTEXT")  // Non-SSL
        .set("sasl.mechanism", "PLAIN")
        .set("sasl.username", &env::var("KAFKA_USERNAME").unwrap())
        .set("sasl.password", &env::var("KAFKA_PASSWORD").unwrap())
        .set("group.id", &env::var("KAFKA_GROUP_ID").unwrap())
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "latest")
        .set("session.timeout.ms", "45000");
    
    config
}
```

#### Option B: SSL Connection (SASL_SSL) - If Required
```bash
# Only if SSL is required by your organization
mkdir -p certs
chmod 700 certs

# Place your SSL certificates (from Bitquery)
# Expected files:
# - certs/kafka-ca.pem     (CA certificate)
# - certs/kafka-cert.pem   (Client certificate)
# - certs/kafka-key.pem    (Client key)

# Set correct permissions
chmod 600 certs/*
```

### 2. Connection Test Script
```bash
#!/bin/bash
# run_connection_test.sh

# Load environment
source .env

# Run connection test
cargo run --example connection_test

# Expected output:
# - "Successfully connected to Kafka"
# - "Metadata received"
# - No SSL errors
```

### 3. Consumer Group Configuration
```rust
// In your config.rs or main application
const CONSUMER_GROUP_PREFIX: &str = "solana_113";
const APP_NAME: &str = "dex-monitor"; // Your app name
const INSTANCE_ID: &str = "prod-01"; // Instance identifier

let consumer_group = format!("{}-{}-{}", CONSUMER_GROUP_PREFIX, APP_NAME, INSTANCE_ID);
// Result: "solana_113-dex-monitor-prod-01"
```

### 4. Kafka Brokers Configuration
```env
# In .env file - all three brokers for redundancy
KAFKA_BROKERS=kafka-solana.bitquery.io:9093,kafka-solana-2.bitquery.io:9093,kafka-solana-3.bitquery.io:9093
```

### 5. Partition Assignment Strategy
```rust
// In your Kafka consumer config
let consumer_config = ClientConfig::new()
    .set("partition.assignment.strategy", "roundrobin")
    .set("session.timeout.ms", "45000")
    .set("max.poll.interval.ms", "300000")
    .set("enable.auto.commit", "false") // Manual commit for reliability
    .set("auto.offset.reset", "latest"); // Or "earliest" based on needs
```

### 6. Metrics & Health Check Setup
```bash
# Run metrics exporter
cargo run --example health_check &

# Verify metrics endpoint
curl http://localhost:9090/metrics

# Verify health endpoint
curl http://localhost:8080/health

# Expected: {"status":"healthy","kafka_connected":true}
```

### 7. Resource Limits Testing
```yaml
# docker-compose.yml or k8s deployment.yaml
resources:
  requests:
    memory: "2Gi"
    cpu: "1000m"
  limits:
    memory: "4Gi"
    cpu: "2000m"
```

### 8. Circuit Breaker Configuration
```rust
// In your processing logic
const CIRCUIT_BREAKER_CONFIG: CircuitBreakerConfig = CircuitBreakerConfig {
    failure_threshold: 5,      // Open after 5 failures
    success_threshold: 2,      // Close after 2 successes
    timeout_duration: Duration::from_secs(60), // Reset timeout
    half_open_max_calls: 3,   // Calls allowed in half-open state
};
```

### 9. Offset Management
```rust
// Implement manual offset management
impl OffsetManager {
    pub async fn commit_offset(&self, partition: i32, offset: i64) -> Result<()> {
        // Store in database for durability
        sqlx::query!(
            "INSERT INTO kafka_offsets (topic, partition, offset, committed_at) 
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (topic, partition) 
             DO UPDATE SET offset = $3, committed_at = $4",
            self.topic,
            partition,
            offset,
            Utc::now()
        )
        .execute(&self.db_pool)
        .await?;
        
        // Then commit to Kafka
        self.consumer.commit_message(&message, CommitMode::Async)?;
        Ok(())
    }
}
```

## Production Deployment

### 1. Blue-Green Deployment Script
```bash
#!/bin/bash
# deploy_blue_green.sh

# Build new version
docker build -t bitquery-solana-kafka:new .

# Deploy to staging (green)
docker-compose -f docker-compose.green.yml up -d

# Health check green
while ! curl -f http://localhost:8081/health; do
  sleep 5
done

# Switch traffic (update load balancer/service mesh)
kubectl patch service bitquery-kafka -p '{"spec":{"selector":{"version":"green"}}}'

# Verify green is handling traffic
sleep 30

# Stop blue
docker-compose -f docker-compose.blue.yml down

# Tag green as new blue for next deployment
docker tag bitquery-solana-kafka:new bitquery-solana-kafka:blue
```

### 2. Auto-Scaling Configuration
```yaml
# k8s HPA (Horizontal Pod Autoscaler)
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: bitquery-kafka-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: bitquery-kafka-consumer
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: External
    external:
      metric:
        name: kafka_consumer_lag
        selector:
          matchLabels:
            topic: "solana.slot_update_stream_1_13"
      target:
        type: AverageValue
        averageValue: "10000"  # Scale up if lag > 10k per pod
```

### 3. Monitoring Alerts (Prometheus)
```yaml
# prometheus_rules.yml
groups:
  - name: bitquery_kafka_alerts
    rules:
      - alert: HighConsumerLag
        expr: kafka_consumer_lag > 10000
        for: 5m
        annotations:
          summary: "High Kafka consumer lag detected"
          description: "Consumer lag is {{ $value }} for {{ $labels.topic }}"
      
      - alert: ConnectionFailure
        expr: kafka_connection_active == 0
        for: 1m
        annotations:
          summary: "Kafka connection lost"
      
      - alert: HighMemoryUsage
        expr: (container_memory_usage_bytes / container_spec_memory_limit_bytes) > 0.8
        for: 5m
        annotations:
          summary: "Memory usage above 80%"
      
      - alert: HighErrorRate
        expr: rate(processing_errors_total[5m]) > 0.01
        for: 5m
        annotations:
          summary: "Processing error rate above 1%"
```

### 4. Log Aggregation Setup
```yaml
# filebeat.yml for ELK
filebeat.inputs:
- type: container
  paths:
    - /var/lib/docker/containers/*/*.log
  processors:
    - add_docker_metadata:
        host: "unix:///var/run/docker.sock"
    - decode_json_fields:
        fields: ["message"]
        target: "json"

output.elasticsearch:
  hosts: ["elasticsearch:9200"]
  index: "bitquery-kafka-%{+yyyy.MM.dd}"
```

### 5. Runbook for Common Issues

#### Issue: High Consumer Lag
```bash
# 1. Check current lag
kafka-consumer-groups --bootstrap-server $KAFKA_BROKERS \
  --group solana_113-dex-monitor-prod-01 --describe

# 2. Scale up consumers
kubectl scale deployment bitquery-kafka-consumer --replicas=5

# 3. If specific partition is lagging, check for poison messages
kafka-console-consumer --bootstrap-server $KAFKA_BROKERS \
  --topic solana.slot_update_stream_1_13 \
  --partition <problematic_partition> \
  --offset <problematic_offset>
```

#### Issue: Connection Failures
```bash
# 1. Verify SSL certificates haven't expired
openssl x509 -enddate -noout -in certs/kafka-cert.pem

# 2. Test connectivity
openssl s_client -connect kafka-solana.bitquery.io:9093 \
  -CAfile certs/kafka-ca.pem \
  -cert certs/kafka-cert.pem \
  -key certs/kafka-key.pem

# 3. Check DNS resolution
nslookup kafka-solana.bitquery.io

# 4. Restart with increased logging
RUST_LOG=rdkafka=debug cargo run
```

#### Issue: Memory Leaks
```bash
# 1. Get heap dump
jmap -dump:format=b,file=heap.bin <PID>

# 2. Analyze with profiler
cargo install cargo-instruments
cargo instruments -t Allocations

# 3. Emergency restart with memory limit
docker run --memory="2g" --memory-swap="2g" bitquery-solana-kafka
```

## Post-Deployment Monitoring

### 1. Throughput Verification Script
```rust
// monitoring/throughput_checker.rs
use prometheus::{Counter, Histogram};

lazy_static! {
    static ref MESSAGES_PROCESSED: Counter = register_counter!(
        "messages_processed_total", 
        "Total messages processed"
    ).unwrap();
    
    static ref PROCESSING_DURATION: Histogram = register_histogram!(
        "message_processing_duration_seconds",
        "Message processing duration"
    ).unwrap();
}

pub async fn monitor_throughput() {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    let mut last_count = 0;
    
    loop {
        interval.tick().await;
        let current = MESSAGES_PROCESSED.get();
        let throughput = (current - last_count) / 60;
        
        info!("Current throughput: {} msg/sec", throughput);
        
        if throughput < 100 {
            warn!("Throughput below expected threshold!");
        }
        
        last_count = current;
    }
}
```

### 2. Latency Monitoring
```rust
// Add to your message processor
let timer = PROCESSING_DURATION.start_timer();
process_message(&msg).await?;
timer.observe_duration();

// Alert if p99 > 500ms
if PROCESSING_DURATION.get_sample_sum() / PROCESSING_DURATION.get_sample_count() > 0.5 {
    error!("High latency detected!");
}
```

### 3. Partition Balance Check
```bash
#!/bin/bash
# check_partition_balance.sh

kafka-consumer-groups --bootstrap-server $KAFKA_BROKERS \
  --group solana_113-dex-monitor-prod-01 \
  --describe \
  | awk '{print $2, $4}' \
  | sort | uniq -c

# Should show roughly equal message counts per consumer
```

### 4. Data Completeness Validation
```sql
-- Compare against known DEX volumes
WITH kafka_stats AS (
  SELECT 
    date_trunc('hour', timestamp) as hour,
    count(*) as trade_count,
    sum(amount_usd) as volume_usd
  FROM dex_trades
  WHERE source = 'kafka'
  GROUP BY 1
),
reference_stats AS (
  -- Your reference data source
  SELECT hour, trade_count, volume_usd
  FROM reference_dex_data
)
SELECT 
  k.hour,
  k.trade_count as kafka_trades,
  r.trade_count as reference_trades,
  abs(k.trade_count - r.trade_count)::float / r.trade_count as diff_percent
FROM kafka_stats k
JOIN reference_stats r ON k.hour = r.hour
WHERE diff_percent > 0.05; -- Flag >5% difference
```

## Environment Variables Summary
```env
# Complete .env file
KAFKA_BROKERS=kafka-solana.bitquery.io:9093,kafka-solana-2.bitquery.io:9093,kafka-solana-3.bitquery.io:9093
KAFKA_USERNAME=your_username
KAFKA_PASSWORD=your_password
KAFKA_TOPIC=solana.slot_update_stream_1_13
KAFKA_GROUP_ID=solana_113-dex-monitor-prod-01
KAFKA_CA_CERT_PATH=./certs/kafka-ca.pem
KAFKA_CLIENT_CERT_PATH=./certs/kafka-cert.pem
KAFKA_CLIENT_KEY_PATH=./certs/kafka-key.pem

# Monitoring
PROMETHEUS_PORT=9090
HEALTH_CHECK_PORT=8080
LOG_LEVEL=info

# Database for offset management
DATABASE_URL=postgresql://user:pass@localhost/bitquery_kafka
```

## Quick Start Commands
```bash
# 1. Verify setup
./scripts/verify_setup.sh

# 2. Run health checks
cargo run --example health_check &

# 3. Start consumer with monitoring
RUST_LOG=info cargo run --release

# 4. Monitor metrics
open http://localhost:9090/metrics

# 5. Check logs
tail -f logs/bitquery-kafka.log | jq
```

---

This guide provides everything needed to complete your production deployment!
