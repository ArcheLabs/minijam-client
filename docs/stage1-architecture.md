# MiniJAM Stage-1 architecture

Stage-1 is the supported network generation. Earlier network profiles remain
protocol compatibility history only and are not shipped as deployment
products.

Stage-1 exposes application-neutral node, Formal Work, state, and Service
lifecycle interfaces. JamScript, JAM Computer, and future MiniCells production
integrations depend on those interfaces. Formal RPC
constructs canonical Work and provides neutral Service creation from an
already-built blob; it does not accept source code or application actions.

The canonical network starts from fresh genesis, so removal of the obsolete
faucet claim map does not require a storage migration. The Stage-0 chain and
its historical files remain available; they are not upgraded in place.

The Stage-1 genesis endows an ordinary AccountId32 for the independently
operated faucet. The external service uses normal Balances transfers and owns
rate limits, CAPTCHA, persistence, and its signing secret. No MiniJAM runtime
call, storage item, event, error, or genesis field has faucet semantics.

SS58 prefix remains 42. Validator, worker, Work-ingress, deployment, and
external-faucet keys are separate responsibilities and are not mounted into a
single public process.

## Local Work E2E profile

`stage1-work-e2e` is a LOCAL/CI-ONLY profile with chain id
`minijam_stage1_work_e2e`; it is never a public or production network. It
keeps the production Stage-1 Service 0 protocol state, runtime constants,
worker stake, and Work protocol unchanged. Its only genesis differences are
Alice development consensus and three explicitly endowed development workers:
worker 0/Alice, worker 1/Bob, and worker 2/Charlie. Their session keys are the
corresponding deterministic sr25519 public keys.

The persistent native provider stores the node database, Formal bundles, and
three independent worker recovery databases below `target/stage1-native-local`.
The consumer-facing `connection.env` contains only node/Formal endpoints, the
chain id/spec path, and worker count. Work E2E clients must use those public
interfaces and must not read provider PID files, worker keys, or recovery
state. The Docker profile mirrors the same three worker identities and binds
only the node/Formal endpoints to localhost.
