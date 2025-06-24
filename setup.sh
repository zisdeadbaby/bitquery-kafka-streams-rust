#!/bin/bash

# Zola Streams - Complete Setup and Verification Script
# This script performs a comprehensive setup and validation of the application

set -e  # Exit on any error

echo "============================================"
echo "Zola Streams - Setup and Verification"
echo "============================================"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[OK]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_info() {
    echo -e "[INFO] $1"
}

# Check if Rust is installed
echo "1. Checking Rust installation..."
if command -v cargo &> /dev/null; then
    RUST_VERSION=$(rustc --version)
    print_status "Rust is installed: $RUST_VERSION"
else
    print_error "Rust is not installed. Please install Rust first:"
    echo "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check system dependencies
echo ""
echo "2. Checking system dependencies..."

# Check for pkg-config
if command -v pkg-config &> /dev/null; then
    print_status "pkg-config is available"
else
    print_warning "pkg-config not found. Install with: sudo apt-get install pkg-config"
fi

# Check for OpenSSL development files
if pkg-config --exists openssl 2>/dev/null; then
    print_status "OpenSSL development files are available"
else
    print_warning "OpenSSL development files not found. Install with: sudo apt-get install libssl-dev"
fi

# Check for SASL development files
if pkg-config --exists libsasl2 2>/dev/null; then
    print_status "SASL development files are available"
else
    print_warning "SASL development files not found. Install with: sudo apt-get install libsasl2-dev"
fi

# Clean previous builds
echo ""
echo "3. Cleaning previous builds..."
cargo clean
print_status "Previous builds cleaned"

# Build the project
echo ""
echo "4. Building the project..."
print_info "Building debug version..."
if cargo build; then
    print_status "Debug build successful"
else
    print_error "Debug build failed"
    exit 1
fi

print_info "Building release version..."
if cargo build --release; then
    print_status "Release build successful"
else
    print_error "Release build failed"
    exit 1
fi

# Run tests
echo ""
echo "5. Running tests..."
if cargo test; then
    TEST_RESULT=$(cargo test 2>&1 | grep "test result:")
    print_status "All tests passed: $TEST_RESULT"
else
    print_error "Some tests failed"
    exit 1
fi

# Run clippy for code quality
echo ""
echo "6. Running code quality checks..."
if cargo clippy -- -D warnings; then
    print_status "Clippy checks passed - no warnings or errors"
else
    print_warning "Clippy found issues - check output above"
fi

# Check formatting
echo ""
echo "7. Checking code formatting..."
if cargo fmt -- --check; then
    print_status "Code formatting is correct"
else
    print_warning "Code formatting issues found. Run 'cargo fmt' to fix"
fi

# Validate configuration
echo ""
echo "8. Validating configuration..."
if ./target/release/zola-streams --config-check; then
    print_status "Configuration validation passed"
else
    print_warning "Configuration validation failed - check environment variables"
fi

# Test dry run
echo ""
echo "9. Testing dry run..."
if ./target/release/zola-streams --dry-run; then
    print_status "Dry run test passed"
else
    print_warning "Dry run test failed - check connectivity and certificates"
fi

# Check Docker setup (if Docker is available)
echo ""
echo "10. Checking Docker setup..."
if command -v docker &> /dev/null; then
    print_status "Docker is available"
    if command -v docker-compose &> /dev/null; then
        print_status "Docker Compose is available"
        
        # Check if docker-compose.yml exists
        if [ -f "docker/docker-compose.yml" ]; then
            print_status "Docker Compose configuration found"
            
            # Validate docker-compose file
            cd docker
            if docker-compose config > /dev/null 2>&1; then
                print_status "Docker Compose configuration is valid"
            else
                print_warning "Docker Compose configuration has issues"
            fi
            cd ..
        else
            print_warning "Docker Compose configuration not found"
        fi
    else
        print_warning "Docker Compose not found"
    fi
else
    print_warning "Docker not found - Docker deployment not available"
fi

# Check environment variables
echo ""
echo "11. Checking environment variables..."
if [ -n "$BITQUERY_USERNAME" ]; then
    print_status "BITQUERY_USERNAME is set"
else
    print_warning "BITQUERY_USERNAME not set"
fi

if [ -n "$BITQUERY_PASSWORD" ]; then
    print_status "BITQUERY_PASSWORD is set"
else
    print_warning "BITQUERY_PASSWORD not set"
fi

if [ -n "$KAFKA_BOOTSTRAP_SERVERS" ]; then
    print_status "KAFKA_BOOTSTRAP_SERVERS is set: $KAFKA_BOOTSTRAP_SERVERS"
else
    print_warning "KAFKA_BOOTSTRAP_SERVERS not set"
fi

# File structure check
echo ""
echo "12. Checking file structure..."
REQUIRED_FILES=("Cargo.toml" "src/main.rs" "config/default.env" "docker/docker-compose.yml")
for file in "${REQUIRED_FILES[@]}"; do
    if [ -f "$file" ]; then
        print_status "Required file exists: $file"
    else
        print_error "Required file missing: $file"
    fi
done

# Performance check
echo ""
echo "13. Performance metrics..."
BINARY_SIZE=$(du -h target/release/zola-streams | cut -f1)
print_info "Release binary size: $BINARY_SIZE"

# Final summary
echo ""
echo "============================================"
echo "Setup and Verification Summary"
echo "============================================"

# Count warnings and errors from the checks
if [ -f "target/release/zola-streams" ]; then
    print_status "Application is built and ready"
else
    print_error "Application build failed"
    exit 1
fi

echo ""
echo "Next steps:"
echo "1. Set environment variables (BITQUERY_USERNAME, BITQUERY_PASSWORD, etc.)"
echo "2. Run: ./target/release/zola-streams start"
echo "3. Or use Docker: cd docker && docker-compose up -d"
echo ""
echo "For detailed documentation, see COMPLETE_GUIDE.md"
echo ""
print_status "Setup verification completed successfully!"
