# MiniJAM Quint Specification

This directory contains an executable [Quint](https://quint-lang.org) model of
the MiniJAM MVP execution network.

MiniJAM is a simplified JAM-compatible execution network. This model focuses on
the protocol-level safety properties around workers, work packages, candidate
reports, voting, accumulate execution, service state, preimages, exports,
Bulletin-compatible data references, and native bridge effects.

## What this specification is

This specification models the MiniJAM MVP as a bounded, executable state
machine. It is intended to make the core protocol transitions explicit and to
check safety invariants over randomized traces and deterministic scenarios.

The model currently covers:

- worker registration, active worker snapshots, assignments, voting, rewards
  and slashing;
- work package submission, candidate report submission, voting thresholds,
  deadline settlement and execution queueing;
- malformed vote rejection for wrong chain ID, protocol version, candidate
  hash, unassigned worker and expired deadline;
- abstract refine output tied to a work package, service code hash, parent
  service root and post service root;
- abstract accumulate execution with ordered delta validation, namespace
  allowlisting and atomic rollback;
- stale report rejection, replay rejection and service-root continuity;
- service storage, service lookup, preimage and export effects;
- native bridge escrow, admin bridge records and outbound nonce protection;
- Bulletin-compatible authorization, content retention and fault boundaries.

## Modeling abstractions

The model deliberately abstracts implementation details that are outside the
state-machine properties checked here:

- cryptography, signatures, hash functions, CIDs and canonical report byte
  encodings are represented by opaque symbolic values;
- PVM execution, canonical codec behavior and storage-root construction are
  represented by constrained protocol transitions;
- Worker client networking and real report corpus execution are represented by
  protocol-level report and vote actions.

Opaque values such as hashes, signatures, report bytes and CIDs are represented
by small symbolic values. The model checks equality, ordering, state transition
rules and safety properties, not byte-level encoding correctness.

## File layout

| File | Purpose |
|---|---|
| [`types.qnt`](./types.qnt) | Foundational opaque types, bounded model constants, namespaces, statuses and error enums. |
| [`messages.qnt`](./messages.qnt) | Work packages, report envelopes, worker votes, state delta, execution output and bridge effects. |
| [`state.qnt`](./state.qnt) | `MiniJamState`: worker, work, candidate, execution, service, preimage, export, bridge and Bulletin state. |
| [`state_vars.qnt`](./state_vars.qnt) | Shared Quint variables used by `main.qnt` and `invariants.qnt`. |
| [`workers.qnt`](./workers.qnt) | Worker registration abstraction, active snapshots, deterministic assignments, vote accounting, rewards and slashing. |
| [`work.qnt`](./work.qnt) | Work submission, candidate submission, voting, equivocation handling and deadline settlement. |
| [`refine.qnt`](./refine.qnt) | Abstract refine report construction from a work package and service context. |
| [`accumulate.qnt`](./accumulate.qnt) | Abstract MiniJAM executive, delta validation, root checks, replay checks and atomic commit/rollback. |
| [`service.qnt`](./service.qnt) | Service storage and service lookup namespace updates. |
| [`preimage.qnt`](./preimage.qnt) | Preimage namespace updates. |
| [`export.qnt`](./export.qnt) | Service output and export recording. |
| [`bridge.qnt`](./bridge.qnt) | Native bridge escrow, admin bridge records and exactly-once outbound nonces. |
| [`bulletin.qnt`](./bulletin.qnt) | Bulletin-compatible authorization, content storage and fault abstraction. |
| [`invariants.qnt`](./invariants.qnt) | Nullary invariant catalogue and composite `invariants`. |
| [`main.qnt`](./main.qnt) | Top-level `init`, `step` and nondeterministic trace generation. |
| [`tests.qnt`](./tests.qnt) | Deterministic scenario tests. |

## Key invariants

The invariant catalogue lives in [`invariants.qnt`](./invariants.qnt). Important
properties include:

- `assignment_invariant`: assignments contain the expected number of known
  active workers.
- `vote_threshold_invariant`: accepted and rejected works are backed by the
  required support or oppose thresholds.
- `single_vote_or_equivocation_invariant`: conflicting worker votes are tracked
  as equivocations.
- `execution_requires_acceptance_invariant`: only accepted work can be queued
  for execution.
- `paused_or_quarantined_not_executed_invariant`: paused or quarantined
  execution items are not executed.
- `delta_allowlist_invariant`: successful execution commits only allowed
  protocol namespaces.
- `delta_order_unique_invariant`: successful execution commits sorted,
  duplicate-free deltas.
- `execution_atomicity_invariant`: fatal execution errors do not partially
  commit state, bridge records or report application state.
- `service_root_continuity_invariant`: applied reports advance service roots
  according to their parent and post roots.
- `accepted_report_parent_continuity_invariant`: successful execution records
  the report and updates the corresponding service root.
- `malformed_vote_rejected_invariant`: malformed votes leave state unchanged.
- `bridge_nonce_invariant`: bridge inbound nonces are monotonic and outbound
  nonces are consumed at most once.
- `bridge_escrow_invariant`: bridge escrow balances never become negative.
- `reward_slash_conservation_invariant`: the abstract reward pool and worker
  stake balances never become negative.
- `preimage_consistency_invariant`: service lookup references require a known
  preimage or an explicit Bulletin fault boundary.
- `export_determinism_invariant`: each applied report records the export root
  declared by its metadata.

## Deterministic scenarios

[`tests.qnt`](./tests.qnt) contains deterministic checks for representative
protocol flows:

- happy-path report execution;
- candidate rejection by oppose threshold;
- insufficient workers;
- equivocation slashing and suspension;
- invalid delta rollback;
- bridge inbound/admin-record atomicity;
- stale report rejection;
- report replay rejection;
- malformed vote rejection;
- deadline settlement and absence slashing;
- service lookup plus preimage update;
- Bulletin fault isolation.

## Running the specification

From the repository root:

```sh
cd designs/minijam/quint
```

If `quint` is installed globally:

```sh
quint typecheck main.qnt
quint typecheck tests.qnt
quint test --backend=typescript tests.qnt
quint run --backend=typescript main.qnt --invariant=invariants --max-samples=50 --max-steps=20
```

Without a global install:

```sh
npx @informalsystems/quint typecheck main.qnt
npx @informalsystems/quint typecheck tests.qnt
npx @informalsystems/quint test --backend=typescript tests.qnt
npx @informalsystems/quint run --backend=typescript main.qnt --invariant=invariants --max-samples=50 --max-steps=20
```

`quint verify main.qnt` may also be used when a compatible symbolic backend is
available. The TypeScript backend is used in the commands above because it is
portable across common development environments.

## Current validation status

At the time this version was prepared, the following checks passed with
`@informalsystems/quint` 0.32.0:

```sh
npx --yes @informalsystems/quint@0.32.0 typecheck main.qnt
npx --yes @informalsystems/quint@0.32.0 typecheck tests.qnt
npx --yes @informalsystems/quint@0.32.0 test --backend=typescript tests.qnt
npx --yes @informalsystems/quint@0.32.0 run --backend=typescript main.qnt --invariant=invariants --max-samples=50 --max-steps=20 --verbosity=0
```

The deterministic test suite contains 12 scenarios.

## Maintenance notes

- Keep this specification aligned with the MiniJAM protocol types and pallet
  behavior as they evolve.
- Prefer strengthening invariants over adding scenario-only tests when the
  expected behavior is safety-critical.
- Keep public documentation free of local filesystem paths and non-public
  implementation details.
- If a model change intentionally abstracts away implementation behavior, state
  the abstraction explicitly in this README.
