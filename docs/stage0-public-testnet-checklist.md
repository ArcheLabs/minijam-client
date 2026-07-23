# MiniJAM Stage-0 Public Testnet Checklist

This checklist tracks implementation against
`minijam-stage0-public-testnet-implementation-spec.md`.

## Baseline

- minijam-client current implementation commit: `7168ea8add2a8477d1b6bcaa71c73a85588d98e5`
- external/jambda pinned commit: `fce620ab070bddf832c62a18a7d530408d01f7db`
- /home/libingjiang/jambda companion commit: `19d1bde466918a03e9229eb72d46a0eb68f47ecc`
- Rust toolchain: `nightly-2026-05-02`
- Polkadot SDK revision: `2e4dd0bc22366a5af820492528869a493b5a5208`

## M0: Baseline, Build, CI

- [x] `external/jambda` remains a fixed submodule revision.
- [x] `scripts/check-submodule.sh` validates initialization, pinned SHA, executive crate presence, and a clean submodule worktree.
- [x] `.github/workflows/ci.yml` covers fmt, workspace tests, no_std checks, runtime Wasm check, node release build, worker release build, raw chain spec export, and artifact upload.
- [ ] CI has passed three consecutive clean-runner executions.

Evidence:

- `./scripts/check-submodule.sh`
- `cargo fmt --all -- --check`
- `cargo check -p minijam-protocol -p minijam-jamcore-api -p minijam-worker-engine --no-default-features --target wasm32-unknown-unknown`
- `cargo check -p minijam-runtime --no-default-features --target wasm32v1-none`

## M1: Recoverable Runtime Execution

- [x] Empty blocks execute STF.
- [x] System-op-only service execution failure no longer panics.
- [x] Failed pending system ops move to structured `QuarantinedSystemOps` records with canonical hash, stable error code, block number, and retryability.
- [x] Root can retry, drop, or clear quarantined system ops.
- [ ] Report/preimage/delta validation failures have structured quarantine records with stable error codes.
- [ ] All user-input execution failure paths are audited to ensure only true state corruption can panic.

Evidence:

- `cargo test -p pallet-minijam`
- Tests: `system_op_execution_failure_is_quarantined_without_panic`, `root_manages_quarantined_system_ops`

## M2: Candidate and WorkPackage Binding

- [x] `MiniJamExecutor::project_report` exposes Runtime-safe report projection.
- [x] Jambda executive projects package hash, context hash, exports root, result count, per-service result data, and gas totals.
- [x] `submit_candidate` validates report projection before holding candidate bond or opening voting.
- [x] `accept_candidate` revalidates the candidate report before queueing execution.
- [x] Tests reject mismatched package hash, result count, service id, code hash, gas limit, and trailing bytes.
- [ ] Lookup-anchor semantics and authorizer state binding need full Jambda-backed validation.

Evidence:

- `cargo test -p minijam-jamcore-api -p pallet-minijam`
- `cargo check -p minijam-runtime`

## M3: Real Worker Daemon

- [x] Worker has config validation and documented TOML loading.
- [x] Worker runner verifies ContentRef size and hash and records per-task status.
- [ ] Worker chain RPC via finalized state.
- [ ] Persistent recovery database.
- [ ] Real Auditable Work Bundle decoder.
- [ ] Jambda Is-Authorized and Refine execution.
- [ ] Candidate submission and independent Support/Oppose voting.
- [ ] Prometheus metrics.

## M4: Real Service 0

- [x] Runtime embeds `artifacts/system-service.blob` instead of an inline byte string.
- [x] Manifest length is tested against the embedded blob.
- [ ] `services/system-service/` source tree exists.
- [ ] `scripts/build-system-service.sh` deterministically rebuilds the blob and manifest.
- [ ] Blob is a real executable PVM artifact, not a placeholder.
- [ ] CreateService end-to-end test creates a new service through service 0 accumulation.

## M5: Public Interfaces, CLI, Faucet

- [x] Pallet view functions expose core work, fuel, preimage, system op, receipt, and protocol-state queries.
- [ ] Stable Runtime API and custom read-only RPC methods.
- [ ] `minijam-cli` command suite.
- [ ] Rate-limited test MINI faucet.

## M6: Fixed Public Network Configuration

- [ ] `chain-specs/stage0.json`
- [ ] `chain-specs/stage0-raw.json`
- [ ] Non-development authority, worker, sudo, faucet, reward, and escrow public accounts.
- [ ] Deployment topology, Docker, systemd, compose, monitoring stack.
- [ ] Public RPC safety profile and authority RPC isolation.

## M7: Cross-Process E2E

- [ ] `tests/e2e-stage0/` process orchestration.
- [ ] Network startup, service creation, fuel, normal work, oppose, bad candidate, OOG, worker restart, node restart, IPFS interruption, duplicate request, and ledger conservation scenarios.
- [ ] Minimal E2E job in CI.

## M8: Canary and Release

- [ ] Node, worker, and economic metrics.
- [ ] Alerting rules.
- [ ] Backup and restore procedures.
- [ ] Runtime upgrade rehearsal.
- [ ] 48-hour Canary soak.
- [ ] Final `STAGE0-RELEASE-CHECKLIST.md` with evidence links.

## Current Known Risks

- Service 0 is still a placeholder artifact and cannot satisfy the public testnet CreateService requirement.
- Worker binary is not yet connected to chain RPC or transaction submission.
- No fixed non-development public chain spec exists yet.
- No faucet or custom public RPC exists yet.
- Cross-process E2E and Canary evidence are missing.
