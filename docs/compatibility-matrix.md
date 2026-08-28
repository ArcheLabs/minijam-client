# MiniJAM compatibility matrix

This matrix is updated with each reproducible integration baseline.

| Component | Revision / version |
|---|---|
| Jambda | `TBD` until the MiniJamSpec consolidation commit is published |
| MiniJamSpec | v1 |
| MiniJAM | `TBD` for the consolidated main commit |
| SDK ABI | 1 |
| System ABI | V2 |
| JamScript target adapter | minijam-0.2 |
| JamScript action protocol | SignedActionV2 |
| Managed State | v1 |
| MiniCells workload | current CLM baseline; workload-only consumer |

The dependency invariant is:

```text
Jambda J → MiniJAM M → JamScript/MiniCells consumers
```

Production dependencies must resolve to merged commits, release commits, or
immutable tags; feature branches are not valid pins.
