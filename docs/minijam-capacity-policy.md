# MiniJamSpec capacity policy

MiniJamSpec v1 keeps the canonical per-Refine ceiling at 1,000,000,000 gas.
Capacity changes are evidence-driven:

| Measured Refine P95 | Action |
|---:|---|
| `< 300M` | Keep MiniJamSpec v1 |
| `300M ≤ P95 < 700M` | Keep v1 and raise a capacity warning |
| `P95 ≥ 700M` | Evaluate an explicit MiniJamSpec v2 proposal |
| Valid workload `> 1B` | Require an explicit network upgrade proposal |

Worker concurrency, wall time, peak memory, and local scheduling are
operational measurements. They do not modify TinySpec or FullSpec and do not
silently raise the MiniJamSpec gas ceiling.
