# MiniJAM System Service 0

This directory is the source root for the Stage-0 MiniJAM system service.

The current `src/system-service.pvm.hex` source rebuilds a minimal executable
PVM artifact. It halts successfully; Stage-0 `CreateService` state transitions
are applied by the MiniJAM system-op adapter in `MiniJamExecutive`.

Use:

```bash
./scripts/build-system-service.sh
```

The script writes `artifacts/system-service.blob` and
`artifacts/system-service.manifest.json` deterministically from this source
tree.
