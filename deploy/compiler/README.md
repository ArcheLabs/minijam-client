# Service Compiler

The internal Compiler API exposes:

- `POST /internal/v1/compile`
- `GET /health/ready`
- `GET /metrics`

Set `MINIJAM_COMPILER_IMAGE` to the image built from `Dockerfile`. The image
uses the pinned `silkeh/clang:20-bullseye` manifest digest and a pinned Rust
converter base image; it does not download `llvm.sh` during the build.

Compilation always runs as uid 65532 with no network, a read-only root
filesystem, dropped capabilities, bounded CPU, memory, PIDs, temporary
storage, execution time, source size, diagnostics, and output size. Only
C/C++, O0/Os, the committed SDK, and the manifest-pinned converter are
selectable.

`scripts/print-service-toolchain-diagnostics.sh` records `clang`, `clang++`,
`ld.lld`, LLVM major, both base image identities, the compiler-manifest hash,
the converter hash, and a deterministic SDK tree hash.

`scripts/test-service0-provenance.sh` is the separate Service 0 gate: it
rebuilds `services/system-service/src/service.c` and compares the resulting
blob, debug artifact, and manifest identity with the committed files.

`scripts/test-compiler-image.sh` is the generic C/C++ conformance gate. It
rebuilds both Counter artifacts inside the image, compares them byte-for-byte
with the committed artifacts, and the separate conformance job executes those
artifacts through Jambda Refine and Accumulate.

The Stage-1 core workflow does not build this image or invoke
`scripts/compile-service`. The compiler toolchain is an optional provenance
and conformance dependency, not a runtime dependency of Service 0 or Stage 1.
