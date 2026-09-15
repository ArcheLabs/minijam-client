# MiniJAM Stage-1 architecture

Stage-1 is the supported fresh-genesis network. Its execution boundary is
deliberately small:

- Formal RPC accepts service transactions and persists them in a durable queue;
- the queue batches by `(serviceId, serviceCodeHash)) into one canonical package;
- one Worker refines the active package against one finalized context;
- the Worker submits one canonical report directly to the node;
- the runtime imports the report, applies the Jambda state delta, and exposes
  package status and receipt by package hash.

The runtime has no ingress relayer, WorkRecord, assignment, candidate, voting,
worker-market, or service-fuel path. CreateService uses an ordinary signed
system operation. Allocation submission retains its separate
AllocationRelayer boundary. Locus is outside this repository and is unchanged.

The canonical network starts from fresh genesis. WorkerAccount,
AllocationRelayer, the Service 0 protocol state, and the bounded execution
queues are initialized directly in genesis; there is no migration from the
removed architecture.

## Local direct-refine E2E profile

`stage1-direct-e2e` is a LOCAL/CI-ONLY profile. It uses Alice consensus and exactly
one configured Worker account. The persistent native provider stores the node
database, Formal transaction queue, bundles, and one Worker recovery database
below `target/stage1-native-local`.

The consumer-facing gate submits two transactions for one service through
`minijam_submitTransactionV1`, verifies deterministic transaction IDs,
observes `queued -> packaged -> refining -> reported -> imported), and
asserts that both transactions share one package hash, item indexes 0/1, and
one package receipt. It only uses Node and Formal RPC; it does not inspect
provider PID files or private keys.
