# MiniJAM System Service 0

This directory contains the Stage-0 MiniJAM system service C source.

The service decodes accumulated `SystemOpBatch` results and executes
`CreateService` through the standard `NEW`, `WRITE`, and `YIELD` JAM host
calls. The narrowly scoped native adapter remains responsible only for
`UpgradeService`.

Use:

```bash
./scripts/build-system-service.sh
```

The script writes `artifacts/system-service.blob` and
`artifacts/system-service.manifest.json` deterministically from this source
tree.
