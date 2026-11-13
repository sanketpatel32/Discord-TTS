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

# Copy full source and build release binary
COPY . .
RUN cargo build --release --locked

# Runtime stage: slim Debian container with minimal memory footprint
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libopus0 \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Run as non-privileged service user
RUN useradd -m -u 1000 -U -s /bin/sh vcreader

WORKDIR /app
COPY --from=builder /usr/src/vc-reader/target/release/vc-reader /app/vc-reader
RUN chown -R vcreader:vcreader /app

USER vcreader
VOLUME ["/app/data"]

ENV RUST_LOG=info
ENTRYPOINT ["/app/vc-reader"]
