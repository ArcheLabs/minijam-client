FROM rust:1.88-bookworm AS rust-builder
WORKDIR /src
ENV RUSTUP_MAX_RETRIES=10 \
    CARGO_TARGET_DIR=/cargo-target
RUN apt-get -o Acquire::Retries=5 update \
    && apt-get -o Acquire::Retries=5 install -y --no-install-recommends clang libclang-dev protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/local/rustup \
    --mount=type=cache,target=/cargo-target \
    cargo build --locked --release \
    -p minijam-node \
    -p minijam-compiler-api \
    -p minijam-playground-api \
    -p minijam-worker \
    && install -D /cargo-target/release/minijam-node /out/minijam-node \
    && install -D /cargo-target/release/minijam-compiler-api /out/minijam-compiler-api \
    && install -D /cargo-target/release/minijam-playground-api /out/minijam-playground-api \
    && install -D /cargo-target/release/minijam-worker /out/minijam-worker

FROM debian:bookworm-slim AS rust-runtime
RUN apt-get -o Acquire::Retries=5 update \
    && apt-get -o Acquire::Retries=5 install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --create-home --shell /usr/sbin/nologin minijam

FROM rust-runtime AS node
COPY --from=rust-builder /out/minijam-node /usr/local/bin/
USER minijam
ENTRYPOINT ["minijam-node"]

FROM rust-runtime AS playground
COPY --from=rust-builder /out/minijam-playground-api /usr/local/bin/
RUN install -d -o minijam -g minijam /data/bundles
USER minijam
ENTRYPOINT ["minijam-playground-api"]

FROM rust-runtime AS worker
COPY --from=rust-builder /out/minijam-worker /usr/local/bin/
RUN install -d -o minijam -g minijam /data
USER minijam
ENTRYPOINT ["minijam-worker"]

FROM rust-runtime AS compiler-binary
COPY --from=rust-builder /out/minijam-compiler-api /usr/local/bin/

FROM rust:1.88-bookworm AS converter
WORKDIR /build
ENV CARGO_TARGET_DIR=/cargo-target
COPY service-toolchain/compiler/polkavm-to-jam ./polkavm-to-jam
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/cargo-target \
    cargo build --locked --release --manifest-path ./polkavm-to-jam/Cargo.toml

FROM debian:bookworm-slim AS compiler
ARG LLVM_VERSION=20
RUN apt-get -o Acquire::Retries=5 update \
    && apt-get -o Acquire::Retries=5 install -y --no-install-recommends ca-certificates curl gnupg lsb-release software-properties-common \
    && curl --http1.1 --retry 5 --retry-all-errors -fsSL https://apt.llvm.org/llvm.sh -o /tmp/llvm.sh \
    && chmod +x /tmp/llvm.sh \
    && /tmp/llvm.sh "${LLVM_VERSION}" \
    && apt-get purge -y curl gnupg lsb-release software-properties-common \
    && apt-get autoremove -y \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/llvm.sh \
    && useradd --uid 10001 --create-home --shell /usr/sbin/nologin minijam
COPY --from=compiler-binary /usr/local/bin/minijam-compiler-api /usr/local/bin/
COPY --from=converter /cargo-target/release/minijam-polkavm-to-jam /usr/local/bin/polkavm-to-jam
COPY scripts /repository/scripts
COPY service-toolchain /repository/service-toolchain
USER minijam
ENV MINIJAM_COMPILER_BIND=0.0.0.0:8081 \
    MINIJAM_REPOSITORY=/repository \
    MINIJAM_CONVERTER_BIN=/usr/local/bin/polkavm-to-jam
ENTRYPOINT ["minijam-compiler-api"]
