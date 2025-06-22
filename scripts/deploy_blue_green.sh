#!/bin/bash
# Blue-Green Deployment Script for Bitquery Solana Kafka Consumer

set -e

# Configuration
APP_NAME="bitquery-solana-kafka"
DOCKER_REGISTRY="your-registry.com"  # Update with your registry
STAGING_PORT=8081
PRODUCTION_PORT=8080
HEALTH_TIMEOUT=60

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')] $1${NC}"
}

warn() {
    echo -e "${YELLOW}[$(date +'%Y-%m-%d %H:%M:%S')] WARNING: $1${NC}"
}

error() {
    echo -e "${RED}[$(date +'%Y-%m-%d %H:%M:%S')] ERROR: $1${NC}"
}

# Get current version
CURRENT_VERSION=$(git rev-parse --short HEAD)
NEW_VERSION="${CURRENT_VERSION}-$(date +%Y%m%d-%H%M%S)"

log "Starting blue-green deployment for $APP_NAME"
log "New version: $NEW_VERSION"

# 1. Build new version
log "Building new Docker image..."
docker build -t "${APP_NAME}:${NEW_VERSION}" .
docker tag "${APP_NAME}:${NEW_VERSION}" "${APP_NAME}:green"

# 2. Deploy to staging (green environment)
log "Deploying to green environment..."

# Create green docker-compose file if it doesn't exist
cat > docker-compose.green.yml << EOF
version: '3.8'
services:
  bitquery-kafka-green:
    image: ${APP_NAME}:green
    container_name: ${APP_NAME}-green
    ports:
      - "${STAGING_PORT}:8080"
    environment:
      - HEALTH_CHECK_PORT=8080
      - KAFKA_GROUP_ID=solana_113-dex-monitor-staging-01
    env_file:
      - .env
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s
EOF

# Start green environment
docker-compose -f docker-compose.green.yml down || true
docker-compose -f docker-compose.green.yml up -d

# 3. Health check green environment
log "Performing health checks on green environment..."
sleep 10  # Give container time to start

# Wait for green to be healthy
HEALTH_CHECK_COUNT=0
MAX_HEALTH_CHECKS=$((HEALTH_TIMEOUT / 5))

while [ $HEALTH_CHECK_COUNT -lt $MAX_HEALTH_CHECKS ]; do
    if curl -f http://localhost:$STAGING_PORT/health > /dev/null 2>&1; then
        log "Green environment is healthy!"
        break
    else
        warn "Health check failed, retrying in 5 seconds... ($((HEALTH_CHECK_COUNT + 1))/$MAX_HEALTH_CHECKS)"
        sleep 5
        HEALTH_CHECK_COUNT=$((HEALTH_CHECK_COUNT + 1))
    fi
done

if [ $HEALTH_CHECK_COUNT -eq $MAX_HEALTH_CHECKS ]; then
    error "Green environment failed health checks"
    docker-compose -f docker-compose.green.yml logs
    docker-compose -f docker-compose.green.yml down
    exit 1
fi

# 4. Run smoke tests on green
log "Running smoke tests on green environment..."
sleep 10  # Let it process some messages

# Check metrics endpoint
if curl -f http://localhost:$STAGING_PORT/metrics > /dev/null 2>&1; then
    log "Metrics endpoint is working"
else
    error "Metrics endpoint failed"
    exit 1
fi

# 5. Switch traffic (if using load balancer, update here)
log "Ready to switch traffic. Green environment validated."
read -p "Continue with traffic switch? (y/N): " -n 1 -r
echo

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    warn "Deployment cancelled by user"
    docker-compose -f docker-compose.green.yml down
    exit 0
fi

# 6. Update production configuration
log "Switching traffic to green environment..."

# Create blue docker-compose file (current production)
if [ -f docker-compose.yml ]; then
    cp docker-compose.yml docker-compose.blue.yml
fi

# Create new production docker-compose file
cat > docker-compose.yml << EOF
version: '3.8'
services:
  bitquery-kafka:
    image: ${APP_NAME}:${NEW_VERSION}
    container_name: ${APP_NAME}-production
    ports:
      - "${PRODUCTION_PORT}:8080"
    environment:
      - HEALTH_CHECK_PORT=8080
      - KAFKA_GROUP_ID=solana_113-dex-monitor-prod-01
    env_file:
      - .env
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s
    deploy:
      resources:
        limits:
          memory: 4G
          cpus: '2.0'
        reservations:
          memory: 2G
          cpus: '1.0'
EOF

# Start new production
docker-compose down || true
docker-compose up -d

# 7. Verify new production is working
log "Verifying new production environment..."
sleep 15

HEALTH_CHECK_COUNT=0
while [ $HEALTH_CHECK_COUNT -lt $MAX_HEALTH_CHECKS ]; do
    if curl -f http://localhost:$PRODUCTION_PORT/health > /dev/null 2>&1; then
        log "New production environment is healthy!"
        break
    else
        warn "Production health check failed, retrying... ($((HEALTH_CHECK_COUNT + 1))/$MAX_HEALTH_CHECKS)"
        sleep 5
        HEALTH_CHECK_COUNT=$((HEALTH_CHECK_COUNT + 1))
    fi
done

if [ $HEALTH_CHECK_COUNT -eq $MAX_HEALTH_CHECKS ]; then
    error "New production environment failed health checks"
    error "Rolling back..."
    
    # Rollback
    if [ -f docker-compose.blue.yml ]; then
        cp docker-compose.blue.yml docker-compose.yml
        docker-compose up -d
        log "Rollback completed"
    fi
    
    docker-compose -f docker-compose.green.yml down
    exit 1
fi

# 8. Monitor for a few minutes
log "Monitoring production for 2 minutes..."
for i in {1..24}; do
    if curl -f http://localhost:$PRODUCTION_PORT/health > /dev/null 2>&1; then
        echo -n "."
    else
        error "Production health check failed during monitoring"
        exit 1
    fi
    sleep 5
done
echo ""

# 9. Clean up
log "Cleaning up green environment..."
docker-compose -f docker-compose.green.yml down

# 10. Tag images
docker tag "${APP_NAME}:${NEW_VERSION}" "${APP_NAME}:latest"
docker tag "${APP_NAME}:${NEW_VERSION}" "${APP_NAME}:blue"  # For next deployment

log "Deployment completed successfully!"
log "New version $NEW_VERSION is now running in production"
log "Old blue environment can be cleaned up manually if needed"

# Optional: Push to registry
read -p "Push images to registry? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    log "Pushing images to registry..."
    docker push "${DOCKER_REGISTRY}/${APP_NAME}:${NEW_VERSION}"
    docker push "${DOCKER_REGISTRY}/${APP_NAME}:latest"
fi
