# MiniJAM Stage-1 architecture

Stage-1 is the protocol generation. It has two published network environments:

- `local`: deterministic development network, selected by `--dev` or
  `--chain local`;
- `testnet`: deterministic public test network, selected by `--chain testnet`.

`mainnet` is reserved and intentionally fails with an unpublished-network
error. Stage-1 is not itself a chain name, and no `stage1-e2e` or
`stage1-work-e2e` chain exists.

Both environments use the same `stage1_genesis` builder, Stage-1 runtime,
Service 0 state, economic constants, work rules, and one-worker topology.
Only identities and network metadata differ. Local uses fixed public
development identities; testnet uses the fixed identities of the deployed
testnet. Chain-spec generation does not read environment variables.

The Stage-1 genesis endows an ordinary AccountId32 for the independently
operated faucet. The external service uses normal Balances transfers and owns
rate limits, CAPTCHA, persistence, and its signing secret. No MiniJAM runtime
call, storage item, event, error, or genesis field has faucet semantics.

SS58 prefix remains 42. Validator, worker, Work-ingress, deployment, and
external-faucet keys are separate responsibilities and are not mounted into a
single public process.

## Canonical local E2E

The aggregate `minijam` image is the canonical local entry point. Running
`minijam --dev` supervises one local node, Formal RPC on port 8080, and Worker
0 on health port 8082. Consumer tests use only the public Node/Formal APIs;
they do not start a second network, read provider state, or select a test-only
genesis.
