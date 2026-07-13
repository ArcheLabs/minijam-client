# MiniJAM Client

English | [Simplified Chinese](README.zh-CN.md)

> MiniJAM is still in early development and is not production-ready.
>

MiniJAM is a parachain running a simplified version of the JAM protocol.

## Differences from JAM

MiniJAM narrows the JAM execution model from the Gray Paper into the smallest protocol surface that can be validated on an independent chain.

- Consensus and chain: MiniJAM uses a Polkadot SDK parachain to host protocol state instead of JAM shared consensus.
- Worker and client boundaries: the Worker side is designed with a multi-client target, so different Worker clients can independently implement submission, validation, and voting logic. The Runtime side is currently a single Runtime and does not support multiple interchangeable Runtime clients, which would cause chain inconsistency.
- Work and Guarantees: MiniJAM compresses the guarantee pipeline into Work, `ReportEnvelopeV1`, candidate report bonds, and Worker voting.
- Assurance and availability: MiniJAM does not implement JAM assurance semantics. It currently abstracts the data availability boundary through `BulletinEvidence`, the Bulletin-compatible simulator, and Worker voting.
- Disputes and verdicts: MiniJAM does not implement global disputes, judgments, or other work-report correctness and consistency logic. These are delegated to Support/Oppose voting within each Work round, absence slashing, candidate report rejection slashing, and equivocation proofs.
- State and state transition: MiniJAM keeps JAM's original state and state transition logic, but some state always remains at default values. This makes multi-client off-chain Worker implementations easier because they do not need to implement a separate logic set.
- Accumulate: MiniJAM keeps the accumulation logic and runs it as Runtime logic, so it cannot run in parallel at execution time.
- Bridge: unlike JAM, MiniJAM depends on a parachain and therefore has special token bridging requirements. This is implemented by monitoring specific storage.

## Current Progress

The current repository already includes:

- Versioned MiniJAM protocol types, report envelopes, Worker votes, and state change formats;
- Deterministic Worker selection, task assignment, multi-round candidate reports, and voting logic;
- The MiniJAM JamCore execution interface, plus execution result normalization and atomic state validation;
- Bulletin storage abstractions and a local simulator with injectable faults;
- Bridging required for inbound escrow and outbound release;
- A MiniJAM Executive implemented on top of jambda.

## Workflow

A Work item roughly moves from submission to execution as follows:

1. A user submits Work, and the Runtime locks the Work deposit.
2. The Worker module deterministically assigns validators from the active Worker set using the current Epoch and a delayed random seed.
3. Before the deadline, the report submitter submits a versioned `ReportEnvelopeV1` and locks the candidate report bond.
4. Assigned Workers vote for or against the candidate report. Once a threshold is reached, the result is locked.
5. An accepted candidate report enters the bounded execution queue. If it is rejected or times out, it advances to the next round until the maximum round count is reached.
6. At the end of the block, the Runtime executes due reports, validates and normalizes state changes, then writes protocol state atomically.
7. Execution receipts, service outputs, and bridge effects are recorded for downstream components.

The Runtime also provides Root administration operations for pausing execution and quarantining the pending execution queue, so protocol state can be protected if execution fails.

## Repository Layout

| Path | Responsibility |
| --- | --- |
| `crates/minijam-protocol` | Public protocol constants, content references, reports, votes, state changes, and bridge effect types |
| `crates/minijam-jamcore-api` | Versioned JamCore input/output, error types, state reads, and executor interface |
| `crates/minijam-jamcore-mock` | Configurable mock executor for tests |
| `crates/minijam-worker-engine` | Runtime-independent Worker ordering, assignment, voting, and slashing algorithms |
| `crates/minijam-bridge-engine` | Inbound/outbound bridge ledger and administrator state record encoding |
| `crates/minijam-bulletin-api` | Bulletin storage, authorization, renewal, and status query interfaces |
| `crates/minijam-bulletin-simulator` | Bulletin-compatible local file simulator and fault injection |
| `crates/minijam-state-adapter` | Execution output validation, state change normalization, and atomic application |
| `pallets/minijam-workers` | Worker registration, updates, unbonding, task assignment, voting, and misbehavior proofs |
| `pallets/minijam` | Work lifecycle, candidate reports, multi-round voting, execution queue, and protocol state |
| `pallets/minijam-bridge` | Inbound escrow, outbound release, and replay protection records |
| `runtime` | FRAME Runtime configuration and jambda Executive integration |
| `node` | MiniJAM node CLI, RPC, chain specs, and Aura/GRANDPA service |

## Current Protocol Parameters

The following values come from the current development configuration in `minijam-protocol` and may change before the protocol stabilizes:

| Parameter | Current value |
| --- | --- |
| Candidate Worker set | 8 |
| Max Work items per round | 4 |
| Workers assigned per Work | 3 |
| Support/Oppose threshold | 2 / 2 |
| Epoch length | 100 blocks |
| Assignment seed delay | 10 blocks |
| Report submission deadline | 20 blocks |
| Voting window | 10 blocks |
| Max candidate rounds | 3 |
| Max tasks per Worker per round | 2 |
| Minimum Worker stake | 1,000 UNIT |
| Work deposit | 10 UNIT |
| Candidate report bond | 10 UNIT |
| Max state delta | 4 MiB |

## Pinned Baselines

- Polkadot SDK: `polkadot-stable2603`, currently pinned to commit `2e4dd0bc22366a5af820492528869a493b5a5208`;
- Rust: `nightly-2026-05-02`;
- JAM Gray Paper semantics: `0.7.2`;
- Bulletin Chain compatibility baseline: `b6c2827d232669b525c0906cc20def0e5eb4676b`.

## Preparing the Development Environment

The repository's `rust-toolchain.toml` pins the Rust toolchain and installs `rustfmt`, `clippy`, `rust-src`, `wasm32-unknown-unknown`, and `wasm32v1-none`.

`runtime` currently depends on jambda through a relative path, so both repositories must be placed under the same parent directory:

```text
workspace/
├── jambda/
└── minijam-client/
```

After entering `minijam-client`, run:

```bash
cargo test --workspace
```

Check that the core crates that must run in Wasm remain `no_std` compatible:

```bash
cargo check \
  -p minijam-protocol \
  -p minijam-jamcore-api \
  -p minijam-worker-engine \
  --no-default-features \
  --target wasm32-unknown-unknown
```

Check the MiniJAM execution boundary from the sibling jambda repository:

```bash
cd ../jambda
cargo check \
  -p jambda-minijam-executive \
  --no-default-features \
  --target wasm32-unknown-unknown
```

## Build and Run a Local Node

Build the release binary:

```bash
cargo build --release -p minijam-node
```

Start a single-node development chain with a temporary database:

```bash
cargo run --release -p minijam-node -- --dev --tmp
```

Generate the development chain spec:

```bash
cargo run --release -p minijam-node -- export-chain-spec --chain dev
```

The development chain uses Alice as the Aura/GRANDPA authority and Sudo account. The local testnet configuration contains Alice and Bob as two authority nodes. These presets are only for local development.

## Development Checks

Before submitting changes, it is recommended to run:

```bash
cargo fmt --all -- --check
cargo test --workspace
```

Changes that touch the Runtime execution boundary should also run the two Wasm `no_std` checks above.

## Compatibility and Stability

- The public protocol is currently `PROTOCOL_VERSION_V1`, and the JamCore interface is currently `INTERFACE_VERSION = 1`.
- Reports, batches, state values, and execution queues have explicit bounds to keep Runtime execution bounded.
- `minijam-bulletin-simulator` only reproduces the Bulletin semantics needed for local development.
- Economic parameters, slashing ratios, and administrative privileges are still development configuration and should not be treated as final mainnet parameters.

## License

This project is licensed under the Apache License 2.0.
