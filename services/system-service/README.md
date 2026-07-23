# MiniJAM System Service 0

This directory is the source root for the Stage-0 MiniJAM system service.

The current `src/system-service.placeholder` source intentionally rebuilds the
existing placeholder artifact while the PVM compiler pipeline is still being
wired. It is not a valid executable PVM program and must be replaced before the
Stage-0 public testnet can satisfy the CreateService end-to-end requirement.

Use:

```bash
./scripts/build-system-service.sh
```

The script writes `artifacts/system-service.blob` and
`artifacts/system-service.manifest.json` deterministically from this source
tree.
