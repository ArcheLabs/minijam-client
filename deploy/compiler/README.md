# Stage 0 Compiler

The internal Compiler API exposes:

- `POST /internal/v1/compile`
- `GET /health/ready`
- `GET /metrics`

Set `MINIJAM_COMPILER_IMAGE` to the image built from `Dockerfile`. Compilation
always runs as uid 65532 with no network, a read-only root filesystem, dropped
capabilities, bounded CPU, memory, PIDs, temporary storage, execution time,
source size, diagnostics, and output size. Only C/C++, O0/Os, the committed
SDK, and the manifest-pinned converter are selectable.

`scripts/test-compiler-image.sh` builds the image, rebuilds both Counter
artifacts inside it, compares them byte-for-byte with the committed artifacts,
and CI then executes those artifacts through Jambda Refine and Accumulate.
