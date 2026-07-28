# syntax=docker/dockerfile:1
FROM rust:1.88-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --locked --release -p minijam-worker

FROM debian:bookworm-slim
RUN groupadd --system --gid 10001 minijam \
    && useradd --system --uid 10001 --gid minijam --home-dir /var/lib/minijam minijam \
    && install -d -o minijam -g minijam /data
COPY --from=builder /src/target/release/minijam-worker /usr/local/bin/minijam-worker
COPY deploy/worker/worker.example.toml /etc/minijam/worker.toml
USER 10001:10001
VOLUME ["/data"]
EXPOSE 8082
ENTRYPOINT ["minijam-worker", "--config", "/etc/minijam/worker.toml"]
