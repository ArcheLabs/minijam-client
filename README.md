# MiniJam Client

MiniJam is a solo-chain-hosted JAM execution environment. The current
implementation milestone contains the public protocol/ABI, a deterministic
worker assignment and voting engine, a Bulletin-compatible local simulator,
and the `no_std` jambda MiniJam execution boundary.

Real report corpus import and off-chain refine workers are intentionally
deferred.

## Pinned baselines

- Polkadot SDK: `polkadot-stable2603` (node/runtime scaffold pending)
- Rust: `nightly-2026-05-02`
- Gray Paper semantics: `0.7.2`
- Bulletin Chain compatibility:
  `b6c2827d232669b525c0906cc20def0e5eb4676b`

## Build

```bash
cargo test --workspace
cargo check \
  -p minijam-protocol \
  -p minijam-jamcore-api \
  -p minijam-worker-engine \
  --no-default-features \
  --target wasm32-unknown-unknown
```

The jambda execution boundary is checked from the sibling repository:

```bash
cd /home/libingjiang/jambda
cargo check \
  -p jambda-minijam-executive \
  --no-default-features \
  --target wasm32-unknown-unknown
```
