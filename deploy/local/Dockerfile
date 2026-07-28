FROM rust:1.88-bookworm AS rust-builder
WORKDIR /src
COPY . .
RUN cargo build --locked --release \
    -p minijam-node \
    -p minijam-compiler-api \
    -p minijam-playground-api \
    -p minijam-worker

FROM debian:bookworm-slim AS rust-runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --create-home --shell /usr/sbin/nologin minijam

FROM rust-runtime AS node
COPY --from=rust-builder /src/target/release/minijam-node /usr/local/bin/
USER minijam
ENTRYPOINT ["minijam-node"]

FROM rust-runtime AS playground
COPY --from=rust-builder /src/target/release/minijam-playground-api /usr/local/bin/
RUN install -d -o minijam -g minijam /data/bundles
USER minijam
ENTRYPOINT ["minijam-playground-api"]

FROM rust-runtime AS worker
COPY --from=rust-builder /src/target/release/minijam-worker /usr/local/bin/
RUN install -d -o minijam -g minijam /data
USER minijam
ENTRYPOINT ["minijam-worker"]

FROM rust-runtime AS compiler-binary
COPY --from=rust-builder /src/target/release/minijam-compiler-api /usr/local/bin/

FROM rust:1.88-bookworm AS converter
WORKDIR /build
COPY service-toolchain/compiler/polkavm-to-jam ./polkavm-to-jam
RUN cargo build --locked --release --manifest-path ./polkavm-to-jam/Cargo.toml

FROM debian:bookworm-slim AS compiler
ARG LLVM_VERSION=20
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg lsb-release software-properties-common \
    && curl --http1.1 --retry 5 --retry-all-errors -fsSL https://apt.llvm.org/llvm.sh -o /tmp/llvm.sh \
    && chmod +x /tmp/llvm.sh \
    && /tmp/llvm.sh "${LLVM_VERSION}" \
    && apt-get purge -y curl gnupg lsb-release software-properties-common \
    && apt-get autoremove -y \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/llvm.sh \
    && useradd --uid 10001 --create-home --shell /usr/sbin/nologin minijam
COPY --from=compiler-binary /usr/local/bin/minijam-compiler-api /usr/local/bin/
COPY --from=converter /build/polkavm-to-jam/target/release/minijam-polkavm-to-jam /usr/local/bin/polkavm-to-jam
COPY scripts /repository/scripts
COPY service-toolchain /repository/service-toolchain
USER minijam
ENV MINIJAM_COMPILER_BIND=0.0.0.0:8081 \
    MINIJAM_COMPILER_DIRECT=1 \
    MINIJAM_REPOSITORY=/repository \
    MINIJAM_CONVERTER_BIN=/usr/local/bin/polkavm-to-jam
ENTRYPOINT ["minijam-compiler-api"]
