FROM rust:1.88-bookworm@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 AS builder

WORKDIR /src
ENV RUSTUP_MAX_RETRIES=10 \
    CARGO_TARGET_DIR=/cargo-target

RUN apt-get -o Acquire::Retries=5 update \
    && apt-get -o Acquire::Retries=5 install -y --no-install-recommends \
      clang \
      libclang-dev \
      protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

COPY . .

ARG GIT_SHA=unknown
ARG RELEASE_TAG=unknown
ENV MINIJAM_BUILD_GIT_SHA=${GIT_SHA} \
    MINIJAM_BUILD_RELEASE_TAG=${RELEASE_TAG}

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/local/rustup \
    --mount=type=cache,target=/cargo-target \
    cargo build --locked --release \
      -p minijam-node \
      -p minijam-worker \
      -p minijam-formal-rpc \
    && install -D /cargo-target/release/minijam-node /out/minijam-node \
    && install -D /cargo-target/release/minijam-worker /out/minijam-worker \
    && install -D /cargo-target/release/minijam-formal-rpc /out/minijam-formal-rpc

FROM builder AS tests

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/local/rustup \
    --mount=type=cache,target=/cargo-target \
    cargo test --locked \
      -p minijam-node \
      -p minijam-worker \
      -p minijam-formal-rpc

FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818 AS runtime

ARG GIT_SHA=unknown
ARG RELEASE_TAG=unknown

RUN apt-get -o Acquire::Retries=5 update \
    && apt-get -o Acquire::Retries=5 install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --create-home --shell /usr/sbin/nologin minijam

LABEL org.opencontainers.image.revision="${GIT_SHA}" \
      org.opencontainers.image.version="${RELEASE_TAG}" \
      org.opencontainers.image.source="https://github.com/ArcheLabs/minijam-client"

FROM runtime AS node
COPY --from=builder /out/minijam-node /usr/local/bin/minijam-node
RUN install -d -o minijam -g minijam /data
USER minijam
ENTRYPOINT ["minijam-node"]

FROM runtime AS worker
COPY --from=builder /out/minijam-worker /usr/local/bin/minijam-worker
RUN install -d -o minijam -g minijam /data
USER minijam
ENTRYPOINT ["minijam-worker"]

FROM runtime AS formal-rpc
COPY --from=builder /out/minijam-formal-rpc /usr/local/bin/minijam-formal-rpc
RUN install -d -o minijam -g minijam /bundles
USER minijam
ENTRYPOINT ["minijam-formal-rpc"]
