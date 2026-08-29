# MiniJAM compatibility matrix

This matrix is updated with each reproducible integration baseline.

| Component | Revision / version |
|---|---|
| Jambda | `fe67ecf5ccbe16b3490d73cc4d8b1e48eb7bea86` |
| MiniJamSpec | v1 |
| MiniJAM | `c4dec2db5d59ab40f8293335e29c94dd82b8eaf4` |
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
