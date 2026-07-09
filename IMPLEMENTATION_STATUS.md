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
- Runtime-independent protocol state adapter with namespace, ordering, operation,
  gas, receipt, and total-delta validation.
- Transaction-style in-memory protocol state application with rollback tests.
- `pallet-minijam-workers` registration with a dedicated balance hold reason,
  bounded candidate pool, next-epoch activation, and deterministic Top-N epoch
  snapshots.
- Next-epoch worker key/stake updates, immediate holds for stake increases, and
  two-epoch delayed release for stake decreases.
- Delayed-block-hash, domain-separated worker assignment with strict K, M, and
  per-worker duty bounds; insufficient pools never lower K.
- sr25519 session-key vote verification, explicit Support/Oppose thresholds,
  locked decisions with continued duty responses, and deadline finalization
  with bounded absentee recording.
- Equal timely-response rewards for Support/Oppose, absence slashing with a
  proportional/minimum rule, and slash resolution into the funded reward pool.
- Assignment-key-frozen equivocation proofs with one-shot 20% slashing and
  two-epoch worker suspension.
- `pallet-minijam` work deposits, candidate bonds, bounded pending queue,
  assignment/voting lifecycle, candidate rejection slashing, three-round
  retry/failure, accepted submitter reward, and execution queue handoff.
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
- FRAME storage binding for the validated protocol state adapter.
- Runtime execution hook and JAM state storage.
- Balances hold integration for bridge escrow.
- Full DAG-PB chunked Bulletin uploads and persistent simulator metadata.
- MiniJam accumulate-core execution and protocol delta export.
- Runtime benchmarks, weights, pause/quarantine recovery, and node restart
  integration tests.
