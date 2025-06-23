# Deployment Guide

This guide covers various deployment options for the Bitquery Solana Kafka Streaming Service.

## Overview

The service can be deployed in several ways:
- **Docker Compose**: For development and small production deployments
- **Docker Swarm**: For multi-node Docker deployments
- **Kubernetes**: For scalable production deployments
- **Bare Metal**: Direct deployment on servers

## Docker Deployment

### Prerequisites
- Docker 20.10+
- Docker Compose 2.0+
- Access to Bitquery Kafka endpoints
- Valid SSL certificates (for production)

### Development Deployment

1. **Clone and Configure**:
   ```bash
   git clone <repository-url>
   cd zola-streams
   
   # Copy and edit configuration
   cp config/default.env config/local.env
   nano config/local.env  # Edit with your Kafka credentials
   ```

2. **Start Services**:
   ```bash
   cd docker
   docker-compose up -d
   ```

3. **Verify Deployment**:
   ```bash
   # Check health
   curl http://localhost:8080/health
   
   # Check metrics
   curl http://localhost:9090/metrics
   
   # View logs
   docker-compose logs -f
   ```

### Production Deployment

1. **Production Configuration**:
   ```bash
   # Copy production template
   cp config/production.env config/prod.env
   
   # Edit with production values
   nano config/prod.env
   ```

2. **Deploy with Production Compose**:
   ```bash
   cd docker
   docker-compose -f docker-compose.prod.yml up -d
   ```

3. **Production Checklist**:
   - [ ] SSL certificates properly mounted
   - [ ] Resource limits configured
   - [ ] Logging configured
   - [ ] Monitoring setup
   - [ ] Backup strategy in place
   - [ ] Health checks working

### Docker Swarm Deployment

1. **Initialize Swarm**:
   ```bash
   docker swarm init
   ```

2. **Deploy Stack**:
   ```bash
   docker stack deploy -c docker-compose.prod.yml bitquery-kafka
   ```

3. **Scale Services**:
   ```bash
   docker service scale bitquery-kafka_app=3
   ```

## Kubernetes Deployment

### Prerequisites
- Kubernetes 1.20+
- kubectl configured
- Persistent storage (for certificates and logs)
- Ingress controller (optional)

### Basic Deployment

1. **Create Namespace**:
   ```yaml
   # namespace.yaml
   apiVersion: v1
   kind: Namespace
   metadata:
     name: bitquery-kafka
   ```

2. **Configuration Secret**:
   ```yaml
   # secret.yaml
   apiVersion: v1
   kind: Secret
   metadata:
     name: kafka-config
     namespace: bitquery-kafka
   type: Opaque
   stringData:
     KAFKA_BROKERS: "rpk0.bitquery.io:9092,rpk1.bitquery.io:9092"
     KAFKA_USERNAME: "your-username"
     KAFKA_PASSWORD: "your-password"
     KAFKA_TOPIC: "solana.dextrades.proto"
     KAFKA_GROUP_ID: "your-group-id"
   ```

3. **Deployment Manifest**:
   ```yaml
   # deployment.yaml
   apiVersion: apps/v1
   kind: Deployment
   metadata:
     name: bitquery-kafka
     namespace: bitquery-kafka
   spec:
     replicas: 3
     selector:
       matchLabels:
         app: bitquery-kafka
     template:
       metadata:
         labels:
           app: bitquery-kafka
       spec:
         containers:
         - name: bitquery-kafka
           image: bitquery-kafka:latest
           ports:
           - containerPort: 8080
             name: health
           - containerPort: 9090
             name: metrics
           envFrom:
           - secretRef:
               name: kafka-config
           resources:
             requests:
               memory: "512Mi"
               cpu: "500m"
             limits:
               memory: "1Gi"
               cpu: "1000m"
           livenessProbe:
             httpGet:
               path: /health
               port: 8080
             initialDelaySeconds: 30
             periodSeconds: 10
           readinessProbe:
             httpGet:
               path: /health
               port: 8080
             initialDelaySeconds: 5
             periodSeconds: 5
   ```

4. **Service Manifest**:
   ```yaml
   # service.yaml
   apiVersion: v1
   kind: Service
   metadata:
     name: bitquery-kafka-svc
     namespace: bitquery-kafka
   spec:
     selector:
       app: bitquery-kafka
     ports:
     - name: health
       port: 8080
       targetPort: 8080
     - name: metrics
       port: 9090
       targetPort: 9090
   ```

5. **Deploy**:
   ```bash
   kubectl apply -f namespace.yaml
   kubectl apply -f secret.yaml
   kubectl apply -f deployment.yaml
   kubectl apply -f service.yaml
   ```

### Advanced Kubernetes Configuration

#### Horizontal Pod Autoscaler
```yaml
# hpa.yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: bitquery-kafka-hpa
  namespace: bitquery-kafka
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: bitquery-kafka
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

#### Pod Disruption Budget
```yaml
# pdb.yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: bitquery-kafka-pdb
  namespace: bitquery-kafka
spec:
  minAvailable: 1
  selector:
    matchLabels:
      app: bitquery-kafka
```

#### Network Policy
```yaml
# network-policy.yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: bitquery-kafka-netpol
  namespace: bitquery-kafka
spec:
  podSelector:
    matchLabels:
      app: bitquery-kafka
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: monitoring
    ports:
    - protocol: TCP
      port: 9090
  egress:
  - to: []
    ports:
    - protocol: TCP
      port: 9092  # Kafka
    - protocol: TCP
      port: 443   # HTTPS
```

## Bare Metal Deployment

### Prerequisites
- Linux server (Ubuntu 20.04+ recommended)
- Rust 1.70+ 
- System dependencies installed
- Service user account

### Installation Steps

1. **Prepare System**:
   ```bash
   # Update system
   sudo apt-get update && sudo apt-get upgrade -y
   
   # Install dependencies
   sudo apt-get install -y pkg-config libssl-dev libsasl2-dev \
     ca-certificates curl build-essential
   
   # Install Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source ~/.cargo/env
   ```

2. **Create Service User**:
   ```bash
   sudo useradd --system --home /opt/bitquery-kafka \
     --shell /bin/false bitquery-kafka
   sudo mkdir -p /opt/bitquery-kafka
   sudo chown bitquery-kafka:bitquery-kafka /opt/bitquery-kafka
   ```

3. **Build Application**:
   ```bash
   # Clone repository
   git clone <repository-url> /tmp/bitquery-kafka
   cd /tmp/bitquery-kafka
   
   # Build release
   cargo build --release
   
   # Install binary
   sudo cp target/release/zola-streams /usr/local/bin/
   sudo chmod +x /usr/local/bin/zola-streams
   ```

4. **Configure Service**:
   ```bash
   # Copy configuration
   sudo mkdir -p /etc/bitquery-kafka
   sudo cp config/production.env /etc/bitquery-kafka/
   sudo chown -R bitquery-kafka:bitquery-kafka /etc/bitquery-kafka
   
   # Edit configuration
   sudo nano /etc/bitquery-kafka/production.env
   ```

5. **Create Systemd Service**:
   ```bash
   # Create service file
   sudo tee /etc/systemd/system/bitquery-kafka.service > /dev/null <<EOF
   [Unit]
   Description=Bitquery Solana Kafka Streaming Service
   After=network.target
   Wants=network.target
   
   [Service]
   Type=simple
   User=bitquery-kafka
   Group=bitquery-kafka
   ExecStart=/usr/local/bin/zola-streams
   EnvironmentFile=/etc/bitquery-kafka/production.env
   Restart=always
   RestartSec=5
   LimitNOFILE=65535
   WorkingDirectory=/opt/bitquery-kafka
   
   # Security settings
   NoNewPrivileges=true
   PrivateTmp=true
   ProtectSystem=strict
   ProtectHome=true
   ReadWritePaths=/opt/bitquery-kafka
   
   [Install]
   WantedBy=multi-user.target
   EOF
   ```

6. **Start Service**:
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable bitquery-kafka
   sudo systemctl start bitquery-kafka
   
   # Check status
   sudo systemctl status bitquery-kafka
   ```

## Configuration Management

### Environment-Specific Configuration

#### Development
- Use `config/development.env`
- Local Kafka broker if available
- Debug logging enabled
- Lower resource limits

#### Staging
- Use `config/production.env` with staging values
- Production-like Kafka setup
- Warning level logging
- Production resource limits

#### Production
- Use `config/production.env`
- SSL encryption enabled
- Error level logging
- Full resource allocation
- Monitoring enabled

### Configuration Validation

The service validates configuration at startup:

```bash
# Test configuration
./zola-streams --config-check

# Dry run
./zola-streams --dry-run
```

## Monitoring Setup

### Prometheus Integration

1. **Prometheus Configuration**:
   ```yaml
   # prometheus.yml
   global:
     scrape_interval: 15s
   
   scrape_configs:
   - job_name: 'bitquery-kafka'
     static_configs:
     - targets:
       - 'bitquery-kafka-svc:9090'  # Kubernetes
       - 'localhost:9090'           # Local
     scrape_interval: 10s
     metrics_path: /metrics
   ```

2. **Grafana Dashboard**:
   ```json
   {
     "dashboard": {
       "title": "Bitquery Kafka Streaming",
       "panels": [
         {
           "title": "Message Processing Rate",
           "type": "graph",
           "targets": [
             {
               "expr": "rate(kafka_messages_processed_total[5m])"
             }
           ]
         },
         {
           "title": "Consumer Lag",
           "type": "singlestat",
           "targets": [
             {
               "expr": "kafka_consumer_lag"
             }
           ]
         }
       ]
     }
   }
   ```

### Alerting Rules

```yaml
# alerts.yml
groups:
- name: bitquery-kafka
  rules:
  - alert: HighErrorRate
    expr: rate(kafka_messages_failed_total[5m]) > 0.05
    for: 2m
    labels:
      severity: warning
    annotations:
      summary: High error rate in Kafka processing
      
  - alert: HighConsumerLag
    expr: kafka_consumer_lag > 1000
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: Consumer lag is high
      
  - alert: ServiceDown
    expr: up{job="bitquery-kafka"} == 0
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: Bitquery Kafka service is down
```

## Security Considerations

### Network Security
- Restrict access to health/metrics endpoints
- Use TLS for Kafka connections in production
- Implement proper firewall rules
- Consider VPN for sensitive deployments

### Authentication & Authorization
- Use strong SASL credentials
- Rotate credentials regularly
- Implement least-privilege access
- Audit access logs

### Container Security
- Use non-root user in containers
- Scan images for vulnerabilities
- Keep base images updated
- Use read-only filesystem where possible

## Scaling Guidelines

### Horizontal Scaling
- Multiple consumer instances with same group ID
- Kafka partition count determines max consumers
- Load balance using different consumer groups
- Monitor partition distribution

### Vertical Scaling
- Increase CPU for higher throughput
- Increase memory for larger batches
- Adjust JVM settings for heap size
- Monitor resource utilization

### Performance Tuning
- Batch size optimization
- Compression settings
- Consumer configuration tuning
- Connection pooling

## Troubleshooting

### Common Issues

1. **Connection Issues**:
   ```bash
   # Check network connectivity
   telnet rpk0.bitquery.io 9092
   
   # Verify credentials
   kafka-console-consumer.sh --bootstrap-server rpk0.bitquery.io:9092 \
     --consumer.config client.properties --topic test
   ```

2. **High Consumer Lag**:
   - Check processing performance
   - Increase consumer instances
   - Optimize message processing
   - Review batch sizes

3. **Memory Issues**:
   ```bash
   # Monitor memory usage
   docker stats bitquery-kafka
   
   # Check for memory leaks
   kubectl top pods -n bitquery-kafka
   ```

4. **SSL Certificate Issues**:
   ```bash
   # Verify certificate validity
   openssl x509 -in kafka-cert.pem -text -noout
   
   # Test SSL connection
   openssl s_client -connect rpk0.bitquery.io:9092
   ```

### Log Analysis

```bash
# Container logs
docker logs bitquery-kafka

# Kubernetes logs
kubectl logs -f deployment/bitquery-kafka -n bitquery-kafka

# Systemd logs
journalctl -u bitquery-kafka -f
```

### Health Check Debugging

```bash
# Local health check
curl -v http://localhost:8080/health

# Detailed metrics
curl http://localhost:9090/metrics | grep kafka

# Circuit breaker status
curl http://localhost:8080/health | jq '.circuit_breaker'
```

## Backup and Recovery

### Configuration Backup
- Version control all configuration files
- Backup SSL certificates securely
- Document environment-specific settings

### Data Backup
- Kafka offset management
- State persistence (if applicable)
- Monitoring data retention

### Disaster Recovery
- Multi-region deployment
- Automated failover procedures
- Recovery time objectives (RTO)
- Recovery point objectives (RPO)

## Maintenance

### Regular Tasks
- Update dependencies monthly
- Rotate SSL certificates
- Review and update monitoring alerts
- Performance optimization review

### Upgrade Procedures
1. Test in staging environment
2. Create configuration backups
3. Deploy with rolling updates
4. Verify functionality
5. Monitor for issues

### Capacity Planning
- Monitor resource trends
- Plan for traffic growth
- Review scaling thresholds
- Update resource allocations
