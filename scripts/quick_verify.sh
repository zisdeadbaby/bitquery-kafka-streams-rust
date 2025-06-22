#!/bin/bash
# Quick verification script
set -e

echo "🔍 Running quick verification..."

# 1. Check if .env exists
if [ -f .env ]; then
    echo "✅ .env file found"
else
    echo "❌ .env file not found"
    exit 1
fi

# 2. Check if project builds
echo "🔨 Building project..."
cd streams/kafka/bitquery-solana-kafka
if cargo build --examples >/dev/null 2>&1; then
    echo "✅ Project builds successfully"
else
    echo "❌ Build failed"
    exit 1
fi

# 3. Check if examples exist
if [ -f "target/debug/examples/connection_test" ]; then
    echo "✅ Connection test example built"
else
    echo "❌ Connection test example not found"
fi

if [ -f "target/debug/examples/health_check" ]; then
    echo "✅ Health check example built"
else
    echo "❌ Health check example not found"
fi

echo ""
echo "🎉 Quick verification completed!"
echo ""
echo "Next steps:"
echo "1. Set up production environment variables"
echo "2. Configure SSL certificates (if needed)"
echo "3. Run connection tests"
echo "4. Deploy with docker-compose or kubernetes"
