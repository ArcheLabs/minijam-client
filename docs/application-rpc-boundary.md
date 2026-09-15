# MiniJAM application RPC boundary

Applications are protocol clients, not client-specific server plugins.
Computer, JNS and DOOM are ordinary Services and must not be implemented in
the MiniJAM client.

## Stable boundary

Browser and native clients may use the node JSON-RPC directly:

- `minijam_getFinalizedContext`
- `minijam_getPackageStatus`
- `minijam_getPackageFailure` and `minijam_getExecutionReceiptByPackageHash`
- `minijam_getServiceInfoAt` and `minijam_getServiceStorageAt`
- `system_accountNextIndex`
- `author_submitExtrinsic` for an already encoded and wallet-signed transaction

These methods are application-neutral. New Services must not require a custom
node RPC method.

## Transaction ingress

Formal RPC is the application-neutral transaction queue and bundle gateway. It
is an adapter, not the application protocol. A client may replace it when it can:

1. batch transactions by `(serviceId, serviceCodeHash)` into a canonical package;
2. publish the bundle at the committed `ContentRef`;
3. encode the runtime call using current metadata and nonce;
4. ask the user's wallet to sign the extrinsic;
5. submit the Worker-signed canonical report through `author_submitExtrinsic` and
   follow finalized package status/receipt state.

## Authenticated application principal

The Formal RPC validates ServiceInfo and code hash at a finalized context before
queuing the transaction. Deployment uses an ordinary signed system operation;
the Worker is the only account authorized to submit canonical reports.

Transaction payloads are service-defined bytes. An account string inside a
payload is not authenticated by MiniJAM; services must define and validate any
application-level principal themselves.
