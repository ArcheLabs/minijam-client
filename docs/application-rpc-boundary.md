# MiniJAM application RPC boundary

Applications are protocol clients, not client-specific server plugins.
Computer, JNS and DOOM are ordinary Services and must not be implemented in
the MiniJAM client.

## Stable boundary

Browser and native clients may use the node JSON-RPC directly:

- `minijam_getFinalizedContext`
- `minijam_getWork` and `minijam_getWorkIdByPackageHash`
- `minijam_getExecutionReceipt`
- `minijam_getServiceInfoAt` and `minijam_getServiceStorageAt`
- `system_accountNextIndex`
- `author_submitExtrinsic` for an already encoded and wallet-signed transaction

These methods are application-neutral. New Services must not require a custom
node RPC method.

## Work ingress

Formal RPC is the application-neutral ingress and bundle gateway. It is an
adapter, not the application protocol. A client may replace it when it can:

1. build the canonical Work package and auditable bundle;
2. publish the bundle at the committed `ContentRef`;
3. encode the runtime call using current metadata and nonce;
4. ask the user's wallet to sign the extrinsic;
5. submit it through `author_submitExtrinsic` and follow finalized Work/Receipt state.

## Authenticated application principal

The Work-ingress relayer verifies the request before submitting the runtime
operation. A Service may bind the account included in its payload while the
relayer is the only authorized ingress.

This trust does **not** automatically survive direct node ingress: the signed
extrinsic currently identifies the ingress account, and `WorkPackage` does not
carry a chain-validated end-user principal. Before enabling untrusted or direct
ingress, introduce a versioned authorization envelope in the protocol, validate
it in the runtime, and expose the validated principal to Service execution.
Never treat an unchecked account string in a payload as authenticated.
