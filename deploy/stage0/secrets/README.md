# Stage 0 runtime secrets

This directory contains documentation only. Do not commit authority, relayer,
Faucet, Sudo, or Worker credentials.

Before starting the release stack, provision these values out of band:

- an authority keystore directory containing the Aura and GRANDPA keys declared
  by the release chain spec;
- one seed URI file for each of Worker 1, Worker 2, and Worker 3;
- the Playground relayer URI in the untracked `deploy/stage0/.env` file.

Each seed file must contain only its URI followed by an optional newline. Limit
files to the operator account (`chmod 600`) and limit the keystore directory to
that account (`chmod 700`). The Compose stack mounts Worker files and the Node
keystore read-only.

The public Faucet and Sudo accounts are documented in
`deploy/stage0/ACCOUNTS.md`. Their private credentials are not part of a
MiniJAM release.
