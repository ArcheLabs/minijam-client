# MiniJAM direct-Refine Quint model

This directory contains the executable model for the single-Worker direct
Refine architecture.

The model deliberately keeps the boundary small: Formal RPC queues stable
transaction IDs, batches transactions with the same service and code hash
under one fixed context, and assigns one WorkItem to each transaction. The
configured Worker is the only origin accepted by direct report submission.
Package state is `Pending`, `Imported`, or `Failed`; transaction state exposes
the queued, packaged, refining, reported, and terminal lifecycle.

The previous Worker-market, WorkRecord, assignment, candidate, and voting
model has been removed together with the corresponding runtime path.

## Running

```sh
cd designs/minijam/quint
npx --yes @informalsystems/quint@0.32.0 typecheck direct_ingress.qnt
npx --yes @informalsystems/quint@0.32.0 test --backend=typescript direct_ingress.qnt
```
