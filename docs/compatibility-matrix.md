# MiniJAM compatibility matrix

This matrix is updated with each reproducible integration baseline.

| Component | Revision / version |
|---|---|
| Jambda | `788bc054223f81282e4d88a83f05f2fe9e94c121` |
| MiniJamSpec | v1 |
| MiniJAM | `0b352d42726c548e932f81138c8dff7bc9b5a786` |
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
