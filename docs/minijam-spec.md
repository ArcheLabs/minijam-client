# MiniJamSpec v1

MiniJamSpec is MiniJAM's canonical network profile. It is not JAM TinySpec,
not JAM FullSpec, and not a MiniCells-specific profile.

The canonical Rust definition is owned by the pinned Jambda revision in
`external/jambda/crates/minijam-spec`. MiniJAM production code imports that
profile; it does not duplicate its constants.

| Constant | MiniJamSpec v1 |
|---|---:|
| `NUM_VALIDATORS` | 6 |
| `NUM_CORES` | 2 |
| `PREIMAGE_EXPUNGE_PERIOD` | 32 |
| `SLOT_DURATION` | 6 |
| `EPOCH_DURATION` | 12 |
| `CONTEST_DURATION` | 10 |
| `TICKETS_PER_VALIDATOR` | 3 |
| `MAX_TICKETS_PER_EXTRINSIC` | 3 |
| `ROTATION_PERIOD` | 4 |
| `NUM_EC_PIECES_PER_SEGMENT` | 1026 |
| `MAX_REFINE_GAS` | 1,000,000,000 |
| `MAX_BLOCK_GAS` | 2,000,000,000 |
| `MAX_LOOKUP_ANCHORAGE` | 24 |

The per-Refine and per-Accumulate operational envelopes are each 1B. The
runtime's `MaxExecutionGas` is a separate 6B aggregate admission budget used
for the block STF and report validation; it is not a 5B per-Refine limit.
