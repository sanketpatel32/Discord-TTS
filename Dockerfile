# Build stage
FROM rust:1.85-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake \
    pkg-config \
    libopus-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/vc-reader

# Cache dependencies
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs
RUN cargo build --release --locked || true
RUN rm -rf src target/release/vc* target/release/deps/vc*

# Copy full source and build release binary
COPY . .
RUN cargo build --release --locked

# Runtime stage: slim Debian container with minimal memory footprint
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libopus0 \
    libssl3 \
    gosu \
    && rm -rf /var/lib/apt/lists/*

# Run as non-privileged service user
RUN useradd -m -u 1000 -U -s /bin/sh vcreader

WORKDIR /app
COPY --from=builder /usr/src/vc-reader/target/release/vc-reader /app/vc-reader
COPY entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh && chown -R vcreader:vcreader /app

VOLUME ["/app/data"]

ENV RUST_LOG=info
ENTRYPOINT ["/app/entrypoint.sh"]
