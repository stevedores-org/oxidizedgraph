# Build stage
FROM rust:1.83-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies for git2/libgit2
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    cmake \
    libgit2-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml ./

# Create dummy src for dependency caching
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/bin/server.rs && \
    echo "pub fn dummy() {}" > src/lib.rs

# Generate lock file and build dependencies only
RUN cargo generate-lockfile && \
    cargo build --release --bin oxidizedgraph-server && \
    rm -rf src

# Copy actual source
COPY src ./src

# Touch main files to invalidate cache and rebuild
RUN touch src/lib.rs src/bin/server.rs

# Build the actual binary
RUN cargo build --release --bin oxidizedgraph-server

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libgit2-1.5 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/oxidizedgraph-server /app/oxidizedgraph-server

# Create non-root user
RUN useradd -r -u 1000 oxidizedgraph
USER oxidizedgraph

# Expose default port
EXPOSE 8080

# Run the server
ENV PORT=8080
CMD ["/app/oxidizedgraph-server"]
