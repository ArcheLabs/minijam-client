# MiniJAM Stage-0 Public Testnet Checklist

This checklist tracks implementation against
`minijam-stage0-public-testnet-implementation-spec.md`.

## Baseline

- minijam-client current implementation commit: `00593766892aee2985f3fcc1385390de5d09451e`
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
- `bash -n scripts/export-stage0-chain-specs.sh`

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
- Partial preimage quarantine coverage: `cargo test -p pallet-minijam preimage_execution_input_failure_is_quarantined_without_panic`
- `cargo check -p minijam-runtime`
- `cargo check -p minijam-rpc-runtime-api`
- `cargo check -p minijam-node`
- `cargo check -p minijam-cli`

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
- [x] Persistent file-backed recovery database stores worker task statuses and resumes ready bundles after restart.
- [x] Worker chain RPC reads pending task inputs through `minijam_getPendingWorkTasks`, which executes against the finalized block hash.
- [x] Real Auditable Work Bundle decoder validates version, SCALE encoding, trailing bytes, and package hash.
- [ ] Jambda Is-Authorized and Refine execution.
- [ ] Candidate submission and independent Support/Oppose voting.
- [x] Prometheus metrics endpoint exposes worker poll, task, bundle-ready, and bundle-rejected counters.

Evidence:

- `cargo test -p minijam-worker`
- `cargo check -p minijam-worker`
- `cargo check -p minijam-protocol -p minijam-rpc-runtime-api`
- `cargo test -p pallet-minijam pending_worker_tasks_project_finalized_worker_inputs`
- `cargo check -p minijam-runtime -p minijam-node -p minijam-worker`
- `cargo test -p minijam-worker-engine`
- `cargo check -p minijam-protocol -p minijam-worker-engine --no-default-features --target wasm32-unknown-unknown`
- `cargo run -p minijam-worker -- --config deploy/stage0/worker-1.toml --state-db /tmp/minijam-worker-state-check.toml --once`
- `cargo run -p minijam-worker -- --config deploy/stage0/worker-1.toml --state-db /tmp/minijam-worker-state-check.toml --metrics-bind 127.0.0.1:0 --once`
- `cargo run -p minijam-worker -- --rpc-url http://127.0.0.1:19944 --state-db /tmp/minijam-worker-main-once.toml --ipfs-gateway http://127.0.0.1:18080 --once` with a local JSON-RPC stub returning no pending work
- Open vote task discovery: `cargo test -p pallet-minijam-workers open_vote_tasks_project_assignment_and_submitted_votes`
- Open vote task worker decoding: `cargo test -p minijam-worker decodes_open_vote_tasks_rpc_response`
- `cargo check -p minijam-runtime -p minijam-node -p minijam-worker`
- Worker vote-task polling: `cargo test -p minijam-worker runner_records_open_vote_task_metrics`
- `cargo run -p minijam-worker -- --rpc-url http://127.0.0.1:19944 --state-db /tmp/minijam-worker-vote-task-once.toml --ipfs-gateway http://127.0.0.1:18080 --once` with a local JSON-RPC stub returning no pending work or open vote tasks

## M4: Real Service 0

- [x] Runtime embeds `artifacts/system-service.blob` instead of an inline byte string.
- [x] Manifest length is tested against the embedded blob.
- [x] `services/system-service/` source tree exists.
- [x] `scripts/build-system-service.sh` deterministically rebuilds the blob and manifest.
- [ ] Blob is a real executable PVM artifact, not a placeholder.
- [ ] CreateService end-to-end test creates a new service through service 0 accumulation.

Evidence:

- `bash -n scripts/build-system-service.sh`
- `./scripts/build-system-service.sh`
- `python3 -m json.tool artifacts/system-service.manifest.json`
- `cargo test -p minijam-runtime system_service_manifest_matches_embedded_blob`

## M5: Public Interfaces, CLI, Faucet

- [x] Pallet view functions expose core work, fuel, preimage, system op, receipt, and protocol-state queries.
- [x] Stable `minijam-rpc-runtime-api` and node `minijam_*` read-only RPC methods expose work, candidate, fuel, preimage, system op, receipt, and protocol-state queries.
- [x] `minijam-cli` command suite.
- [x] Rate-limited test MINI faucet.

Evidence:

- `cargo check -p minijam-rpc-runtime-api`
- `cargo check -p minijam-rpc-runtime-api --no-default-features`
- `cargo check -p minijam-runtime`
- `cargo check -p minijam-node`
- `cargo fmt --all -- --check`
- `cargo test -p pallet-minijam claim_faucet`
- `cargo check -p minijam-cli`
- `cargo test -p minijam-cli`
- `cargo run -p minijam-cli -- claim-faucet`
- `cargo run -p minijam-cli -- submit-vote --worker-id 3 --work-id 42 --round 1 --assignment-epoch 7 --candidate-report-hash 0x0909090909090909090909090909090909090909090909090909090909090909 --verdict support --deadline 100 --chain-id 0x2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a --signature 0x05050505050505050505050505050505050505050505050505050505050505050505050505050505050505050505050505050505050505050505050505050505`

## M6: Fixed Public Network Configuration

- [x] Runtime and node expose a `stage0` chain spec preset with fixed non-development authority, worker, sudo, faucet, reward, and escrow public accounts.
- [x] Release artifact CI exports `stage0.json` and `stage0-raw.json` through `scripts/export-stage0-chain-specs.sh`.
- [x] Committed or published `chain-specs/stage0.json`
- [x] Committed or published `chain-specs/stage0-raw.json`
- [x] Deployment topology, Docker, systemd, compose, monitoring stack.
- [x] Public RPC safety profile and authority RPC isolation.

Evidence:

- `cargo test -p minijam-runtime genesis_config_presets`
- `cargo check -p minijam-node`
- `bash -n scripts/export-stage0-chain-specs.sh`
- `cargo build -p minijam-node`
- `./scripts/export-stage0-chain-specs.sh ./target/debug/minijam-node chain-specs`
- `python3 -m json.tool chain-specs/stage0.json >/dev/null`
- `python3 -m json.tool chain-specs/stage0-raw.json >/dev/null`
- `cargo run -p minijam-worker -- --config deploy/stage0/worker-1.toml --once`
- `rg -n -- '--unsafe-rpc|rpc-methods|rpc-external|worker|AUTHORITY_1_PEER_ID|ipfs_gateway' deploy/stage0`

Current environment limits:

- Docker is not installed in this WSL distro, so `docker compose config` could not be executed here.
- `systemd-analyze verify` reached the template command paths, but `/usr/local/bin/minijam-node` and `/usr/local/bin/minijam-worker` are not installed on this machine.
- `promtool` is not installed, so Prometheus rule semantic checks could not be executed here.

## M7: Cross-Process E2E

- [ ] `tests/e2e-stage0/` process orchestration.
- [ ] Network startup, service creation, fuel, normal work, oppose, bad candidate, OOG, worker restart, node restart, IPFS interruption, duplicate request, and ledger conservation scenarios.
- [ ] Minimal E2E job in CI.

## M8: Canary and Release

- [ ] Node, worker, and economic metrics.
- [x] Alerting rules.
- [x] Backup and restore procedures.
- [ ] Runtime upgrade rehearsal.
- [ ] 48-hour Canary soak.
- [ ] Final `STAGE0-RELEASE-CHECKLIST.md` with evidence links.

Evidence:

- `ruby -e 'require "yaml"; YAML.load_file("deploy/stage0/alerts.yml"); YAML.load_file("deploy/stage0/prometheus.yml"); puts "yaml ok"'`
- Backup and restore procedures in `deploy/stage0/README.md`.
- Runtime upgrade rehearsal runbook: `docs/stage0-runtime-upgrade-rehearsal.md`.
- Release gate checklist skeleton: `docs/STAGE0-RELEASE-CHECKLIST.md`.

## Current Known Risks

- Service 0 is still a placeholder artifact and cannot satisfy the public testnet CreateService requirement.
- Worker binary can poll finalized pending task inputs, fetch verified bundles, and observe open vote tasks, but automatic candidate/vote generation, signing, and transaction submission are still pending.
- Cross-process E2E and Canary evidence are missing.
