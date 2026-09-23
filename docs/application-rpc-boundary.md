# MiniJAM application RPC boundary

Applications are protocol clients, not client-specific server plugins.
Computer, JNS and DOOM are ordinary Services and must not be implemented in
the MiniJAM client.

## Stable boundary

Trusted application infrastructure may use the node JSON-RPC methods directly:

- `minijam_getFinalizedContext`
- `minijam_getWork` and `minijam_getWorkIdByPackageHash`
- `minijam_getExecutionReceipt`
- `minijam_getServiceInfoAt` and `minijam_getServiceStorageAt`
- `system_accountNextIndex`
- `author_submitExtrinsic` for an already encoded and wallet-signed transaction

These methods are application-neutral. New Services must not require a custom
node RPC method. Access to the endpoint is still a deployment security
boundary: MiniJAM testnet Node RPC must be reachable by the JamScript Backend
and other trusted backend infrastructure, and must never be exposed directly
to the public Internet. Browser applications should use their application-
facing backend or same-origin reverse proxy rather than a public Node RPC URL.

## Private RPC topology

On one host, bind Node RPC and Formal RPC to loopback (`127.0.0.1:9944` and
`127.0.0.1:8080`). A host-local backend can use those endpoints. When the
backend runs on another host, route it over a private network, VPN, or private
overlay and restrict access with firewall rules; do not bind either RPC port
to `0.0.0.0` on a public interface. The Stage-1 compact and split Compose
profiles follow these rules.

For a JamScript Backend running as a separate container on the same host,
attach it to the Stage-1 private chain network. The compact Compose profile
names that network `minijam-testnet-chain`; the split profile uses that same
external name by default. The backend can then use the private service names
`node:9944` and `formal-rpc:8080`. Operators with another private Docker or
overlay network can set `MINIJAM_CHAIN_NETWORK` to its name in both Compose
deployments. Do not make either RPC reachable from the public Internet to
connect the backend.

The application-facing path for an Internet user is:

```text
Browser -> HTTPS reverse proxy -> JamScript Backend -> private Node RPC / Formal RPC
```

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
