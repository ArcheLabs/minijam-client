# MiniJAM execution boundary

| Layer | Owner | Meaning |
|---|---|---|
| JAM `ChainSpec` | Jambda | TinySpec and FullSpec reference/conformance profiles |
| `MiniJamSpec` | MiniJAM/Jambda integration | Canonical MiniJAM network constants |
| Runtime policy | MiniJAM runtime | Deadlines, pending-work limits, aggregate execution budget |
| Worker strategy | MiniJAM worker | Local `execution_lanes`, cache and scheduling choices |
| Application ABI | JamScript / service target | SDK ABI 1, PVM entry points and service payloads |

`execution_lanes` is local worker concurrency, not `NUM_CORES` and not a
consensus parameter. The reproducible default is `1`; deployments may choose
`4` without changing WorkPackage bytes, WorkReport ordering, or the PVM ABI.

JamScript depends on the MiniJAM target adapter, network identity and stable
application ABI. It must not embed MiniJamSpec constants. MiniCells is a
workload consumer and should use measured gas and worker policy before any
network profile upgrade is proposed.
