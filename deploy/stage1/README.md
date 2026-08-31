# Canonical MiniJAM Stage-1 deployment

Stage-1 is the current MiniJAM network generation. Its core is the node,
worker, and application-neutral Formal RPC. Playground is a frozen legacy
Stage-0 product and is not part of this deployment or its dependency graph.

The compact profile runs all three roles on one host while retaining separate
containers, networks, data, and signing material. The split profile uses the
same boundary across private hosts. Formal RPC owns only its Work-ingress
relayer key and bundle store. The worker owns only its worker key. Validator,
deployment-controller, and external-faucet keys are separate.

Generate fresh chain specifications with `scripts/export-stage1-chain-specs.sh`.
Public account IDs are deployment inputs; private keys never belong here.
SS58 prefix remains 42. Faucet funding is an ordinary endowed account in
genesis and the external faucet signs normal Balances transfers.

