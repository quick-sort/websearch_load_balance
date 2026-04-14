# Build stage
FROM rust:1.85-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev pkgconfig

# Create app directory
WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build release binary
RUN cargo build --release --locked --no-default-features

# Runtime stage
FROM alpine:3.21

# Install runtime dependencies
RUN apk add --no-cache openssl ca-certificates

# Create non-root user
RUN adduser -D -u 1000 appuser

# Create app directory
WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/one_search /usr/local/bin/

# Copy default config template
COPY config.yaml /app/config.yaml

# Set ownership
RUN chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Default config path
ENV CONFIG_PATH=/app/config.yaml

# Use stdin/stdout for MCP communication
ENTRYPOINT ["one_search"]