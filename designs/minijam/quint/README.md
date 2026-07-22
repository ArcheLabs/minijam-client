# MiniJAM Quint Specification

This directory contains an executable [Quint](https://quint-lang.org) model of
the MiniJAM MVP.

The model treats MiniJAM Runtime execution as virtual JAM block import. Runtime
does not interpret report deltas, service roots, service outputs, preimages or
bridge effects as MiniJAM-specific state. Instead, accepted canonical JAM
`WorkReport` projections and canonical preimages are passed to an abstract
Executive, which runs the retained standard JAM STF stages and returns an
opaque JAM state commitment plus import receipts.

## Scope

The model covers:

- worker registration, active snapshots, deterministic assignment, voting,
  rewards and slashing;
- work submission, candidate report submission, vote thresholds, deadline
  settlement and execution queueing;
- malformed vote rejection for wrong chain ID, protocol version, candidate
  hash, unassigned worker and expired deadline;
- canonical WorkReport projection imported into a virtual JAM block;
- retained STF order: last history, Safrole with empty tickets, preimage
  prepare, Accumulate, current history, authorization, statistics and preimage
  apply;
- removed STF boundaries: disputes, assurances and reports/guarantees never
  run in MiniJAM;
- Runtime import semantics: a report can be marked `Imported` without being
  accumulated in the same virtual block;
- native bridge ledger bookkeeping as a Runtime-only subsystem that observes
  standard JAM state keys instead of receiving Executive bridge effects;
- Bulletin-compatible authorization, content retention and fault boundaries.

## Files

| File | Purpose |
|---|---|
| [`types.qnt`](./types.qnt) | Opaque base types, bounded constants, work statuses and Executive phase enums. |
| [`messages.qnt`](./messages.qnt) | Work packages, canonical WorkReport/preimage projections, worker votes and execution I/O. |
| [`jam_projection.qnt`](./jam_projection.qnt) | Abstract projection of standard JAM state touched by retained STF stages. |
| [`executive.qnt`](./executive.qnt) | Abstract virtual JAM block executor and preimage queue admission. |
| [`state.qnt`](./state.qnt) | Runtime state plus opaque `JamProjection`, bridge and Bulletin state. |
| [`state_vars.qnt`](./state_vars.qnt) | Shared variables and ghost transition records. |
| [`workers.qnt`](./workers.qnt) | Worker assignment, vote accounting, rewards and slashing. |
| [`work.qnt`](./work.qnt) | Work/candidate lifecycle and execution queue admission. |
| [`refine.qnt`](./refine.qnt) | Canonical WorkReport projection construction. |
| [`bridge.qnt`](./bridge.qnt) | Runtime bridge ledger abstraction. |
| [`bulletin.qnt`](./bulletin.qnt) | Bulletin-compatible storage/fault abstraction. |
| [`invariants.qnt`](./invariants.qnt) | Invariant catalogue. |
| [`main.qnt`](./main.qnt) | Top-level transition system. |
| [`tests.qnt`](./tests.qnt) | Deterministic scenario tests. |

## Key Invariants

- `virtual_block_atomicity_invariant`
- `retained_stf_order_invariant`
- `tickets_always_empty_invariant`
- `reports_bypass_assurance_invariant`
- `reports_bypass_guarantee_invariant`
- `history_uses_imported_reports_invariant`
- `authorization_uses_imported_reports_invariant`
- `preimage_snapshot_invariant`
- `runtime_import_not_accumulation_invariant`
- `removed_stfs_never_run_invariant`

## Running

```sh
cd designs/minijam/quint
npx --yes @informalsystems/quint@0.32.0 typecheck main.qnt
npx --yes @informalsystems/quint@0.32.0 typecheck tests.qnt
npx --yes @informalsystems/quint@0.32.0 test --backend=typescript tests.qnt
npx --yes @informalsystems/quint@0.32.0 run --backend=typescript main.qnt --invariant=invariants --max-samples=100 --max-steps=30 --verbosity=0
```

The deterministic test suite currently contains 12 scenarios.
