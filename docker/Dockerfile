# Multi-stage Dockerfile for Bitquery Solana Kafka Consumer
FROM rust:1.70-slim AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libsasl2-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy dependency manifests for all workspace members
COPY Cargo.toml Cargo.lock ./
COPY core/Cargo.toml ./core/
COPY tools/Cargo.toml ./tools/

# Create dummy source files for dependency caching
RUN mkdir -p src core/src tools/src && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > core/src/lib.rs && \
    echo "fn main() {}" > tools/src/lib.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release

# Remove dummy files
RUN rm -rf src core/src tools/src

# Copy all source code
COPY src ./src
COPY core/src ./core/src
COPY tools/src ./tools/src
COPY examples ./examples
COPY tests ./tests
COPY benches ./benches

# Copy build scripts
COPY core/build.rs ./core/

# Build application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libsasl2-2 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd --create-home --shell /bin/bash --uid 1000 app

# Set working directory
WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /app/target/release/zola-streams /usr/local/bin/

# Copy configuration files from new structure
COPY config/ ./config/

# Create directories for certificates and logs
RUN mkdir -p certs logs && \
    chown -R app:app /app

# Switch to app user
USER app

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Expose ports
EXPOSE 8080 9090

# Set default command
CMD ["zola-streams"]
