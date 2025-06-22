# Production Deployment Checklist

## ✅ Completed Deployment Components

### Infrastructure & Configuration
- [x] Created comprehensive `.env` configuration with both SSL and non-SSL options
- [x] Created deployment scripts:
  - [x] `scripts/verify_setup.sh` - Pre-production verification
  - [x] `scripts/deploy_blue_green.sh` - Blue-green deployment
  - [x] `scripts/monitor_deployment.sh` - Post-deployment monitoring
  - [x] `scripts/quick_verify.sh` - Quick verification
- [x] Created `Dockerfile` for containerized deployment
- [x] Created `docker-compose.yml` with production configuration
- [x] Created Kubernetes deployment manifests:
  - [x] `deployment/kubernetes/deployment.yaml`
  - [x] `deployment/kubernetes/hpa.yaml` (with auto-scaling)
- [x] Created Prometheus monitoring configuration:
  - [x] `deployment/configs/prometheus.yml`
  - [x] `deployment/configs/prometheus_rules.yml`
- [x] Created PostgreSQL initialization:
  - [x] `deployment/sql/init.sql`

### Code & Examples
- [x] All example applications working:
  - [x] `connection_test.rs` - Connection validation
  - [x] `dex_monitor.rs` - DEX trade monitoring
  - [x] `health_check.rs` - Health endpoint
  - [x] `connection_recovery.rs` - Error handling
  - [x] `production_config.rs` - Production settings
- [x] All tests and benchmarks implemented
- [x] Metrics and monitoring integrated

## 📋 Next Steps for Production Deployment

### Phase 1: Environment Setup
1. **Configure Environment Variables**
   ```bash
   # Update .env with your actual credentials
   cp .env .env.production
   # Edit KAFKA_USERNAME, KAFKA_PASSWORD with real values
   ```

2. **Choose Connection Type**
   - **Option A (Recommended)**: Non-SSL connection
     ```bash
     export KAFKA_SECURITY_PROTOCOL=SASL_PLAINTEXT
     ```
   - **Option B**: SSL connection (if required)
     ```bash
     export KAFKA_SECURITY_PROTOCOL=SASL_SSL
     # Place certificates in certs/ directory
     ```

### Phase 2: Pre-Deployment Verification
```bash
# Run verification script
./scripts/quick_verify.sh

# Build production binary
cd streams/kafka/bitquery-solana-kafka
cargo build --release

# Test connection (with real credentials)
cargo run --example connection_test

# Test health endpoint
cargo run --example health_check &
curl http://localhost:8080/health
```

### Phase 3: Docker Deployment
```bash
# Build Docker image
docker build -t bitquery-solana-kafka:latest .

# Run with docker-compose
docker-compose up -d

# Verify health
curl http://localhost:8080/health
curl http://localhost:9090/metrics
```

### Phase 4: Kubernetes Deployment (Alternative)
```bash
# Create namespace
kubectl create namespace bitquery

# Apply configurations
kubectl apply -f deployment/kubernetes/

# Check deployment
kubectl get pods -n bitquery
kubectl logs -n bitquery deployment/bitquery-kafka-consumer
```

### Phase 5: Monitoring Setup
```bash
# Start Prometheus (if not using existing)
docker run -d -p 9091:9090 \
  -v ./deployment/configs/prometheus.yml:/etc/prometheus/prometheus.yml \
  prom/prometheus

# View metrics
open http://localhost:9091

# Run monitoring script
./scripts/monitor_deployment.sh
```

### Phase 6: Production Deployment
```bash
# Use blue-green deployment
./scripts/deploy_blue_green.sh

# Or manual deployment
docker-compose -f docker-compose.yml up -d
```

## 🔧 Operational Commands

### Health Checks
```bash
# Application health
curl http://localhost:8080/health

# Metrics
curl http://localhost:9090/metrics

# Container logs
docker logs bitquery-kafka-production

# Database connection (if using PostgreSQL)
psql -h localhost -U kafka_user -d bitquery_kafka
```

### Scaling
```bash
# Docker Compose
docker-compose scale bitquery-kafka=3

# Kubernetes
kubectl scale deployment bitquery-kafka-consumer --replicas=5 -n bitquery
```

### Monitoring
```bash
# Check consumer lag
kafka-consumer-groups --bootstrap-server $KAFKA_BROKERS \
  --group $KAFKA_GROUP_ID --describe

# View recent logs
docker logs --tail 100 -f bitquery-kafka-production

# Monitor system resources
docker stats bitquery-kafka-production
```

### Troubleshooting
```bash
# Connection issues
telnet kafka-solana.bitquery.io 9093

# SSL issues (if using SSL)
openssl s_client -connect kafka-solana.bitquery.io:9093

# Debug mode
RUST_LOG=debug cargo run --example connection_test

# Container debugging
docker exec -it bitquery-kafka-production /bin/bash
```

## 📊 Success Metrics

Your deployment is successful when:
- ✅ Health endpoint returns 200 OK
- ✅ Metrics endpoint shows active message processing
- ✅ Consumer lag < 10,000 messages
- ✅ Error rate < 1%
- ✅ Memory usage < 80%
- ✅ All containers are running and healthy
- ✅ No critical alerts in monitoring

## 🚨 Rollback Plan

If deployment fails:
```bash
# Docker Compose rollback
docker-compose down
docker-compose -f docker-compose.blue.yml up -d

# Kubernetes rollback
kubectl rollout undo deployment/bitquery-kafka-consumer -n bitquery

# Manual verification
curl http://localhost:8080/health
```

## 📈 Performance Tuning

After deployment, monitor and tune:
- Consumer group rebalancing
- Memory allocation
- JVM GC settings (if using Java tools)
- Network buffer sizes
- Kafka consumer configurations

This completes the production deployment setup for your Bitquery Solana Kafka integration!
