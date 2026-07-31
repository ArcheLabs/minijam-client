# Stage 0 native stack

Prepare dependencies, build, and start the local stack:

```bash
./scripts/stage0-native.sh deps
./scripts/stage0-native.sh build
./scripts/stage0-native.sh up
```

The Playground is available at <http://127.0.0.1:4173>.

```bash
./scripts/stage0-native.sh logs
./scripts/stage0-native.sh down
./scripts/stage0-native.sh reset
```

`up` starts the binaries already produced by `build`; it does not rebuild them.
After changing the Web sources, run the Web build before starting again. After
changing Rust sources, run the complete `build` command. Native mode uses the
deterministic development identities and is intended only for local
development and human testing. It is not a production or public-server
deployment method; use the Digest-pinned Stage 0 Compose release instead.
