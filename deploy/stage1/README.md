# Canonical MiniJAM Stage-1 testnet deployment

Stage-1 is distributed as Docker images. The production release unit is the
exact image digest, not a host-built executable or a GitHub Actions binary
artifact.

The canonical GHCR repositories are:

- `ghcr.io/archelabs/minijam-node`
- `ghcr.io/archelabs/minijam-worker`
- `ghcr.io/archelabs/minijam-formal-rpc`
- `ghcr.io/archelabs/minijam` (the zero-configuration local launcher)

Use the immutable release tag for evaluation and prefer
`repository@sha256:digest` references for production deployments. The compact
and split Compose profiles consume the three component image references
directly. They always use the built-in `testnet` chain spec and do not mount a
generated chain-spec file.

Stage-1 is the protocol generation; `testnet` is its public network. Its core
is the node, worker, and application-neutral Formal RPC.

The compact profile runs all three roles on one host while retaining separate
containers, networks, data, and signing material. The split profile uses the
same boundary across private hosts. Formal RPC owns only its Work-ingress
relayer key and bundle store. The worker owns only its worker key. Validator,
deployment-controller, and external-faucet keys are separate.

Node RPC is an infrastructure dependency for the JamScript Backend and other
trusted backend services. It is not a public application endpoint. In the
compact profile, node RPC is published on the host loopback only at
`127.0.0.1:9944`; the node also joins a non-internal edge bridge for that
host-local boundary while the service-to-service `chain` network remains
internal. A backend process running directly on the same host can use
`http://127.0.0.1:9944`. The split profile does not publish node RPC to the
host: connect the backend over a private interface, Docker network, VPN, or
private overlay protected by firewall rules. In both layouts, port `9944` must
not be reachable from the public Internet. Never replace the loopback publish
with `9944:9944`. The compact profile gives its internal chain bridge the
stable name `minijam-testnet-chain`, which lets the separately managed
JamScript Backend container join that private network and use `node:9944` and
`formal-rpc:8080` without routing through host-published ports. The split
profile expects the same externally managed private network name by default;
set `MINIJAM_CHAIN_NETWORK` consistently if the operator uses another private
Docker/overlay network.

Formal RPC is another backend-only dependency and has the same exposure rule.
Both Compose profiles publish it on host loopback only at
`127.0.0.1:8080`. For a remote frontend, expose the JamScript Backend through
the application reverse proxy; do not publish Node or Formal RPC publicly.

Stage-1 service-to-service RPC uses Docker/private DNS names such as
`node:9944`, so both node profiles require `--rpc-cors=all`. This permits the
private hostname boundary while `--rpc-methods=safe` remains mandatory; CORS
configuration does not enable unsafe RPC methods.

Compose reads the node network key, worker seed, and Formal RPC relayer URI
from operator-controlled secret sources and mounts them as `/run/secrets/*`.
Do not replace these mounts with world-readable files or run the images as
root.

Generate the reproducible public export with
`scripts/export-testnet-chain-specs-image.sh` from the exact node image used by
the deployment. The node binary owns the canonical testnet identities; no
public-key environment variable is accepted by the chain-spec exporter.
SS58 prefix remains 42. Faucet funding is an ordinary endowed account in
genesis and the external faucet signs normal Balances transfers.

Stage-1 testnet registers and runs one Worker (worker 0). The `/ipfs/<CID>` endpoint
is a temporary local content-addressed bundle transport served directly by
Formal RPC; no IPFS daemon or IPFS network is required. Formal RPC stores the
bundle bytes locally before Work submission, and the Worker fetches them from
the Formal RPC origin through that route.
