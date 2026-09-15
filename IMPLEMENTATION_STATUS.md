# MiniJAM implementation status

## Direct Refine architecture

MiniJAM now has one configured Worker account and one direct report path:

- Formal RPC accepts `minijam_submitTransactionV1` requests and persists them
  in a durable JSON queue.
- Transactions are batched by `(service_id, service_code_hash)` into one
  `WorkPackage`; each transaction is exactly one `WorkItem`.
- The Worker polls `GET /worker/v1/task`, runs Jambda Refine against the fixed
  finalized context, and submits the canonical report directly through
  `MiniJam.submit_report`.
- Runtime derives the package hash from the canonical report projection,
  rejects every origin except the configured Worker account, deduplicates
  packages, and queues `Pending` reports.
- Finalization executes the retained Jambda/JAM STF path. Package status is
  `Pending`, `Imported`, or `Failed`, keyed by package hash, with receipt and
  failure views keyed by the same hash.
- Formal RPC maps package and receipt results back to transaction IDs,
  including package hash, item index, receipt, and terminal error.

The deprecated ingress-relayer, WorkRecord, assignment, candidate, voting,
worker-market, fuel, and chain-side Work lifecycle are not part of the runtime
or node RPC surface. Allocation ingress remains separately gated by the
configured AllocationRelayer. CreateService and preimage submission use normal
signed accounts.

## Verification

The direct-refine branch is verified with protocol, WorkPackage builder,
Formal RPC, Worker, CLI, runtime, node, and pallet compilation/tests. The
native single-node/direct-Worker E2E gate is the authoritative gate before
JamScript changes are made.

## Deferred

- Production Bulletin Chain/data-availability backend.
- Full JAM assurance, disputes, judgments, verdicts, audits, and rollback
  semantics outside the retained MiniJAM execution boundary.
- Production weights, benchmarks, security audit, and mainnet operations.
- Public distribution of runtime artifacts for users without the private
  Jambda submodule.
