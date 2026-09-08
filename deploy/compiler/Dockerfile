FROM rust:1.88-bullseye@sha256:4fe592966775f3396e3d099d674f0664e4383336371ca5600e40c2fe3b575383 AS converter
WORKDIR /build
COPY service-toolchain/compiler/polkavm-to-jam ./polkavm-to-jam
RUN cargo build --locked --release --manifest-path ./polkavm-to-jam/Cargo.toml

FROM silkeh/clang:20-bullseye@sha256:302c1c6d5cfd72ee154696a11e097bcc3a20e7060dac9add9a75dfbe8319947b
ENV MINIJAM_LLVM_BASE_IMAGE="silkeh/clang:20-bullseye@sha256:302c1c6d5cfd72ee154696a11e097bcc3a20e7060dac9add9a75dfbe8319947b" \
    MINIJAM_CONVERTER_BASE_IMAGE="rust:1.88-bullseye@sha256:4fe592966775f3396e3d099d674f0664e4383336371ca5600e40c2fe3b575383"
COPY --from=converter /build/polkavm-to-jam/target/release/minijam-polkavm-to-jam /usr/local/bin/polkavm-to-jam
USER 65532:65532
WORKDIR /workspace
