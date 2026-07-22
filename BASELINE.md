# MiniJam implementation baseline

- Polkadot SDK target: `polkadot-stable2603`
  (`2e4dd0bc22366a5af820492528869a493b5a5208`)
- Rust toolchain: `nightly-2026-05-02`
- FRAME runtime Wasm target: `wasm32v1-none`
- jambda no-std check target: `wasm32-unknown-unknown`
- jambda submodule commit: `8e03baf44d7f315a2c4313d499d64f5dfe9cf185`
- Gray Paper semantics: `0.7.2`
- Bulletin Chain compatibility commit:
  `b6c2827d232669b525c0906cc20def0e5eb4676b`

The local Gray Paper repository still reports `0.7.1` in its `VERSION` file;
the MiniJam compatibility target is the updated 0.7.2 content agreed by the
project.

The report corpus is intentionally not part of the current implementation
milestone. No report generator or importer is provided.
