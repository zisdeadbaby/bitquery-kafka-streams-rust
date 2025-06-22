#!/bin/bash
# Pre-production verification script for Bitquery Solana Kafka Integration

set -e  # Exit on any error

echo "🔍 Starting Pre-Production Verification..."

# Load environment variables
if [ -f .env ]; then
    source .env
    echo "✅ Environment variables loaded"
else
    echo "❌ .env file not found"
    exit 1
fi

# 1. Verify SSL certificates (if using SASL_SSL)
if [ "$KAFKA_SECURITY_PROTOCOL" = "SASL_SSL" ]; then
    echo "🔐 Verifying SSL certificates..."
    
    if [ ! -f "$KAFKA_CA_CERT_PATH" ]; then
        echo "❌ CA certificate not found: $KAFKA_CA_CERT_PATH"
        exit 1
    fi
    
    if [ ! -f "$KAFKA_CLIENT_CERT_PATH" ]; then
        echo "❌ Client certificate not found: $KAFKA_CLIENT_CERT_PATH"
        exit 1
    fi
    
    if [ ! -f "$KAFKA_CLIENT_KEY_PATH" ]; then
        echo "❌ Client key not found: $KAFKA_CLIENT_KEY_PATH"
        exit 1
    fi
    
    # Set correct permissions
    chmod 600 "$KAFKA_CA_CERT_PATH" "$KAFKA_CLIENT_CERT_PATH" "$KAFKA_CLIENT_KEY_PATH"
    
    # Verify certificate validity
    openssl x509 -checkend 86400 -noout -in "$KAFKA_CLIENT_CERT_PATH"
    if [ $? -eq 0 ]; then
        echo "✅ SSL certificates are valid"
    else
        echo "⚠️  SSL certificate expires within 24 hours"
    fi
else
    echo "🔓 Using non-SSL connection (SASL_PLAINTEXT)"
fi

# 2. Test basic connectivity
echo "🌐 Testing network connectivity..."
for broker in $(echo $KAFKA_BROKERS | tr ',' ' '); do
    host=$(echo $broker | cut -d':' -f1)
    port=$(echo $broker | cut -d':' -f2)
    
    if timeout 5 bash -c "</dev/tcp/$host/$port"; then
        echo "✅ $broker is reachable"
    else
        echo "❌ $broker is not reachable"
        exit 1
    fi
done

# 3. Build project
echo "🔨 Building project..."
cargo build --release
if [ $? -eq 0 ]; then
    echo "✅ Project builds successfully"
else
    echo "❌ Build failed"
    exit 1
fi

# 4. Run connection test
echo "🔗 Testing Kafka connection..."
timeout 30 cargo run --example connection_test
if [ $? -eq 0 ]; then
    echo "✅ Kafka connection test passed"
else
    echo "❌ Kafka connection test failed"
    exit 1
fi

# 5. Verify consumer group naming
if [[ $KAFKA_GROUP_ID =~ ^solana_113-.+-[a-zA-Z0-9\-]+$ ]]; then
    echo "✅ Consumer group ID follows naming convention: $KAFKA_GROUP_ID"
else
    echo "❌ Consumer group ID does not follow naming convention: $KAFKA_GROUP_ID"
    echo "Expected format: solana_113-{app-name}-{instance-id}"
    exit 1
fi

# 6. Test health check endpoint
echo "🏥 Testing health check endpoint..."
cargo run --example health_check &
HEALTH_PID=$!
sleep 5

curl -f http://localhost:$HEALTH_CHECK_PORT/health > /dev/null 2>&1
if [ $? -eq 0 ]; then
    echo "✅ Health check endpoint is working"
else
    echo "❌ Health check endpoint failed"
    kill $HEALTH_PID 2>/dev/null || true
    exit 1
fi

kill $HEALTH_PID 2>/dev/null || true

# 7. Run tests
echo "🧪 Running tests..."
cargo test
if [ $? -eq 0 ]; then
    echo "✅ All tests passed"
else
    echo "❌ Tests failed"
    exit 1
fi

# 8. Run benchmarks
echo "📊 Running benchmarks..."
cargo bench --bench bench_message_processing
if [ $? -eq 0 ]; then
    echo "✅ Benchmarks completed"
else
    echo "⚠️  Benchmarks had issues (non-critical)"
fi

echo ""
echo "🎉 Pre-production verification completed successfully!"
echo ""
echo "Next steps:"
echo "1. Deploy to staging environment"
echo "2. Run load tests"
echo "3. Monitor metrics and logs"
echo "4. Deploy to production"
