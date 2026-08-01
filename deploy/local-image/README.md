# MiniJAM local Docker network

Local development only. Uses known public development keys, contains no real-value assets, and is unrelated to the hosted Stage 0 genesis. Do not expose its ports to the internet or reuse these keys anywhere else.

```bash
sha256sum -c minijam-local-<tag>.tar.gz.sha256
tar -xzf minijam-local-<tag>.tar.gz
cd minijam-local-<tag>
./minijam-local up
```

Open http://127.0.0.1:4173. Node RPC is http://127.0.0.1:9944.

The node uses `//Alice`; workers use `//Alice`, `//Bob`, and `//Charlie`; the Playground Relayer is the public deterministic `0x92`-repeated development seed. These are public development keys, not Docker secrets, and must never be used on a public network.

Useful commands: `./minijam-local status`, `logs`, `down`, `up`, and `reset`. If Docker is unavailable, start Docker first. If GHCR pulls fail, verify proxy/registry access. Check that ports 4173 and 9944 are free, and use `logs` for unhealthy services. WSL users must enable Docker Desktop WSL integration. Reset volumes after changing releases.
