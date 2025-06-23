#!/bin/bash
# Health check script for Docker containers

set -e

# Configuration
HEALTH_PORT=${HEALTH_CHECK_PORT:-8080}
TIMEOUT=${HEALTH_TIMEOUT:-10}
MAX_RETRIES=${HEALTH_MAX_RETRIES:-3}

# Health check function
check_health() {
    local url="http://localhost:${HEALTH_PORT}/health"
    
    if command -v curl >/dev/null 2>&1; then
        curl -f --max-time "$TIMEOUT" "$url" >/dev/null 2>&1
    elif command -v wget >/dev/null 2>&1; then
        wget -q --timeout="$TIMEOUT" -O /dev/null "$url" >/dev/null 2>&1
    else
        echo "Neither curl nor wget found. Installing curl..."
        apt-get update && apt-get install -y curl
        curl -f --max-time "$TIMEOUT" "$url" >/dev/null 2>&1
    fi
}

# Main health check with retries
for i in $(seq 1 "$MAX_RETRIES"); do
    if check_health; then
        echo "Health check passed (attempt $i/$MAX_RETRIES)"
        exit 0
    fi
    
    if [ "$i" -lt "$MAX_RETRIES" ]; then
        echo "Health check failed (attempt $i/$MAX_RETRIES), retrying..."
        sleep 2
    fi
done

echo "Health check failed after $MAX_RETRIES attempts"
exit 1
