# Build stage
FROM rust:1.90-slim AS builder

WORKDIR /usr/src/headwind

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 headwind

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /usr/src/headwind/target/release/headwind /app/headwind

# Change ownership
RUN chown -R headwind:headwind /app

USER headwind

# Expose ports
EXPOSE 8080 8081 9090

ENTRYPOINT ["/app/headwind"]
