# Canonical MiniJAM Stage-1 deployment

Stage-1 is distributed as Docker images. The production release unit is the
exact image digest, not a host-built executable or a GitHub Actions binary
artifact.

The canonical GHCR repositories are:

- `ghcr.io/archelabs/minijam-node`
- `ghcr.io/archelabs/minijam-worker`
- `ghcr.io/archelabs/minijam-formal-rpc`

Use the immutable release tag for evaluation and prefer
`repository@sha256:digest` references for production deployments. The compact
and split Compose profiles consume these three image references directly.

Stage-1 is the only supported MiniJAM deployment generation. Its core is the
node, worker, and application-neutral Formal RPC.

The compact profile runs all three roles on one host while retaining separate
containers, networks, data, and signing material. The split profile uses the
same boundary across private hosts. Formal RPC owns only its Work-ingress
relayer key and bundle store. The worker owns only its worker key. Validator,
deployment-controller, and external-faucet keys are separate.

In the compact profile, node RPC is published on the host loopback only at
`127.0.0.1:9944`. The node also joins a non-internal edge bridge for that
host-local boundary; the service-to-service `chain` network remains internal.
The split profile does not publish node RPC to the host: `9944` belongs on
private infrastructure protected by a firewall, VPN, or private overlay, and
must not be exposed directly to the public Internet.

Stage-1 service-to-service RPC uses Docker/private DNS names such as
`node:9944`, so both node profiles require `--rpc-cors=all`. This permits the
private hostname boundary while `--rpc-methods=safe` remains mandatory; CORS
configuration does not enable unsafe RPC methods.

Formal RPC performs bounded startup retries while waiting for the node RPC to
become available. A node container may therefore be started before its RPC
listener is ready without requiring an external sleep-based startup sequence.

Compose reads `MINIJAM_NODE_NETWORK_KEY`, `MINIJAM_WORKER_SEED`, and
`MINIJAM_FORMAL_RPC_RELAYER_URI` from the operator environment and mounts them
as `0400` `/run/secrets/*` files owned by UID/GID `10001`. The node uses
`--base-path=/data` and the explicit node key file so its libp2p identity and
chain database survive restarts. Do not replace these secret mounts with
world-readable files or run the images as root.

Generate fresh chain specifications with
`scripts/export-stage1-chain-specs-image.sh` from the exact node image used by
the deployment. Generated specs belong to the release artifact and are not
committed to the repository.
Public account IDs are deployment inputs; private keys never belong here.
SS58 prefix remains 42. Faucet funding is an ordinary endowed account in
genesis and the external faucet signs normal Balances transfers.
