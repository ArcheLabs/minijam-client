# MiniJAM application RPC boundary

Applications are protocol clients, not Playground plugins. Computer, JNS and
DOOM are ordinary Services and must not be implemented in the MiniJAM client.

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

Stage 0 still uses the Playground API as an ingress relayer because canonical
JAM Work package construction, bundle publication and operation tracking are
not yet available as a browser library. This is an adapter, not the application
protocol. A client may replace it when it can:

1. build the canonical Work package and auditable bundle;
2. publish the bundle at the committed `ContentRef`;
3. encode the runtime call using current metadata and nonce;
4. ask the user's wallet to sign the extrinsic;
5. submit it through `author_submitExtrinsic` and follow finalized Work/Receipt state.

## Authenticated application principal

The Stage 0 ingress relayer verifies the user's signed action over the complete
Work parameters, including the Service payload. A Service may therefore bind
the account included in that payload while the relayer is the only authorized
ingress.

This trust does **not** automatically survive direct node ingress: the signed
extrinsic currently identifies the ingress account, and `WorkPackage` does not
carry a chain-validated end-user principal. Before enabling untrusted or direct
ingress, introduce a versioned authorization envelope in the protocol, validate
it in the runtime, and expose the validated principal to Service execution.
Never treat an unchecked account string in a payload as authenticated.
