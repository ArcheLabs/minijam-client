# Implementation status

## Completed in the foundation milestone

- Pinned Rust, Gray Paper, jambda, and Bulletin compatibility baselines.
- `no_std` MiniJam protocol types and JAMCore ABI.
- Bounded canonical report envelope and worker vote payload.
- Deterministic top-N worker selection and balanced work assignment.
- Support/Oppose voting, attendance, and slash calculations.
- Bulletin-compatible Raw/Blake2b-256 CID store/fetch/renew simulator.
- Fault injection for missing, corrupt, and timeout responses.
- Native-asset bridge nonce and escrow core.
- Mock JAMCore executor and deterministic receipt generation.
- jambda logging split into std tracing and no_std no-op facades.
- jambda `minijam-executive` canonical report metadata projection.
- Native and Wasm `no_std` compilation of the MiniJam execution boundary.
- Removal of state-backend's direct work-output Merkle dependency; the
  standard JAM accumulate wrapper retains its existing root result.

## Intentionally deferred

- Existing report corpus import and execution tests.
- Worker client and refine.

## Remaining before the foundation MVP is complete

- Polkadot SDK solo-chain node/runtime scaffold.
- FRAME storage adapter.
- `pallet-minijam-workers`, `pallet-minijam`, and runtime execution hook.
- sr25519 vote verification in the runtime.
- Balances hold integration for stake, bonds, rewards, slashes, and bridge.
- Full DAG-PB chunked Bulletin uploads and persistent simulator metadata.
- MiniJam accumulate-core execution and protocol delta export.
- Runtime benchmarks, weights, pause/quarantine recovery, and node restart
  integration tests.
