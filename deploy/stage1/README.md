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

Generate fresh chain specifications with
`scripts/export-stage1-chain-specs-image.sh` from the exact node image used by
the deployment. Generated specs belong to the release artifact and are not
committed to the repository.
Public account IDs are deployment inputs; private keys never belong here.
SS58 prefix remains 42. Faucet funding is an ordinary endowed account in
genesis and the external faucet signs normal Balances transfers.
