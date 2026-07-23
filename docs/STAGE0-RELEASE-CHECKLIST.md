# MiniJAM Stage-0 Release Checklist

Current status: not releasable. This file is the operator-facing release gate for
the Stage-0 public testnet. Items stay unchecked until the linked evidence proves
completion on the release commit or release candidate network.

## Release Candidate

- Release commit: TBD
- Jambda submodule commit: TBD
- Runtime Wasm SHA-256: TBD
- Node binary SHA-256: TBD
- Worker binary SHA-256: TBD
- Chain spec SHA-256: TBD
- Release owner: TBD
- Rehearsal network: TBD
- Public testnet launch window: TBD

## Build Gates

- [ ] `./scripts/check-submodule.sh`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace`
- [ ] `cargo check -p minijam-runtime --no-default-features --target wasm32v1-none`
- [ ] `cargo check -p minijam-protocol -p minijam-jamcore-api -p minijam-worker-engine --no-default-features --target wasm32-unknown-unknown`
- [ ] `cargo build --release -p minijam-node -p minijam-worker -p minijam-cli`
- [ ] `./scripts/export-stage0-chain-specs.sh ./target/release/minijam-node chain-specs`

Evidence:

- CI run 1: TBD
- CI run 2: TBD
- CI run 3: TBD
- Artifact hashes: TBD

## Protocol Gates

- [ ] Runtime executes MiniJAM STF every block.
- [ ] Accepted WorkReports are bound to their original WorkPackage and candidate
      metadata.
- [ ] Preimages enter Jambda state as full canonical bytes.
- [ ] User-input failures are recoverable or quarantined and do not panic the
      runtime.
- [ ] Service fuel reserve, charge, refund, and escrow invariants hold.
- [ ] Service 0 is a real executable PVM artifact.
- [ ] CreateService succeeds through service 0 accumulation.
- [ ] Worker daemon executes Is-Authorized / Refine against Jambda.
- [ ] Worker submits candidates and independent Support/Oppose votes.

Evidence:

- `docs/stage0-public-testnet-checklist.md`
- Runtime and pallet test logs: TBD
- Worker E2E logs: TBD

## Network Gates

- [ ] `chain-specs/stage0.json` and `chain-specs/stage0-raw.json` match the
      release artifact hashes.
- [ ] At least 3 authority nodes run non-development keys.
- [ ] At least 3 workers run separate keys and recovery databases.
- [ ] Public RPC node exposes only safe RPC methods.
- [ ] Authority RPC remains private or loopback-only.
- [ ] Faucet account is funded and rate-limited.
- [ ] Monitoring stack scrapes nodes and workers.
- [ ] Alert rules are loaded by Prometheus.
- [ ] Backup and restore procedure has been executed for this release candidate.

Evidence:

- `deploy/stage0/README.md`
- `deploy/stage0/prometheus.yml`
- `deploy/stage0/alerts.yml`
- Chain spec export logs: TBD
- Prometheus target screenshot or API output: TBD

## Runtime Upgrade Rehearsal

- [ ] Rehearsal network launched from release candidate artifacts.
- [ ] Pre-upgrade backup archives recorded with hashes.
- [ ] Runtime upgrade extrinsic included and finalized.
- [ ] Blocks and finalization advanced for at least 100 blocks after upgrade.
- [ ] STF receipts continue after upgrade.
- [ ] Workers continue polling after upgrade.
- [ ] Faucet, preimage, fuel, work, and bad-candidate smoke paths pass.
- [ ] Rollback procedure exercised in rehearsal environment.

Evidence:

- `docs/stage0-runtime-upgrade-rehearsal.md`
- Rehearsal command transcript: TBD
- Upgrade extrinsic hash: TBD
- Runtime version before/after: TBD

## Cross-Process E2E

- [ ] `tests/e2e-stage0/` orchestration starts a local network.
- [ ] Service creation scenario passes.
- [ ] Service fuel and normal work scenario passes.
- [ ] Oppose and bad candidate scenario passes.
- [ ] Out-of-gas scenario passes.
- [ ] Worker restart recovery scenario passes.
- [ ] Node restart recovery scenario passes.
- [ ] IPFS or bundle gateway interruption scenario passes.
- [ ] Duplicate request scenario passes.
- [ ] Ledger conservation scenario passes.
- [ ] Minimal E2E job runs in CI.

Evidence:

- E2E CI run: TBD
- Scenario logs: TBD

## Canary

- [ ] Public canary network runs for 48 hours.
- [ ] No critical alert remains unresolved.
- [ ] No runtime panic or node crash is observed.
- [ ] Worker polling and task processing counters continue advancing.
- [ ] Finalization lag remains within configured alert thresholds.
- [ ] Service fuel accounting remains conservative.
- [ ] Backup snapshot is created during canary.
- [ ] Rollback decision criteria are reviewed by operators.

Evidence:

- Canary start: TBD
- Canary end: TBD
- Prometheus alert history: TBD
- Incident log: TBD

## Launch Decision

- [ ] All build, protocol, network, rehearsal, E2E, and canary gates are checked.
- [ ] Known risks are accepted by the release owner.
- [ ] User and worker operator documentation is published.
- [ ] Launch announcement includes chain spec, RPC URL, faucet instructions, and
      worker config template.

Decision: not approved.
