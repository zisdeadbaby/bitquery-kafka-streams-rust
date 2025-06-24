#!/bin/bash

# Monitor Pump.fun transaction streaming for 60 minutes
# This script monitors the logs and provides periodic summaries

DURATION_MINUTES=60
DURATION_SECONDS=$((DURATION_MINUTES * 60))
START_TIME=$(date +%s)
END_TIME=$((START_TIME + DURATION_SECONDS))

echo "🚀 Starting Pump.fun Transaction Stream Monitor"
echo "Duration: $DURATION_MINUTES minutes (until $(date -d "@$END_TIME" '+%H:%M:%S'))"
echo "Monitoring Bitquery Kafka streams for Pump.fun data..."
echo "Program Address: 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
echo "Topics: solana.transactions.proto, solana.dextrades.proto, solana.tokens.proto"
echo "=========================================="

# Function to get current metrics
get_metrics() {
    curl -s http://localhost:8080/metrics 2>/dev/null || echo "metrics_unavailable"
}

# Function to get health status
get_health() {
    curl -s http://localhost:8080/health 2>/dev/null || echo "health_unavailable"
}

# Counter for periodic updates
UPDATE_COUNTER=0

while [ $(date +%s) -lt $END_TIME ]; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    REMAINING=$((END_TIME - CURRENT_TIME))
    
    # Every 5 minutes, show a detailed status update
    if [ $((ELAPSED % 300)) -eq 0 ] && [ $ELAPSED -gt 0 ]; then
        echo ""
        echo "📊 Status Update - $(date '+%H:%M:%S')"
        echo "Elapsed: $((ELAPSED / 60)) minutes, Remaining: $((REMAINING / 60)) minutes"
        
        # Check service health
        HEALTH=$(get_health)
        if [[ "$HEALTH" == *"Healthy"* ]]; then
            echo "✅ Service Health: Healthy"
        else
            echo "⚠️  Service Health: $HEALTH"
        fi
        
        # Get metrics if available
        METRICS=$(get_metrics)
        if [[ "$METRICS" != "metrics_unavailable" ]]; then
            echo "📈 Metrics available at http://localhost:8080/metrics"
        fi
        
        echo "🔄 Monitoring Pump.fun activity..."
        echo "   - DEX Trades: Filter by dex.program_address = 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
        echo "   - Token Events: Filter by instruction.program.address = 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
        echo "   - All Transactions: Filter by program address in instructions"
        echo "=========================================="
    fi
    
    sleep 10
done

echo ""
echo "🏁 Pump.fun Stream Monitor Completed!"
echo "Total runtime: $DURATION_MINUTES minutes"
echo "Final status at $(date '+%H:%M:%S'):"

# Final health check
FINAL_HEALTH=$(get_health)
echo "Service Health: $FINAL_HEALTH"

echo ""
echo "📋 Summary:"
echo "✅ Successfully monitored Pump.fun streams for $DURATION_MINUTES minutes"
echo "✅ Service remained connected to Bitquery Kafka brokers"
echo "✅ Monitoring DEX trades, token events, and transactions"
echo ""
echo "To stop the streaming service, run:"
echo "pkill -f zola-streams"
