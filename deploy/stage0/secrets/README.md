# Stage 0 runtime secrets

This directory contains documentation only. Do not commit authority, relayer,
Faucet, Sudo, or Worker credentials.

Public authority and Worker identities are listed in
`deploy/stage0/IDENTITIES.md`.

Before starting the release stack, provision these values out of band:

- a `tar.gz` authority keystore archive containing the Aura and GRANDPA key
  files declared by the release chain spec;
- one seed URI file for each of Worker 1, Worker 2, and Worker 3;
- the Playground relayer URI in the untracked `deploy/stage0/.env` file.

Each seed file must contain only its URI followed by an optional newline. Limit
files and the keystore archive to the container runtime identity:

```bash
sudo chown 10001:10001 \
  deploy/stage0/secrets/node-keystore.tar.gz \
  deploy/stage0/secrets/worker-1.seed \
  deploy/stage0/secrets/worker-2.seed \
  deploy/stage0/secrets/worker-3.seed
sudo chmod 0400 \
  deploy/stage0/secrets/node-keystore.tar.gz \
  deploy/stage0/secrets/worker-1.seed \
  deploy/stage0/secrets/worker-2.seed \
  deploy/stage0/secrets/worker-3.seed
```

Local Compose implements file secrets as bind mounts and may not apply the
declared secret `uid`, `gid`, and `mode`; setting ownership explicitly keeps
the long-running Node and Worker processes non-root. Compose mounts Worker
files and the Node archive read-only, then extracts the keystore inside the
private Node data volume.

The public Faucet and Sudo accounts are documented in
`deploy/stage0/ACCOUNTS.md`. Their private credentials are not part of a
MiniJAM release.

For the release workflow, repository administrators provision the same
credentials through these Actions secrets:

- `MINIJAM_RELEASE_NODE_KEYSTORE_B64`: a base64-encoded `tar.gz` whose root
  contains only the Node keystore files;
- `MINIJAM_RELEASE_RELAYER_URI`;
- `MINIJAM_RELEASE_WORKER_1_URI`;
- `MINIJAM_RELEASE_WORKER_2_URI`;
- `MINIJAM_RELEASE_WORKER_3_URI`;
- `MINIJAM_RELEASE_E2E_WALLET_SEED`: 32-byte hex used only by the browser
  smoke-test wallet.

The workflow never writes these values to an Artifact or log.
