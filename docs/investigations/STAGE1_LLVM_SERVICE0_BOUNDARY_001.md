# STAGE1_LLVM_SERVICE0_BOUNDARY_001

## Scope

This investigation separates the Stage-1 core gate from the Service 0
source-to-artifact gate and from generic C/C++ toolchain conformance. The
previous `JAM_COMPAT_EXECUTION_FAILURE_001` investigation remains a separate
record and is not closed by this document.

## Baseline

- Repository: `ArcheLabs/minijam-client`
- Baseline commit: `c8fc8268b43a2cd3c38f204d99a55f326916529f`
- Working branch: `codex/stage1-llvm-service0-boundary-fix`
- Service 0 source: `services/system-service/src/service.c` (C)
- Committed Service 0 blob: `artifacts/system-service.blob`
- Committed Service 0 blob SHA-256: `def863ba5364bd7eccac35a1aced33e09a5867218a8287564b4e11c8df5c440a`
- Service 0 manifest: `artifacts/system-service.manifest.json`
- Compiler manifest: `service-toolchain/compiler/toolchain.lock`
- LLVM major declared by the compiler manifest: `20`

## Findings before repair

1. `.github/workflows/ci.yml` installed LLVM/Clang and ran the compiler Docker
   reproduction and Jambda artifact execution checks in the same job as the
   core Rust/runtime checks.
2. The runtime embeds the committed Service 0 blob with
   `include_bytes!("../../artifacts/system-service.blob")`. Runtime tests
   already execute CreateService and allocation operations through the real
   Jambda executor using that embedded blob.
3. Stage 0 and Stage 1 genesis presets both consume the same committed Service
   0 blob. The manifest's `stage: 0` is provenance/origin information; it must
   not be interpreted as saying that Stage 1 does not consume the artifact.
4. `deploy/compiler/Dockerfile` downloaded `https://apt.llvm.org/llvm.sh`
   during every build without pinning the image or script contents.
5. The retired local Dockerfile used the same mutable LLVM installer in its
   compiler target. Its node, worker, and runtime targets copied binaries from
   builder stages and did not intentionally include the compiler toolchain;
   this boundary is now made explicit and checked.

## Repair plan

- Make the canonical core workflow free of LLVM/compiler-toolchain setup and
  add a boundary assertion for that fact.
- Add a separate Service 0 provenance job that rebuilds the canonical C source
  in the pinned compiler image and compares both committed artifacts and the
  manifest identity.
- Keep generic C/C++ conformance and Jambda execution in a separate workflow
  job, with toolchain/version/hash diagnostics.
- Pin the LLVM compiler image and remove the mutable `llvm.sh` download path.
- Record the distinction between Service 0 provenance and Stage 1 consumption
  in the manifest and runtime tests.

## Repair completed

The canonical `.github/workflows/ci.yml` now contains only the core Rust/runtime
path. It installs `ripgrep` as a shell utility, but does not install LLVM,
Clang, libclang, protobuf tooling, the compiler Dockerfile, or
`scripts/compile-service`. `scripts/check-stage1-core-boundary.sh` asserts the
same boundary on every core run.

`.github/workflows/service-toolchain.yml` owns the two optional gates:

- `service0-provenance` rebuilds the canonical C source and compares the blob,
  debug artifact, and manifest identity.
- `c-cpp-conformance` rebuilds the C and C++ Counter fixtures in the isolated
  compiler image, emits toolchain diagnostics, and executes the reproduced
  artifacts through the pinned Jambda revision.

The compiler Dockerfiles now use immutable Rust and LLVM base-image digests and
no longer download `llvm.sh`. The Service 0 manifest explicitly records that
its Stage 0-origin artifact is consumed by both Stage 0 and Stage 1.

## Validation

Local checks completed:

```text
STAGE1_CORE_BOUNDARY=PASS
STAGE1_CORE_GATE=PASS
PRODUCTION_IMAGE_TOOLCHAIN_BOUNDARY=PASS
cargo fmt --all -- --check                         PASS
cargo test -p minijam-runtime --lib system_       PASS (3 passed)
cargo test -p minijam-runtime --lib stage1_genesis_uses_committed_service0_protocol_state
                                                     PASS (1 passed)
git diff --check                                   PASS
```

The current WSL environment has no Docker daemon, so the two Docker-backed
toolchain gates could not be executed locally. The scripts and workflow are
ready for canonical CI execution and deliberately report that limitation
rather than treating an unexecuted provenance check as a passing artifact
rebuild.

## Acceptance status

```text
SERVICE0_SOURCE=C
SERVICE0_RUNTIME_REQUIRES_LLVM=false
SERVICE0_REBUILD_REQUIRES_LLVM=true
STAGE1_CORE_REQUIRES_LLVM=false
STAGE1_CORE_GATE=PASS
SERVICE0_PROVENANCE_GATE=NOT_RUN (Docker unavailable locally)
C_TOOLCHAIN_CONFORMANCE=NOT_RUN (Docker unavailable locally)
CPP_TOOLCHAIN_CONFORMANCE=NOT_RUN (Docker unavailable locally)
LLVM_ENVIRONMENT_PINNED=true
JAMBDA_COMPAT_FAILURE_001=UNRESOLVED (awaiting canonical CI rerun)
```

`JAM_COMPAT_EXECUTION_FAILURE_001` remains a separate investigation. Its
earlier lockfile/harness repair is retained, but this boundary repair does not
claim a fresh Docker-backed Jambda execution result.
