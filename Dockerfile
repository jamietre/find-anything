# find-server Docker image
#
# Build: docker build -t find-anything-server .
# Run:   docker run -v ./data:/data -v ./server.toml:/etc/find-anything/server.toml:ro \
#               -p 8080:8080 find-anything-server

# ── Stage 1: build ────────────────────────────────────────────────────────────
FROM rust:1-slim AS builder

# Install build deps for native libs (openssl, etc.)
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .

# Build only the server crate (avoids compiling client/extractor deps)
RUN cargo build --release -p find-server

# ── Stage 2: runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/find-server /usr/local/bin/find-server

# Data directory — mount a volume here for persistent storage
VOLUME /data

EXPOSE 8080

ENTRYPOINT ["find-server", "--config", "/etc/find-anything/server.toml"]
