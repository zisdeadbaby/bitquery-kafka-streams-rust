#!/bin/bash
# Production Readiness Check for Zola Streams
# This script validates the system is ready for production deployment

set -e

echo "🔍 Zola Streams Production Readiness Check"
echo "=========================================="

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

ERRORS=0
WARNINGS=0

check_error() {
    echo -e "${RED}❌ ERROR: $1${NC}"
    ERRORS=$((ERRORS + 1))
}

check_warning() {
    echo -e "${YELLOW}⚠️  WARNING: $1${NC}"
    WARNINGS=$((WARNINGS + 1))
}

check_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

echo
echo "1. Building the project..."
if cargo build --release; then
    check_success "Project builds successfully"
else
    check_error "Project failed to build"
    exit 1
fi

echo
echo "2. Running tests..."
if cargo test --release; then
    check_success "All tests pass"
else
    check_warning "Some tests failed"
fi

echo
echo "3. Checking SSL certificates..."
if [ -f "certs/client.cer.pem" ] && [ -f "certs/client.key.pem" ] && [ -f "certs/server.cer.pem" ]; then
    check_success "SSL certificate files exist"
    
    # Check if they're placeholder certificates
    if grep -q "PLACEHOLDER\|localhost\|development" certs/client.cer.pem 2>/dev/null; then
        check_warning "Client certificate appears to be a placeholder"
        echo "   Replace with production certificates from Bitquery"
    else
        # Check certificate size (real certs are usually larger)
        cert_size=$(wc -c < certs/client.cer.pem)
        if [ "$cert_size" -lt 500 ]; then
            check_warning "Client certificate file is very small (${cert_size} bytes)"
            echo "   This might be a placeholder certificate"
        else
            check_success "Client certificate appears to be production-ready"
        fi
    fi
    
    # Check file permissions
    cert_perms=$(stat -c "%a" certs/client.key.pem 2>/dev/null || stat -f "%A" certs/client.key.pem 2>/dev/null)
    if [[ "$cert_perms" =~ ^6[0-9][0-9]$ ]]; then
        check_success "SSL key file has secure permissions ($cert_perms)"
    else
        check_warning "SSL key file permissions should be 600, currently: $cert_perms"
        echo "   Run: chmod 600 certs/client.key.pem"
    fi
else
    check_error "Missing SSL certificate files"
    echo "   Required: certs/client.cer.pem, certs/client.key.pem, certs/server.cer.pem"
fi

echo
echo "4. Validating configuration..."
if ./target/release/zola-streams --config-check; then
    check_success "Configuration validation passed"
else
    check_error "Configuration validation failed"
fi

echo
echo "5. Testing connectivity (dry run)..."
if ./target/release/zola-streams --dry-run; then
    check_success "Dry run validation passed"
else
    check_error "Dry run validation failed"
fi

echo
echo "6. Checking environment variables..."
required_env_vars=("BITQUERY_USERNAME" "BITQUERY_PASSWORD" "KAFKA_BOOTSTRAP_SERVERS")
for var in "${required_env_vars[@]}"; do
    if [ -n "${!var}" ]; then
        check_success "$var is set"
    else
        check_warning "$var environment variable not set"
        echo "   This may use default configuration values"
    fi
done

echo
echo "7. Checking Docker setup..."
if [ -f "docker/docker-compose.prod.yml" ]; then
    check_success "Production Docker Compose file exists"
    
    # Check if production config references the correct image
    if grep -q "image.*zola-streams" docker/docker-compose.prod.yml; then
        check_success "Production Docker image configured"
    else
        check_warning "Production Docker image may need configuration"
    fi
else
    check_warning "Production Docker Compose file not found"
fi

echo
echo "8. Resource limits check..."
if [ -f "config/production.env" ]; then
    check_success "Production configuration file exists"
    
    # Check for reasonable resource limits
    if grep -q "MAX_MEMORY_MB" config/production.env; then
        memory_limit=$(grep "MAX_MEMORY_MB" config/production.env | cut -d'=' -f2)
        if [ "$memory_limit" -ge 1024 ]; then
            check_success "Memory limit configured: ${memory_limit}MB"
        else
            check_warning "Memory limit seems low: ${memory_limit}MB"
        fi
    else
        check_warning "Memory limit not explicitly configured"
    fi
else
    check_warning "Production configuration file not found"
fi

echo
echo "=========================================="
echo "Production Readiness Summary:"
echo "=========================================="

if [ $ERRORS -eq 0 ] && [ $WARNINGS -eq 0 ]; then
    echo -e "${GREEN}🎉 PRODUCTION READY!${NC}"
    echo "   No errors or warnings found."
    echo "   The system is ready for production deployment."
elif [ $ERRORS -eq 0 ]; then
    echo -e "${YELLOW}⚠️  READY WITH WARNINGS (${WARNINGS} warnings)${NC}"
    echo "   No critical errors, but please review warnings above."
    echo "   The system can be deployed but may need attention."
else
    echo -e "${RED}❌ NOT READY (${ERRORS} errors, ${WARNINGS} warnings)${NC}"
    echo "   Critical errors must be fixed before production deployment."
    exit 1
fi

echo
echo "Next steps:"
echo "1. Review any warnings above"
echo "2. Test in staging environment"
echo "3. Deploy using: docker-compose -f docker/docker-compose.prod.yml up -d"
echo "4. Monitor health: curl http://localhost:8080/health"
echo "5. Check metrics: curl http://localhost:9090/metrics"
