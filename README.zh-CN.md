# MiniJAM 客户端

MiniJAM Client 是 Stage-1 的节点、Worker、Runtime、协议和可选 Service
Toolchain 实现。Stage-1 是当前唯一支持的部署线，历史开发产品和生成的网络
产物不再属于仓库源码。

[English](README.md) | 简体中文

参见 [MiniJamSpec](docs/minijam-spec.md)、[执行边界](docs/execution-boundary.md)
和[兼容性矩阵](docs/compatibility-matrix.md)。

## 仓库结构

| 路径 | 职责 |
| --- | --- |
| `crates/minijam-protocol` | 协议常量、内容引用、报告、投票和状态变更 |
| `crates/minijam-jamcore-api` | 版本化 JamCore 接口和执行类型 |
| `crates/minijam-worker-engine` | 与 Runtime 无关的 Worker 算法 |
| `crates/minijam-worker` | Worker daemon 和 bundle 获取 |
| `crates/minijam-formal-rpc` | 应用无关的 Work 与 bundle gateway |
| `runtime` | FRAME Runtime 和 jambda Executive 集成 |
| `node` | 节点 CLI、RPC、链配置、Aura 和 GRANDPA |
| `deploy/stage1` | 唯一的 Stage-1 Dockerfile 和 Compose 配置 |
| `deploy/compiler` | 可选 Service compiler 镜像 |
| `service-toolchain` | Service 0 编译和一致性验证源码 |
| `scripts` | CI、Stage-1、发布和 Service toolchain 检查 |

## Stage-1 部署

生产部署单元是精确的镜像 digest 和与之匹配的生成式 chain spec。镜像仓库为：

```text
ghcr.io/archelabs/minijam-node
ghcr.io/archelabs/minijam-worker
ghcr.io/archelabs/minijam-formal-rpc
```

单机部署使用 `deploy/stage1/compose.compact.yml`，跨主机私有网络使用
`deploy/stage1/compose.split.yml`。使用精确的 node 镜像运行
`scripts/export-stage1-chain-specs-image.sh` 生成 `stage1.json` 和
`stage1-raw.json`；生成文件不提交到仓库。

Formal RPC 负责 Work-ingress relayer 和 bundle store，Worker 只持有自己的
签名密钥。Node RPC、Worker health、metrics 和 compiler 端点都应保持在部署的
私有边界内。

## 开发环境

仓库通过 `rust-toolchain.toml` 固定 Rust 工具链，并安装 `rustfmt`、`clippy`、
`rust-src`、`wasm32-unknown-unknown` 和 `wasm32v1-none`。完整 Runtime 和节点
构建依赖私有且固定版本的 `external/jambda` submodule。

```bash
git submodule update --init external/jambda
cargo fmt --all -- --check
cargo test --workspace --exclude minijam-runtime --exclude minijam-node
```

核心 Wasm 检查：

```bash
cargo check -p minijam-protocol -p minijam-jamcore-api \
  -p minijam-worker-engine --no-default-features \
  --target wasm32-unknown-unknown
cargo check -p minijam-runtime --no-default-features --target wasm32v1-none
```

重量级构建和 Docker smoke test 交给 GitHub Actions；仓库不再要求本地构建旧的
开发产品或本地发布栈。

## 许可证

Apache License 2.0。
