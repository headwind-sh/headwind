# Download pre-built binary stage
FROM cgr.dev/chainguard/wolfi-base:latest AS downloader

# Build args for version and architecture
ARG VERSION=v0.1.0
ARG TARGETARCH

# Install curl for downloading
USER root
RUN apk add --no-cache curl

# Download the appropriate binary based on architecture
RUN case "${TARGETARCH}" in \
      amd64) ARCH_SUFFIX="amd64" ;; \
      arm64) ARCH_SUFFIX="arm64" ;; \
      *) echo "Unsupported architecture: ${TARGETARCH}" && exit 1 ;; \
    esac && \
    curl -L "https://github.com/headwind-sh/headwind/releases/download/${VERSION}/headwind-linux-${ARCH_SUFFIX}" \
      -o /tmp/headwind && \
    chmod +x /tmp/headwind

# Runtime stage - using Chainguard's wolfi-base
# Includes glibc, OpenSSL, and CA certificates
FROM cgr.dev/chainguard/wolfi-base:latest

WORKDIR /app

# Copy the binary from downloader
COPY --from=downloader /tmp/headwind /app/headwind

# Chainguard images run as non-root by default (UID 65532)
# No shell, no package managers - minimal attack surface
# Includes CA certificates, glibc, and OpenSSL

# Explicitly set non-root user (Chainguard default is already 65532, but setting for security scanners)
USER 65532

# Expose ports
EXPOSE 8080 8081 8082 9090

ENTRYPOINT ["/app/headwind"]
