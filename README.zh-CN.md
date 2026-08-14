# MiniJAM 客户端

English | 简体中文

> MiniJAM Season 2 是 Experience Network，仍处于早期开发阶段。
>

MiniJAM 是一个运行精简版 JAM 协议的独立 Polkadot SDK 链。

## Playground API

MiniJAM Playground API 是公开的开发者 API，并且有意支持跨域浏览器客户端。
所有 `/api/v1/*` 路由都使用 permissive CORS，允许 jam-os、Playground、
localhost 应用、社区 MiniJAM 应用和第三方开发者前端直接调用。

CORS 不是授权边界。读取接口公开；状态变更仍由 signed action、sr25519 钱包签名
和重放保护强制授权。创建和升级 Service 仍要求 Controller；普通 Work 在
Experience 层 permissionless，由 runtime Ingress Relayer 提交。API 不使用 Cookie
Session，也不启用 credentialed CORS。

## Season 2 Experience Network

Season 2 保留 JAM gas limit 和 Jambda Service balance 语义，但移除 Work 执行路径
上的 MiniJAM Service Fuel 收费层。第一版部署支持一个 active Worker。Hub-backed
价值只能通过单向、带重放保护的 `AllocationV1 { allocation_id, target_service,
amount }` 进入队列。MiniJAM 不提供 mJK 释放或反向桥；赎回完全由 Hub 合约决定。

Compact 和 split 部署配置位于 `deploy/season2`，两者使用相同 Runtime、安全 RPC、
隔离的 Compiler 网络和分离的生产密钥。

## 与 JAM 的区别

MiniJAM 把灰皮书中的 JAM 执行模型收缩到一个可在独立链上验证的最小协议面。

- 共识与链：MiniJAM 使用独立 Polkadot SDK 链承载协议状态，替代 JAM 共享共识。
- Worker 与客户端边界：Worker 侧按多客户端目标设计，不同 Worker 客户端可以独立实现提交、验证和投票逻辑；Runtime 侧当前是单一 Runtime，不支持多 Runtime 客户端替换，这会导致链不一致。
- Work 与 Guarantees：MiniJAM 将 guarantee 流水线压缩为 Work、`ReportEnvelopeV1`、候选报告保证金和 Worker 投票。
- Assurance 与可用性：MiniJAM 不实现 JAM 的 assurance 语义，当前用 `BulletinEvidence`、Bulletin-compatible simulator 和 Worker 投票抽象数据可用性边界。
- 争议与裁决：MiniJAM 不实现全局 disputes、judgments 等工作报告正确性、一致性逻辑。将其交由 Work round 内的 Support/Oppose、缺席惩罚、候选报告拒绝惩罚和 equivocation proof。
- 状态与状态转换：MiniJAM 保留了 JAM 的原始状态和状态转换逻辑，但部分状态始终保持默认值。这方便链下 Worker 的多客户端实现，无需实现一套单独的逻辑。
- Accumulate：MiniJAM 保留了累积逻辑，并将其作为 Runtime 运行，因此在运行上无法并行。
- 桥接：Season 2 只接受 Hub→MiniJAM Allocation receipt。MiniJAM 不能释放 Hub
  mJK，赎回仍是 Hub 操作。

## 当前进度

当前仓库已经包含：

- 带版本的 MiniJAM 协议类型、报告信封、Worker 投票和状态变更格式；
- 确定性的 Worker 筛选、任务分配、多轮候选报告和表决逻辑；
- MiniJAM JamCore 执行接口，以及执行结果的规范化与原子状态校验；
- Bulletin 存储抽象和可注入故障的本地模拟器；
- 入站托管与出站释放所需的桥接；
- 在 jambda 基础上实现的 MiniJAM Executive；
- WorkPackage 入口、ContentRef 上限、保持不变的 preimage/system-op 队列、JAM gas
  accounting 和 pallet view 查询；
- `minijam-worker` daemon 骨架，以及可复用的 Worker 内容获取和 bundle 校验 helper。

## 工作流程

一次 Work 从提交到执行的大致流程如下：

1. 用户提交 Work，Runtime 锁定 Work 押金。
2. Worker 模块根据当前 Epoch 和延迟后的随机种子，从活跃 Worker 中确定性地分配验证者。
3. 报告提交者在截止区块前提交带版本的 `ReportEnvelopeV1`，并锁定候选报告保证金。
4. 被分配的 Worker 对候选报告投赞成票或反对票；达到阈值后锁定结果。
5. 候选报告被接受后进入有界执行队列；若被拒绝或超时，则进入下一轮，直至达到最大轮数。
6. Runtime 在区块结束时执行到期报告，先校验并规范化状态变更，再以原子方式写入协议状态。
7. 执行收据、服务输出和桥接效果被记录，供后续组件消费。

Runtime 还提供暂停执行和隔离待执行队列的 Root 管理操作，用于发生执行异常时保护协议状态。

## 仓库结构

| 路径 | 职责 |
| --- | --- |
| `crates/minijam-protocol` | 公共协议常量、内容引用、报告、投票、状态变更和桥接效果类型 |
| `crates/minijam-jamcore-api` | 版本化 JamCore 输入/输出、错误类型、状态读取和执行器接口 |
| `crates/minijam-jamcore-mock` | 用于测试的可配置 Mock 执行器 |
| `crates/minijam-worker-engine` | 与 Runtime 无关的 Worker 排序、分配、投票和惩罚算法 |
| `crates/minijam-worker` | 阶段 0 Worker daemon 入口和运行配置 |
| `crates/minijam-bridge-engine` | 入站/出站桥接账本与管理员状态记录编码 |
| `crates/minijam-bulletin-api` | Bulletin 存储、授权、续期和状态查询接口 |
| `crates/minijam-bulletin-simulator` | Bulletin 兼容的本地文件模拟器及故障注入 |
| `crates/minijam-state-adapter` | 执行输出校验、状态变更规范化和原子应用 |
| `pallets/minijam-workers` | Worker 注册、更新、解绑、任务分配、投票和作恶证明 |
| `pallets/minijam` | Work 生命周期、候选报告、多轮表决、执行队列和协议状态 |
| `pallets/minijam-bridge` | Stage 0 遗留桥接 pallet，未加入 Season 2 Runtime |
| `runtime` | FRAME Runtime 配置与 jambda Executive 集成 |
| `node` | MiniJAM 节点 CLI、RPC、链配置和 Aura/GRANDPA 服务 |

## 当前协议参数

以下数值来自 `minijam-protocol` 的当前开发配置，可能在协议稳定前调整：

| 参数 | 当前值 |
| --- | --- |
| 候选 Worker 集合 | 8 |
| 每轮最多 Work 数 | 4 |
| 每个 Work 分配的 Worker 数 | 1 |
| 赞成/反对阈值 | 1 / 1 |
| Epoch 长度 | 100 个区块 |
| 分配随机种子延迟 | 10 个区块 |
| 报告提交期限 | 20 个区块 |
| 投票窗口 | 10 个区块 |
| 最大候选轮数 | 3 |
| 每个 Worker 每轮最大任务数 | 2 |
| 最低 Worker 质押 | 1,000 UNIT |
| Work 押金 | 10 UNIT |
| 候选报告保证金 | 10 UNIT |
| 最大状态增量 | 4 MiB |

## 固定基线

- Polkadot SDK：`polkadot-stable2603`，当前固定至提交 `2e4dd0bc22366a5af820492528869a493b5a5208`；
- Rust：`nightly-2026-05-02`；
- JAM 灰皮书语义：`0.7.2`；
- Bulletin Chain 兼容基线：`b6c2827d232669b525c0906cc20def0e5eb4676b`。

## 准备开发环境

仓库中的 `rust-toolchain.toml` 会固定 Rust 工具链，并安装 `rustfmt`、`clippy`、`rust-src`、`wasm32-unknown-unknown` 和 `wasm32v1-none`。

公开协议、Worker、桥接、Bulletin 和 state-adapter crate 已包含在本仓库中。完整 Runtime 与节点构建当前依赖私有 jambda submodule；授权开发者在构建 Runtime 前需要先初始化：

```bash
git submodule update --init external/jambda
```

检出的 submodule 版本必须包含 `crates/minijam-executive`；生产 Runtime 与节点构建只面向拥有该私有 jambda 版本权限的开发者。

进入 `minijam-client` 后，运行公开 crate 与 pallet 测试：

```bash
cargo test --workspace --exclude minijam-runtime --exclude minijam-node
```

检查需要在 Wasm 中运行的核心 crate 是否保持 `no_std` 兼容：

```bash
cargo check \
  -p minijam-protocol \
  -p minijam-jamcore-api \
  -p minijam-worker-engine \
  --no-default-features \
  --target wasm32-unknown-unknown
```

检查 Runtime Wasm 依赖闭包：

```bash
cargo check \
  -p minijam-runtime \
  --no-default-features \
  --target wasm32v1-none
```

## 构建并运行本地节点

当前已经包含节点源码、链配置、RPC 接线、Aura 出块和 GRANDPA 服务。完整 Runtime 与节点构建当前仍需要在私有 jambda 中修复 `TinySpec` state backend 和 codec 路径上的 `generic_const_exprs` trait-solver overflow。

Runtime 构建路径修复后，预期的本地节点流程如下：

```bash
cargo build --release -p minijam-node
cargo run --release -p minijam-node -- --dev --tmp
cargo run --release -p minijam-node -- export-chain-spec --chain dev
```

开发链使用 Alice 作为 Aura/GRANDPA 权威节点和 Sudo 账户；本地测试网配置包含 Alice 与 Bob 两个权威节点。这些预设仅供本地开发。

## 阶段 0 Worker

Worker daemon 入口为 `minijam-worker`。当前 binary 会校验运行配置并启动轮询循环；链上 RPC 任务发现和 WorkReport 提交是下一步接线内容。

运行一次性 readiness check：

```bash
cargo run -p minijam-worker -- --once
```

使用显式本地端点运行：

```bash
cargo run -p minijam-worker -- \
  --rpc-url ws://127.0.0.1:9944 \
  --ipfs-gateway http://127.0.0.1:8080 \
  --poll-interval-ms 1000 \
  --max-bundle-bytes 16777216
```

可复用的 Worker engine 已支持校验 `ContentRef` 的 size/hash 承诺、通过 decoder hook 校验 bundle package-hash 承诺，并提供 memory、HTTP URL 和 IPFS gateway fetcher 适配层。

## 开发检查

提交变更前建议运行：

```bash
cargo fmt --all -- --check
cargo test --workspace --exclude minijam-runtime --exclude minijam-node
```

涉及 Runtime 执行边界的变更还应同时执行上文的 Wasm `no_std` 检查。

## 兼容性与稳定性

- 公共协议当前为 `PROTOCOL_VERSION_V1`，JamCore 接口当前为 `INTERFACE_VERSION = 1`。
- 报告、批次、状态值和执行队列均有明确上限，以保证 Runtime 执行有界。
- `minijam-bulletin-simulator` 只复现本地开发所需的 Bulletin 语义。
- 经济参数、惩罚比例和管理权限仍属于开发配置，不应视为主网最终参数。

## 许可证

本项目采用 Apache License 2.0 许可证。
