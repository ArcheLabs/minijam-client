# Implementation status

## Season 2 P0

- Experience Network deployment profiles support one active Worker and safe
  RPC methods; Compact and Split use the same Runtime.
- Work no longer depends on the deprecated Service Fuel ledger. JAM execution
  gas limits, usage reporting, and Jambda Service balance semantics remain.
- Work is permissionless at the Experience API; create/upgrade and runtime
  ingress-relayer boundaries remain separate.
- `AllocationV1` ingress has a dedicated relayer setting with ingress fallback,
  pending/processed replay protection, canonical Service 0 V2 queue handoff,
  receipts, and read views.
- The legacy outbound bridge pallet is not composed into the Season 2 runtime.
- Preimage submission remains extrinsic-supplied and follows the existing
  validation, pending queue, execution input, and Jambda consume/quarantine
  flow.

MiniJAM is an early development codebase. The public repository contains the
protocol surface, runtime pallets, node skeleton, local simulators, and the
gitlink for the private jambda execution submodule. It does not contain the
private jambda source code.

## Implemented

- Pinned Rust, Polkadot SDK, JAM Gray Paper, jambda submodule, and Bulletin
  compatibility baselines.
- `no_std` MiniJAM protocol types, JamCore ABI, bounded report envelopes,
  Worker vote payloads, protocol state changes, and bridge effect types.
- Deterministic Top-N Worker selection, balanced Work assignment, per-round duty
  limits, Support/Oppose voting, attendance accounting, rewards, slashing, and
  equivocation proofs.
- `pallet-minijam-workers` registration, session keys, stake holds, next-epoch
  activation, key/stake updates, delayed release, suspension, and epoch
  snapshots.
- `pallet-minijam` Work deposits, candidate bonds, pending queue, assignment and
  voting lifecycle, candidate rejection slashing, three-round retry/failure,
  accepted submitter reward, and bounded execution queue handoff.
- Runtime execution hook that runs accepted reports at block finalization,
  validates JamCore output, writes protocol state atomically, records execution
  receipts, and supports Root pause/quarantine controls.
- Runtime-independent protocol state adapter with namespace, ordering,
  operation, gas, receipt, total-delta, and rollback validation.
- FRAME storage binding for MiniJAM protocol state, service outputs, consumed
  reports, execution receipts, and bridge effects.
- Legacy native-asset bridge pallet and bridge-engine sources remain for Stage 0
  compatibility, but are not part of the Season 2 Runtime composition.
- Bulletin storage API plus a local Raw/Blake2b-256 CID simulator with
  fetch/store/renew/status operations and missing/corrupt/timeout fault
  injection.
- Mock JamCore executor and deterministic receipt generation for tests.
- Polkadot SDK stable2603 solo-chain runtime with System, Timestamp, Aura,
  GRANDPA, Balances, TransactionPayment, Sudo, MiniJAM workers, and MiniJAM
  lifecycle pallets.
- Development node CLI, RPC wiring, dev/local chain specs, Aura authoring, and
  GRANDPA finality service.
- Private jambda submodule integration for `jambda-minijam-executive`, including
  the runtime dependency path and `wasm32v1-none` no-default Runtime dependency
  validation.

## Deferred

- Production Bulletin Chain backend and production data-availability network.
- Real report corpus import, report replay, and full execution conformance
  tests.
- Off-chain Worker client, Refine execution, export segment generation, upload,
  and operational networking.
- Full JAM assurance, global dispute, judgment, verdict, audit, and chain
  rollback semantics.
- Production runtime weights, benchmarks, economic parameters, security audit,
  and mainnet stability commitments.
- Full Runtime and node builds. The current no-default `wasm32v1-none` Runtime
  dependency check passes, but host Runtime compilation and the Substrate
  release Wasm builder still hit `generic_const_exprs` trait-solver overflows in
  the private jambda `TinySpec` state backend and codec path.
- Public distribution flow for prebuilt Runtime artifacts for users without
  access to the private jambda submodule.

## Public release notes

- Public protocol, pallet, simulator, and node sources are present in this
  repository.
- Full Runtime and node builds currently require access to the private jambda
  submodule revision recorded in `external/jambda`.
- Public crate and pallet tests currently run with
  `cargo test --workspace --exclude minijam-runtime --exclude minijam-node`.
- `.agent` planning files are ignored and are not part of the public Git index.
