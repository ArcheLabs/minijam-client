# MiniJAM System Service 0

This directory contains the Stage-0 MiniJAM system service C source.

The service decodes the ownerless `SystemOpV2` batch and executes
`CreateService` through the standard `NEW`, `WRITE`, and `YIELD` JAM host
calls. `ApplyAllocation` is a separate explicit command; it is never encoded
as a code upgrade and no controller mapping is written.

Use:

```bash
./scripts/build-system-service.sh
```

The script writes `artifacts/system-service.blob` and
`artifacts/system-service.manifest.json` deterministically from this source
tree.
