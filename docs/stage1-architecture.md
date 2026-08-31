# MiniJAM Stage-1 architecture

Stage-0 is the previous network generation and Playground is its frozen,
best-effort developer product. Season 2 is a historical Experience Network.
Stage-1 is the current generation.

Stage-1 exposes application-neutral node, Formal Work, state, and Service
lifecycle interfaces. JamScript, JAM Computer, and future MiniCells production
integrations depend on those interfaces, never on Playground. Formal RPC
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
