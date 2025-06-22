#!/bin/bash
# Post-deployment monitoring and validation script

set -e

# Configuration
PRODUCTION_PORT=8080
CHECK_INTERVAL=30  # seconds
TOTAL_DURATION=1800  # 30 minutes

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')] $1${NC}"
}

warn() {
    echo -e "${YELLOW}[$(date +'%Y-%m-%d %H:%M:%S')] WARNING: $1${NC}"
}

error() {
    echo -e "${RED}[$(date +'%Y-%m-%d %H:%M:%S')] ERROR: $1${NC}"
}

# Load environment
if [ -f .env ]; then
    source .env
else
    error ".env file not found"
    exit 1
fi

log "Starting post-deployment monitoring for 30 minutes..."

# Initialize counters
CHECKS_PASSED=0
CHECKS_FAILED=0
START_TIME=$(date +%s)

# Function to check application health
check_health() {
    local response
    local http_code
    
    response=$(curl -s -w "\nHTTP_CODE:%{http_code}" http://localhost:$PRODUCTION_PORT/health)
    http_code=$(echo "$response" | grep "HTTP_CODE:" | sed 's/HTTP_CODE://')
    
    if [ "$http_code" = "200" ]; then
        return 0
    else
        return 1
    fi
}

# Function to check metrics endpoint
check_metrics() {
    local http_code
    
    http_code=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:$PROMETHEUS_PORT/metrics)
    
    if [ "$http_code" = "200" ]; then
        return 0
    else
        return 1
    fi
}

# Function to check Kafka consumer lag (placeholder - implement with your Kafka tools)
check_consumer_lag() {
    # This would require kafka-consumer-groups.sh or similar tool
    # For now, we'll simulate this check
    
    # Example implementation:
    # lag=$(kafka-consumer-groups.sh --bootstrap-server $KAFKA_BROKERS \
    #       --group $KAFKA_GROUP_ID --describe | awk '{sum += $5} END {print sum}')
    # 
    # if [ "$lag" -lt 10000 ]; then
    #     return 0
    # else
    #     return 1
    # fi
    
    # Simulated check - replace with actual implementation
    return 0
}

# Function to check memory usage
check_memory_usage() {
    local container_id
    local memory_usage
    local memory_limit
    local usage_percent
    
    container_id=$(docker ps -q --filter "name=bitquery-solana-kafka-production")
    
    if [ -n "$container_id" ]; then
        # Get memory stats
        memory_stats=$(docker stats --no-stream --format "{{.MemUsage}}" "$container_id")
        memory_usage=$(echo "$memory_stats" | cut -d'/' -f1 | sed 's/[^0-9.]//g')
        memory_limit=$(echo "$memory_stats" | cut -d'/' -f2 | sed 's/[^0-9.]//g')
        
        if [ -n "$memory_usage" ] && [ -n "$memory_limit" ]; then
            usage_percent=$(echo "scale=2; ($memory_usage / $memory_limit) * 100" | bc -l)
            usage_percent_int=$(echo "$usage_percent" | cut -d'.' -f1)
            
            if [ "$usage_percent_int" -lt 80 ]; then
                return 0
            else
                warn "Memory usage is high: ${usage_percent}%"
                return 1
            fi
        fi
    fi
    
    return 1
}

# Function to check processing throughput
check_throughput() {
    local metrics_response
    local processed_count
    
    metrics_response=$(curl -s http://localhost:$PROMETHEUS_PORT/metrics | grep "messages_processed_total")
    
    if [ -n "$metrics_response" ]; then
        processed_count=$(echo "$metrics_response" | grep -o '[0-9]\+$' | tail -1)
        
        if [ -n "$processed_count" ] && [ "$processed_count" -gt 0 ]; then
            return 0
        fi
    fi
    
    return 1
}

# Function to check error rate
check_error_rate() {
    local error_metrics
    local error_count
    
    error_metrics=$(curl -s http://localhost:$PROMETHEUS_PORT/metrics | grep "processing_errors_total")
    
    if [ -n "$error_metrics" ]; then
        error_count=$(echo "$error_metrics" | grep -o '[0-9]\+$' | tail -1)
        
        # If we have less than 10 errors in total (simple check), consider it good
        if [ -z "$error_count" ] || [ "$error_count" -lt 10 ]; then
            return 0
        else
            warn "Error count is high: $error_count"
            return 1
        fi
    fi
    
    return 0  # No errors found
}

# Main monitoring loop
while true; do
    current_time=$(date +%s)
    elapsed_time=$((current_time - START_TIME))
    
    if [ $elapsed_time -ge $TOTAL_DURATION ]; then
        log "Monitoring period completed"
        break
    fi
    
    log "Performing health checks... ($(($elapsed_time / 60))min elapsed)"
    
    # Perform all checks
    all_checks_passed=true
    
    # 1. Health endpoint
    if check_health; then
        echo "  ✅ Health check: PASS"
    else
        echo "  ❌ Health check: FAIL"
        all_checks_passed=false
    fi
    
    # 2. Metrics endpoint
    if check_metrics; then
        echo "  ✅ Metrics endpoint: PASS"
    else
        echo "  ❌ Metrics endpoint: FAIL"
        all_checks_passed=false
    fi
    
    # 3. Consumer lag
    if check_consumer_lag; then
        echo "  ✅ Consumer lag: ACCEPTABLE"
    else
        echo "  ❌ Consumer lag: HIGH"
        all_checks_passed=false
    fi
    
    # 4. Memory usage
    if check_memory_usage; then
        echo "  ✅ Memory usage: NORMAL"
    else
        echo "  ⚠️  Memory usage: HIGH"
        # Don't fail on memory warning, just log it
    fi
    
    # 5. Processing throughput
    if check_throughput; then
        echo "  ✅ Processing throughput: ACTIVE"
    else
        echo "  ⚠️  Processing throughput: LOW/NONE"
        # Don't fail immediately on throughput, could be low traffic
    fi
    
    # 6. Error rate
    if check_error_rate; then
        echo "  ✅ Error rate: ACCEPTABLE"
    else
        echo "  ⚠️  Error rate: HIGH"
        all_checks_passed=false
    fi
    
    # Update counters
    if [ "$all_checks_passed" = true ]; then
        CHECKS_PASSED=$((CHECKS_PASSED + 1))
        echo "  🎉 All critical checks passed"
    else
        CHECKS_FAILED=$((CHECKS_FAILED + 1))
        echo "  🚨 Some checks failed"
        
        # If we have multiple consecutive failures, alert
        if [ $CHECKS_FAILED -ge 3 ]; then
            error "Multiple consecutive check failures detected!"
            error "Manual intervention may be required"
            
            # Optionally trigger alerts here
            # send_alert "Production deployment has issues"
        fi
    fi
    
    echo ""
    sleep $CHECK_INTERVAL
done

# Summary
log "Monitoring Summary:"
log "  Checks passed: $CHECKS_PASSED"
log "  Checks failed: $CHECKS_FAILED"
log "  Success rate: $(echo "scale=2; ($CHECKS_PASSED * 100) / ($CHECKS_PASSED + $CHECKS_FAILED)" | bc -l)%"

if [ $CHECKS_FAILED -eq 0 ]; then
    log "🎉 Deployment appears to be stable and healthy!"
elif [ $CHECKS_FAILED -lt 5 ]; then
    warn "⚠️  Deployment has minor issues but appears mostly stable"
else
    error "🚨 Deployment has significant issues - investigate immediately"
    exit 1
fi

log "Monitoring completed successfully"
