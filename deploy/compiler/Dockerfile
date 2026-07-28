FROM rust:1.88-bookworm AS converter
WORKDIR /build
COPY service-toolchain/compiler/polkavm-to-jam ./polkavm-to-jam
RUN cargo build --locked --release --manifest-path ./polkavm-to-jam/Cargo.toml

FROM debian:bookworm-slim
ARG LLVM_VERSION=20
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl gnupg lsb-release software-properties-common \
 && curl -fsSL https://apt.llvm.org/llvm.sh -o /tmp/llvm.sh \
 && chmod +x /tmp/llvm.sh \
 && /tmp/llvm.sh "${LLVM_VERSION}" \
 && apt-get purge -y curl gnupg lsb-release software-properties-common \
 && apt-get autoremove -y \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/* /tmp/llvm.sh
COPY --from=converter /build/polkavm-to-jam/target/release/minijam-polkavm-to-jam /usr/local/bin/polkavm-to-jam
RUN useradd --uid 65532 --no-create-home --shell /usr/sbin/nologin compiler
USER 65532:65532
WORKDIR /workspace
