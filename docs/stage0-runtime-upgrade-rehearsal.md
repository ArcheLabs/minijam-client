# MiniJAM Stage-0 Runtime Upgrade Rehearsal

This runbook defines the rehearsal required before any Stage-0 public testnet
runtime upgrade. It is intentionally written as an operator checklist: every
command that produces release evidence should be copied into
`docs/STAGE0-RELEASE-CHECKLIST.md` with the commit SHA, artifact hash, operator,
and timestamp.

## Scope

The rehearsal proves that operators can:

- Build a reproducible runtime Wasm artifact from the release commit.
- Produce and verify a pre-upgrade snapshot.
- Submit the upgrade through the configured governance path.
- Observe block production, finalization, MiniJAM STF execution, workers, and
  fuel accounting after the upgrade.
- Roll back infrastructure from backups if the rehearsal chain becomes
  unusable.

Stage-0 may use Sudo for runtime upgrades. The rehearsal must still happen on a
non-public rehearsal network before the same artifact is proposed for the public
testnet.

## Preflight

Record the following before starting:

- Release commit: `git rev-parse HEAD`
- Jambda submodule commit: `git -C external/jambda rev-parse HEAD`
- Toolchain: `rustc --version && cargo --version`
- Chain spec hash: `sha256sum chain-specs/stage0-raw.json`
- Runtime Wasm hash after build.

Build and verify the candidate:

```bash
./scripts/check-submodule.sh
cargo fmt --all -- --check
cargo test -p pallet-minijam -p minijam-runtime
cargo check -p minijam-runtime --no-default-features --target wasm32v1-none
cargo build --release -p minijam-node -p minijam-worker
sha256sum target/release/minijam-node target/release/minijam-worker
find target -path '*minijam_runtime*.wasm' -type f -print -exec sha256sum {} \;
```

## Rehearsal Network

Run a private rehearsal network with the same topology as Stage-0:

- 3 authority nodes.
- 1 public RPC node.
- 3 workers with separate recovery databases.
- Prometheus with `deploy/stage0/alerts.yml`.

Use the committed Stage-0 chain spec unless the rehearsal explicitly tests a
chain spec migration. Do not reuse public testnet databases or authority keys.

## Snapshot

Before submitting the upgrade, create backups using `deploy/stage0/README.md`.
Record the archive paths and hashes:

```bash
sha256sum /var/backups/minijam/stage0-*.tgz
```

For Docker Compose rehearsals, record both config and volume archives:

```bash
sha256sum stage0-compose-*.tgz stage0-volumes-*.tgz
```

## Submit Upgrade

Submit the exact Wasm artifact from the build step. With Stage-0 Sudo, submit a
`system.setCode` call wrapped by `sudo.sudo`. The operator may use Polkadot.js or
another signer, but the signed extrinsic hash must be recorded.

Evidence to capture:

- Runtime Wasm SHA-256.
- Signed extrinsic hash.
- Block number containing the upgrade.
- New runtime `spec_version`, `transaction_version`, and metadata hash.

## Post-Upgrade Checks

Run these checks after finalization passes the upgrade block:

```bash
curl -s http://127.0.0.1:9944 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"state_getRuntimeVersion","params":[]}'
cargo run -p minijam-cli -- --rpc http://127.0.0.1:9944 get-last-execution-receipt
cargo run -p minijam-cli -- --rpc http://127.0.0.1:9944 get-system-service-info
cargo run -p minijam-cli -- --rpc http://127.0.0.1:9944 get-pending-work-tasks
cargo run -p minijam-cli -- worker-status --metrics http://127.0.0.1:9616/metrics
```

Prometheus checks:

- All node and worker targets are `up == 1`.
- `MiniJamTargetDown`, `MiniJamFinalizationLag`, and
  `MiniJamWorkerNotPolling` remain inactive.
- Worker poll counters continue increasing.
- Runtime block height and finalized height continue advancing.

Functional smoke checks:

- Claim faucet from a test account.
- Submit a valid preimage and verify it leaves `PendingPreimages`.
- Fund service 0 and verify `get-service-fuel 0` changes.
- Submit a normal WorkPackage and verify it reaches a terminal state.
- Submit or replay a bad candidate path and verify it does not panic the runtime.

## Rollback Exercise

If the rehearsal upgrade fails, restore from the backup created above and verify:

```bash
sudo systemctl status minijam-authority minijam-public-rpc minijam-worker
cargo run -p minijam-cli -- --rpc http://127.0.0.1:9944 get-last-execution-receipt
```

For Docker Compose, restore the config archive and volumes together, then run:

```bash
docker compose -f deploy/stage0/docker-compose.yml ps
```

Do not treat a rollback as proof that the upgrade is safe. A failed rehearsal
blocks the public testnet upgrade until the root cause is fixed and the full
rehearsal passes.

## Pass Criteria

The rehearsal passes only when all of the following are true:

- The build, tests, runtime Wasm check, and release binary build pass from a
  clean checkout.
- The upgrade extrinsic is included and finalized on the rehearsal network.
- Blocks and finalization continue for at least 100 blocks after the upgrade.
- MiniJAM empty-block STF receipts continue changing after the upgrade.
- Worker metrics continue polling after the upgrade.
- Faucet, preimage, service fuel, and one work submission smoke path pass.
- No critical alert remains active for more than one evaluation interval.
- Backup restore commands have been exercised at least once in the rehearsal
  environment during the release cycle.
