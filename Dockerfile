FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app
RUN apt-get update && apt-get install -y libsqlite3-dev && rm -rf /var/lib/apt/lists/*

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin deko

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y libsqlite3-0 ca-certificates && rm -rf /var/lib/apt/lists/* && \
    useradd -m -u 1001 deko && \
    mkdir -p /app/data && \
    chown -R deko:deko /app
WORKDIR /app
COPY --from=builder /app/target/release/deko /usr/local/bin/deko
COPY --from=builder /app/migrations /app/migrations
USER deko
EXPOSE 8000
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8000/health/live || exit 1
ENTRYPOINT ["deko"]
