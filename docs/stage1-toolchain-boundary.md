# Stage-1 toolchain boundary

Stage-1 core is a Rust/runtime validation surface. It consumes the committed
Service 0 artifact but does not compile Service 0 and does not need the LLVM
service compiler image.

```text
Stage-1 core CI
  ├─ Rust format/tests/no_std/Wasm/release checks
  ├─ committed Service 0 blob runtime tests
  └─ boundary assertion: no LLVM/compiler invocation

Service toolchain workflow
  ├─ Service 0 provenance: service.c → compiler image → committed blob
  └─ C/C++ conformance: counter C/C++ → compiler image → Jambda execution
```

## Service 0 ownership

The canonical source remains `services/system-service/src/service.c`. The
committed outputs are:

- `artifacts/system-service.blob`
- `artifacts/system-service.polkavm`
- `artifacts/system-service.manifest.json`

The manifest's `stage: 0` field records the artifact's Service 0 origin. The
`consumed_by_stages: [0, 1]` field records that both Stage 0 and Stage 1
genesis presets embed and execute the same committed artifact. Stage 1 does
not rebuild it at runtime.

The runtime embeds the blob with `include_bytes!` and tests it through the real
Jambda executor. Those tests cover manifest/blob identity, `CreateService`, and
allocation handling through Service 0's `refine` and `accumulate` entrypoints.

## Compiler boundary

The optional service compiler is defined in `deploy/compiler/Dockerfile` and
uses these immutable base identities:

- LLVM image: `silkeh/clang:20-bullseye@sha256:302c1c6d5cfd72ee154696a11e097bcc3a20e7060dac9add9a75dfbe8319947b`
- Converter image: `rust:1.88-bookworm@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0`

The conformance workflow emits compiler and artifact diagnostics, including
the LLVM executable versions, base image identities, compiler manifest hash,
converter hash, SDK hash, artifact hashes, and Jambda commit. The production
node, worker, playground, and runtime targets are checked to ensure that the
compiler toolchain is not copied into them; the compiler service target is the
intentional exception.

## Non-goals

This boundary does not migrate Service 0 to C++, remove the existing C ABI, or
close `JAM_COMPAT_EXECUTION_FAILURE_001`. The latter remains a separate
investigation and must be re-run by the conformance workflow before being
marked resolved.
