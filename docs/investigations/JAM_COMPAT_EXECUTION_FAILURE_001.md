# JAM_COMPAT_EXECUTION_FAILURE_001

## Baseline

MiniJAM SHA: `063e9df7da6f4844f63ef7712831e3a3861656a3`

Jambda SHA: `d33e0abf8116b23bbc551c6a8d7075eacb2994ce`

Compiler/toolchain SHA:

- `service-toolchain/compiler/toolchain.lock` Git blob: `61f7983fb4549e38e31f4a183d23e305de5289e2`
- `service-toolchain/compiler/toolchain.lock` SHA-256: `57306cf6a1b5adfa58964d30afe1be910e991707de685b645d4c8dcb5dc55cc8`
- target: `riscv64-unknown-elf`, `rv64emac`, `lp64e`
- `clang_major = 20`
- `polkavm_linker = 0.30.0`
- `jam_program_blob_common = 0.1.28`
- converter image Rust base: `rust:1.88-bookworm`

Workflow: [CI run 34090410865](https://github.com/ArcheLabs/minijam-client/actions/runs/34090410865)

Job: [Trusted checks, job 101642544086](https://github.com/ArcheLabs/minijam-client/actions/runs/34090410865/job/101642544086)

Failed step: `Execute reproduced-compatible artifacts through Jambda`

Previous classification:

```text
ROOT_CAUSE_1=CI bootstrap/tool availability issue
ROOT_CAUSE_1_STATUS=FIXED
SECONDARY_FAILURE_DISCOVERED=true
FAILED_GATE="Execute reproduced-compatible artifacts through Jambda"
```

Artifact hashes at baseline:

| Artifact | Type | Size | SHA-256 |
|---|---:|---:|---|
| `examples/services/counter/artifacts/counter-c.blob` | JAM program blob | 1328 bytes | `230d50315c643faec0a62282ff2317ddb14ab1d87138cedd5ff95ece46517c45` |
| `examples/services/counter/artifacts/counter-cpp.blob` | JAM program blob | 1328 bytes | `230d50315c643faec0a62282ff2317ddb14ab1d87138cedd5ff95ece46517c45` |
| `examples/services/counter/artifacts/counter-c.polkavm` | PolkaVM program | 1886 bytes | `3fc00f04897d4561a19b9b007c5ca55a14f578af2624a9d95e581c88798c9be1` |
| `examples/services/counter/artifacts/counter-cpp.polkavm` | PolkaVM program | 1886 bytes | `3fc00f04897d4561a19b9b007c5ca55a14f578af2624a9d95e581c88798c9be1` |

The working tree was clean before the investigation. The repair branch is based on `origin/main` at the MiniJAM SHA above.

## Failure Reproduction

The workflow invokes:

```text
./scripts/test-counter-services.sh
```

Working directory:

```text
/home/libingjiang/minijam-client
```

Relevant environment:

```text
CARGO_TERM_COLOR=always
RUST_TEST_THREADS=1
RUST_BACKTRACE=1                 # enabled for reproduction
```

The script expands to:

```text
cargo test --locked \
  --manifest-path /home/libingjiang/minijam-client/external/jambda/crates/minijam-executive/Cargo.toml \
  --config patch.crates-io.minijam-protocol.path='/home/libingjiang/minijam-client/crates/minijam-protocol' \
  --config patch.crates-io.minijam-jamcore-api.path='/home/libingjiang/minijam-client/crates/minijam-jamcore-api' \
  counter_ -- --nocapture
```

The exact local reproduction exits before compilation, artifact loading, PVM predecode, or guest execution:

```text
error: cannot create the lock file /home/libingjiang/minijam-client/external/jambda/crates/minijam-executive/Cargo.lock because --locked was passed to prevent this
help: to generate the lock file without accessing the network, remove the --locked flag and use --offline instead.
EXIT_CODE=101
```

The pinned Jambda workspace has a root `external/jambda/Cargo.lock`, but `crates/minijam-executive` is explicitly excluded from that workspace and has no crate-local lockfile. Passing that excluded crate as `--manifest-path` makes Cargo require `external/jambda/crates/minijam-executive/Cargo.lock` when `--locked` is used.

## Failure Boundary and Root Cause

The earliest failure boundary is before stage 1 in the requested execution classification:

```text
0. Cargo dependency/lockfile setup
1. file loading                  NOT REACHED
2. artifact parse                NOT REACHED
3. program blob decode            NOT REACHED
4. PVM predecode                  NOT REACHED
5. VM instantiate                 NOT REACHED
6. entrypoint resolution          NOT REACHED
7. guest execution                NOT REACHED
8. HostCall                       NOT REACHED
```

The pinned Jambda root manifest explicitly excludes `crates/minijam-executive`, while the CI helper invokes that crate directly with `--manifest-path`. The excluded crate has no crate-local `Cargo.lock`; its dependencies are therefore not covered by `external/jambda/Cargo.lock`. `--locked` requires the local lockfile and Cargo exits 101 before compiling the test crate.

The causal chain is:

```text
CI reached the compatibility helper
→ helper selected the correct pinned Jambda crate
→ `--locked` required a lockfile at the excluded crate's manifest directory
→ that lockfile did not exist
→ Cargo exited 101 before Jambda loaded any artifact
```

Classification:

```text
ROOT_CAUSE_CATEGORY=A (stale CI execution harness)
ROOT_CAUSE_IDENTIFIED=PASS
```

This is not an artifact naming/path mismatch, artifact format mismatch, PVM incompatibility, HostCall ABI mismatch, MiniJAM runtime ABI mismatch, or Jambda execution regression.

## Artifact Provenance and Identity

The reproduction helper compiles the C and C++ fixtures in the isolated compiler image, converts each ELF to a JAM blob, and previously compared the temporary output with the checked-in golden blob before deleting the temporary directory. The execution helper then compiles the pinned Jambda tests, whose `include_bytes!` paths load those checked-in blobs.

At baseline, the C and C++ blobs are byte-identical. The direct local Jambda run after removing only the invalid lock assertion executed both artifacts successfully. The repair now retains the compiler outputs under `target/minijam-compat-artifacts/`, writes a manifest, prints SHA-256/size diagnostics, and requires each reproduced blob to compare byte-for-byte with the blob loaded by the Jambda tests before running Cargo.

```text
EXECUTED_ARTIFACT_IDENTITY=PASS (enforced by the repaired helper)
```

## Repair

Changed files:

- `scripts/test-counter-services.sh`
  - removed `--locked`, which is invalid for the excluded crate manifest;
  - prints MiniJAM, Jambda, compiler/toolchain, command, artifact path, digest, and size;
  - requires the compiler reproduction manifest and checks every reproduced blob against the exact checked-in blob consumed by Jambda;
  - reports `JAMBDA_EXECUTION=PASS` only after the selected pinned-Jambda tests complete successfully.
- `scripts/test-compiler-image.sh`
  - retains C/C++ reproduced blobs and PolkaVM outputs in `target/minijam-compat-artifacts/`;
  - records the artifact manifest and hashes while preserving the existing golden `cmp` assertion.
- `docs/investigations/JAM_COMPAT_EXECUTION_FAILURE_001.md`
  - records this baseline, reproduction, root cause, and validation evidence.

No Jambda revision, MiniJAM protocol, runtime, host ABI, or artifact bytes were changed.

## Validation

The original command with `--locked` reproduced the exit-101 lockfile failure exactly.

The equivalent command without `--locked`, using the same pinned Jambda revision and MiniJAM patches, compiled and executed:

```text
test::c_counter_executes_through_refine_and_accumulate ... ok
test::counter_rejects_invalid_payload_without_changing_state ... ok
test::cpp_counter_executes_through_refine_and_accumulate ... ok

test result: ok. 3 passed; 0 failed
```

The repaired helper was run against a reproduced-artifact fixture with the recorded baseline bytes:

```text
EXECUTED_ARTIFACT_IDENTITY=PASS
JAMBDA_EXECUTION=PASS
3 passed; 0 failed
```

Additional local checks:

```text
MiniJAM specification boundary check passed
cargo fmt --all -- --check                  PASS
bash -n scripts/test-compiler-image.sh      PASS
bash -n scripts/test-counter-services.sh    PASS
git diff --check                            PASS
missing reproduction manifest guard         PASS
```

The Docker daemon is unavailable in this WSL environment, so the actual isolated compiler-image reproduction could not be rerun locally. The referenced GitHub run recorded that preceding reproduction step as PASS; the repaired script preserves its `cmp` assertion and now carries its outputs forward for identity verification.

## Regression and Remaining Risk

The three pinned-Jambda counter tests are the regression coverage for the repaired execution gate. The remaining verification is a canonical GitHub Actions run using the actual Docker compiler reproduction and the revised helper. Until that run completes:

```text
REPRODUCED_ARTIFACT_JAMBDA_EXECUTION=AWAITING_CANONICAL_CI
EXISTING_JAMBDA_INTEGRATION=PASS (from investigated run; not changed by this repair)
NO_PROTOCOL_BYPASS=PASS
COMPAT_GATE_DIAGNOSTICS=PASS
FULL_CI=NOT_YET_RUN
```
