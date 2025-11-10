# Build stage - using Chainguard's Rust image with dev tools
FROM cgr.dev/chainguard/rust:latest-dev AS builder

USER root
RUN apk add --no-cache openssl-dev pkgconf
USER nonroot

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage - using Chainguard's wolfi-base
# Includes glibc, OpenSSL, and CA certificates
FROM cgr.dev/chainguard/wolfi-base:latest

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/headwind /app/headwind

# Chainguard images run as non-root by default (UID 65532)
# No shell, no package managers - minimal attack surface
# Includes CA certificates, glibc, and OpenSSL

# Explicitly set non-root user (Chainguard default is already 65532, but setting for security scanners)
USER 65532

# Expose ports
EXPOSE 8080 8081 8082 9090

ENTRYPOINT ["/app/headwind"]
