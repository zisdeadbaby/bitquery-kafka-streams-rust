# Multi-stage Dockerfile for Bitquery Solana Kafka Consumer
FROM rust:1.70-slim as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libsasl2-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd --create-home --shell /bin/bash app

# Set working directory
WORKDIR /app

# Copy dependency manifests
COPY Cargo.toml Cargo.lock ./
COPY shared/ ./shared/
COPY streams/ ./streams/

# Build dependencies (this layer will be cached)
RUN cargo build --release --manifest-path streams/kafka/bitquery-solana-kafka/Cargo.toml

# Copy source code
COPY streams/kafka/bitquery-solana-kafka/src ./streams/kafka/bitquery-solana-kafka/src
COPY streams/kafka/bitquery-solana-kafka/examples ./streams/kafka/bitquery-solana-kafka/examples
COPY streams/kafka/bitquery-solana-kafka/tests ./streams/kafka/bitquery-solana-kafka/tests
COPY streams/kafka/bitquery-solana-kafka/benches ./streams/kafka/bitquery-solana-kafka/benches

# Build application
RUN cargo build --release --manifest-path streams/kafka/bitquery-solana-kafka/Cargo.toml

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
COPY --from=builder /app/target/release/bitquery-solana-kafka /usr/local/bin/
COPY --from=builder /app/target/release/examples/ /usr/local/bin/examples/

# Copy configuration files
COPY deployment/configs/ ./config/
COPY .env.example ./.env

# Create directories for certificates and logs
RUN mkdir -p certs logs && \
    chown -R app:app /app

# Switch to app user
USER app

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Expose ports
EXPOSE 8080 9090

# Default command
CMD ["/usr/local/bin/examples/dex_monitor"]
