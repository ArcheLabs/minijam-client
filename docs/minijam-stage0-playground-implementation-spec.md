# MiniJAM Stage 0 Playground 上线实施规范

**目标仓库：** `ArcheLabs/minijam-client`  
**审阅基线：** `main`，截至 `5ba32647c1aa43425ca3a2d9aa11667c07f48c94`  
**配套 Jambda：** `external/jambda` 固定子模块；审阅到的配套实现基线为 `e15168173267ec1191dbf4d5aa48b2e4abe4cff9`  
**目标：** Codex 持续实施，直到 MiniJAM Stage 0 Playground 达到公开测试网上线标准，而不是只完成脚手架或演示性页面。

---

## 0. Codex 执行约定

Codex 应将本文作为主实施规范，并遵守以下规则：

1. **先审阅再修改。** 每个阶段开始前检查本文提到的文件是否已发生变化，以当前代码为准，但不得改变已对齐的产品目标。
2. **不得重复实现已有能力。** 当前仓库已有 Runtime、Worker、Refine、Candidate、Vote、Accumulate、Service 0 system-op 路径、RPC、部署拓扑和发布清单，应在其上补齐。
3. **不得以 TODO、mock、假成功或硬编码前端状态作为验收。**
4. **每个阶段必须有自动化测试和可运行验收命令。**
5. **优先小型、原子提交。** 每个提交只解决一个清晰问题，并保持仓库可构建。
6. **协议类型变更必须同步：**
   - `minijam-protocol`
   - pallet
   - Runtime API / Node RPC
   - Worker
   - CLI
   - Playground API
   - 测试向量和文档
7. **所有链上执行路径必须使用真实 Jambda。** 不允许前端或 API 伪造 WorkReport。
8. **Stage 0 不实现本地 Refine 预览。** 用户提交后直接由 Worker Refine。
9. **任何会改变链上状态的用户操作必须经过身份验证与所有权校验。**
10. **编译接口公开，不要求身份验证，但必须严格限制资源。**
11. 每完成一个里程碑，更新：
    - `docs/stage0-public-testnet-checklist.md`
    - `docs/STAGE0-RELEASE-CHECKLIST.md`
    - 本文新增的实施进度文件 `docs/STAGE0-PLAYGROUND-IMPLEMENTATION-CHECKLIST.md`
12. 不得宣布完成，直到本文“最终上线门槛”全部通过并留下证据。

---

# 1. 最终产品定义

MiniJAM Stage 0 是一个面向开发者的浏览器 Playground。完整体验为：

```text
打开 Playground
→ 编写或修改单文件 C/C++ Service
→ 公开编译为 PVM/JAM Blob
→ 钱包签名登录
→ 创建 Service
→ Service 0 创建 Service 并记录 Controller
→ 提交代码 Preimage
→ 得到 Service ID
→ 填写 Payload / Extrinsic Data
→ Playground API 构造标准 WorkPackage 和 WorkBundle
→ 创建链上 Work
→ 被分配的 Worker 获取 Bundle
→ Worker 使用 Jambda 执行 Refine
→ Worker 提交 Candidate WorkReport
→ 独立 Worker 投票
→ WorkReport 被接受
→ Runtime 调用 MiniJamExecutive / Jambda Accumulate
→ Service 状态变化
→ Playground 显示 Refine、Report、Accumulate 和状态结果
```

还必须支持：

```text
编译新版本
→ Service Owner 发起升级
→ Controller 校验
→ 更新 Service code hash / gas 配置
→ 提交新代码 Preimage
→ 后续 Work 使用新代码
```

公开测试网关闭后，同一体验必须可以通过本地 Docker 启动。

---

# 2. 已对齐的产品决策

## 2.1 Stage 0 包含

- 浏览器单文件 C 和受限 C++ 编辑。
- 服务端/容器内编译。
- 标准 PolkaVM/JAM Blob。
- 真实 Service 注册。
- Service Controller。
- Service 升级。
- Payload 和 Extrinsic Data 提交。
- 标准 WorkPackage、WorkBundle、WorkReport。
- Worker Refine。
- Candidate 和 Worker 投票。
- Jambda Accumulate。
- Service 原始 KV 状态查看。
- 签名登录与通用 API 鉴权。
- 公开测试网部署。
- 本地 Docker 开发环境。
- 完整跨进程 E2E。
- 48 小时 Canary 和正式发布证据。

## 2.2 Stage 0 不包含

- 浏览器内 Clang。
- 本地 Refine 预览。
- 完整 JIP-2。
- TypeScript WorkPackage Builder。
- 钱包直接提交每一笔链上交易。
- Service ABI 自动表单。
- 多文件 C/C++ 工程。
- 完整 C++ 标准库。
- Python。
- JamScript。
- 多 Core。
- 多 WorkItem。
- Imports、Exports、Prerequisites。
- 正式经济模型。
- DOT/MINI 计费。
- 动态 Is-Authorized。
- IPFS 节点或 Bulletin Chain 正式接入。
- Service transfer、delete、多签、协作者。
- 将 MiniJam Executive 移出 Jambda。

---

# 3. 当前仓库状态

## 3.1 已具备

当前仓库已经具备：

- Polkadot SDK Node、Runtime、Aura、GRANDPA。
- MiniJAM protocol types。
- Work ingress、Worker assignment、Candidate、Vote、Round、Execution queue。
- Jambda Refine-backed Candidate 生成。
- Jambda Accumulate-backed Runtime STF。
- Preimage queue。
- System-op queue。
- Service fuel 和 Work deposit 会计。
- Service 0 genesis state。
- CreateService system-op 执行路径。
- Worker daemon、签名 Candidate 和 Vote。
- Runtime API 和只读 Node RPC。
- CLI call-data 工具。
- Stage 0 chain spec。
- 3 Authority + Public RPC + 3 Worker 部署拓扑。
- Prometheus、告警、备份和 Runtime upgrade rehearsal 文档。
- Release checklist 骨架。

## 3.2 当前关键事实

### `submit_work` 的当前代码语义

当前 `pallets/minijam/src/lib.rs` 中：

```rust
pub fn submit_work(
    origin,
    canonical_work_package,
    bundle_ref,
)
```

由普通签名账户调用，负责：

- 验证 WorkPackage；
- 验证 ContentRef；
- 预留 Service fuel；
- 锁定 Work deposit；
- 创建 WorkRecord；
- 分配 Worker。

因此在**当前代码中**，`submit_work` 是 Work ingress，而不是 Worker 输出接口。

Worker 输出走：

```rust
submit_candidate
```

Worker 投票走：

```rust
pallet_minijam_workers::submit_vote
```

Stage 0 不要求重命名 pallet call，以避免 call index 和现有测试大面积变化；但产品 API 和文档必须使用清晰名称：

```text
Create Work Request → pallet submit_work
Submit WorkReport Candidate → pallet submit_candidate
Submit Worker Vote → pallet submit_vote
```

### 当前 Service 0 仍不是最终形态

当前 `artifacts/system-service.blob` 是 27 字节最小可执行 Blob，只会成功 halt。CreateService 由 Jambda Accumulate 中的 MiniJAM system adapter 特殊处理。

因此现有实现证明了：

- System op 能进入 Accumulate；
- Service 可以被创建；
- Controller 和 Receipt 可以写入状态；

但尚未证明：

- Service 0 的 PVM 程序自身执行 CreateService。

现有发布清单已经要求“Service 0 is a real executable PVM artifact”，所以这是上线阻断项。

### 当前 Worker 已能真实 Refine

当前 Worker 已经：

- 获取 finalized pending tasks；
- 获取并校验 Bundle；
- 解码 Jambda `MiniJamWorkBundleV1`；
- 加载协议状态；
- 调用 Jambda `compute_work_report`；
- 构造 ReportEnvelope；
- 签名并提交 Candidate；
- 签名并提交 Support Vote。

但仍存在以下上线缺口：

1. 协议状态读取没有绑定 WorkPackage anchor；
2. Pending task 对所有 Worker 全局可见；
3. Candidate submitter 没有在 pallet 中强制为 assigned Worker；
4. Worker 默认配置未启用 Candidate/Vote；
5. Stage 0 Compose 中的 Worker 指向不存在的 `ipfs-gateway`；
6. Worker 的手写 HTTP 客户端只支持 `http://`；
7. 公开部署尚未经过真实跨进程 E2E。

### 当前经济参数不符合最新产品目标

Runtime 当前仍配置：

- 非零 WorkDeposit；
- 非零 CandidateBond；
- 非零 RefineGasPrice；
- 非零 AccumulateGasPrice；
- Reward、Slash 和 Fuel。

Stage 0 产品决定是不向 Playground 用户暴露余额、收费和 Gas 定价。执行 Gas limit 仍保留，但用户不承担费用。

---

# 4. 目标仓库架构

在当前仓库中新增以下结构：

```text
minijam-client/
├── Cargo.toml
├── package.json
├── pnpm-workspace.yaml
│
├── apps/
│   └── playground/
│       ├── src/
│       ├── public/
│       ├── package.json
│       ├── vite.config.ts
│       └── README.md
│
├── crates/
│   ├── ...existing...
│   ├── minijam-chain-client/
│   ├── minijam-work-package-builder/
│   ├── minijam-auth/
│   ├── minijam-compiler-api/
│   └── minijam-playground-api/
│
├── service-toolchain/
│   ├── compiler/
│   │   ├── Dockerfile
│   │   ├── scripts/
│   │   ├── toolchain.lock
│   │   └── README.md
│   └── sdk/
│       ├── include/minijam/
│       ├── src/
│       ├── LICENSES/
│       └── README.md
│
├── services/
│   └── system-service/
│       ├── src/
│       ├── include/
│       ├── tests/
│       └── README.md
│
├── examples/
│   └── services/
│       └── counter/
│           ├── service.c
│           ├── service.cpp
│           └── README.md
│
├── tests/
│   └── e2e-stage0/
│       ├── scenarios/
│       ├── fixtures/
│       ├── scripts/
│       └── README.md
│
├── deploy/
│   ├── dev/
│   └── stage0/
│
└── docs/
    ├── STAGE0-PLAYGROUND-IMPLEMENTATION-CHECKLIST.md
    ├── playground-api.md
    ├── service-sdk.md
    └── stage0-architecture.md
```

## 4.1 Rust workspace 新成员

加入：

```toml
"crates/minijam-chain-client",
"crates/minijam-work-package-builder",
"crates/minijam-auth",
"crates/minijam-compiler-api",
"crates/minijam-playground-api",
```

## 4.2 JavaScript workspace

根目录新增：

```yaml
packages:
  - "apps/*"
```

前端只使用一个 package，不提前拆分 UI package、types package 等。

## 4.3 组件边界

### `minijam-chain-client`

负责：

- JSON-RPC；
- Runtime metadata；
- Signed extrinsic；
- Nonce；
- Finalization；
- Event 解析；
- Runtime call 封装。

应从 Worker 和 CLI 中逐步抽取重复的签名、nonce、genesis hash、raw extrinsic 提交逻辑。

### `minijam-work-package-builder`

纯 Rust library：

- 无 HTTP；
- 无数据库；
- 无签名账户；
- 无 UI DTO。

输入 Service、Anchor、Payload 和 Extrinsics，输出：

- `WorkPackage`
- canonical package bytes
- Jambda `MiniJamWorkBundleV1`
- bundle bytes
- package hash
- ContentRef metadata

### `minijam-auth`

负责：

- challenge；
- Substrate signature verification；
- nonce replay protection；
- session；
- authenticated identity；
- 通用 authorization helper。

不负责 HTTP 路由，不直接访问链。

### `minijam-compiler-api`

内部 Compiler 服务：

- 接收单文件 C/C++；
- 调用固定工具链；
- 返回 Blob 和 diagnostics；
- 在隔离容器内运行。

### `minijam-playground-api`

Stage 0 编排层：

- 提供产品 HTTP API；
- 提供静态前端；
- 调用 Compiler；
- 管理 Auth 和 Session；
- 管理 Build、Deployment、Upgrade、Work Job；
- 调用链；
- 构造 WorkPackage；
- 保存 WorkBundle；
- 提供 Bundle gateway；
- 查询 Work/Candidate/Receipt/Service state；
- 暴露 metrics 和 health。

---

# 5. 身份、Controller 与链上权限

## 5.1 身份模型

身份直接使用 Substrate `AccountId32`。

不使用：

- 邮箱；
- 用户名；
- 密码；
- KYC；
- 云端账户资料。

用户通过 Polkadot 浏览器扩展签署登录挑战。

## 5.2 Challenge 格式

必须使用确定性、域隔离消息：

```text
MiniJAM Playground Authentication
Version: 1
Domain: <configured public domain>
Genesis Hash: <32-byte chain genesis hash>
Account: <SS58 or hex AccountId32>
Nonce: <cryptographically random 32 bytes>
Issued At: <RFC3339>
Expires At: <RFC3339>
```

校验：

- Account 与签名匹配；
- Genesis hash 匹配当前网络；
- Domain 匹配配置；
- Challenge 未过期；
- Nonce 未消费；
- Challenge 只能使用一次；
- 支持 sr25519、ed25519、ecdsa 对应的 Substrate MultiSignature。

## 5.3 Session

采用随机 opaque session token，不把私钥或权限信息放在客户端可修改 JWT 中。

数据库保存：

```text
session_id
token_hash
account_id
created_at
expires_at
revoked_at
last_seen_at
```

默认：

```text
Challenge TTL: 5 minutes
Session TTL: 1 hour
Maximum active sessions/account: 10
```

生产环境只通过 HTTPS。

## 5.4 鉴权范围

### 无需鉴权

```text
POST /api/v1/builds
GET  /api/v1/builds/:build_id
GET  /api/v1/network
GET  /api/v1/services/:service_id
GET  /api/v1/services/:service_id/storage
GET  /api/v1/public/jobs/:public_id
```

### 需要身份

```text
POST /api/v1/services
POST /api/v1/services/:service_id/upgrades
POST /api/v1/services/:service_id/work
GET  /api/v1/me/services
GET  /api/v1/jobs/:job_id
```

### 权限规则

```text
CreateService:
  已登录即可；创建后成为 Controller。

UpgradeService:
  当前用户必须等于链上 Controller。

SubmitServiceWork:
  当前用户必须等于链上 Controller。
```

## 5.5 Controller 的链上来源

Controller 必须写入 Service 0 storage：

```text
system/controller/<service_id> → AccountId32
```

Playground API 每次 Upgrade 和 Submit Work 前都从链上读取，不把本地数据库作为唯一真相。

## 5.6 Relayer 与当前 system-op sender 问题

Playground API 使用固定 relayer 签署链上 extrinsic。如果继续从 extrinsic origin 推导 Controller，所有服务都会属于 relayer。

因此必须修改 SystemCommand：

```rust
pub enum SystemCommandV1 {
    CreateService {
        controller: [u8; 32],
        code_hash: Hash,
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    },
    UpgradeService {
        controller: [u8; 32],
        service_id: u32,
        code_hash: Hash,
        code_len: u32,
        min_item_gas: u64,
        min_memo_gas: u64,
    },
}
```

删除或停止暴露：

```text
initial_balance
```

Stage 0 Service balance 使用固定协议默认值，不由用户输入。

SystemOp 的 `sender` 仍可记录 relayer origin，用于 nonce 和审计；Controller 由 command 明确携带。

## 5.7 API 鉴权不得被直接链调用绕过

仅在 API 中检查 Owner，而 Runtime 允许任意账户调用 `submit_work`，不能满足“只有 Owner 可以提交 Service 数据”。

Stage 0 采用可信 ingress relayer：

```rust
type WorkIngressOrigin: EnsureOrigin<RuntimeOrigin>;
type SystemIngressOrigin: EnsureOrigin<RuntimeOrigin>;
type PreimageIngressOrigin: EnsureOrigin<RuntimeOrigin>;
```

Stage 0 配置只允许 Playground relayer。

适用：

- `submit_work`
- `submit_system_op`
- `submit_preimage`

不适用：

- `submit_candidate`
- `submit_vote`

Worker 继续使用自己的 Worker 身份。

本地开发链可以通过 genesis 配置本地 relayer，不要通过编译 feature 产生不同协议语义。

## 5.8 Candidate 权限

当前 `submit_candidate` 仅要求 signed origin，没有证明 submitter 是本轮 assigned Worker。

必须增加：

1. 从 origin 查 `WorkerByAccount`；
2. 获取 `(work_id, round)` assignment；
3. 要求 WorkerId 在 assignment 中；
4. 可选进一步规定 Candidate Producer：
   - 最小 WorkerId 的 assigned Worker 优先提交；
   - 其他 assigned Worker在超时前可 fallback。
5. Candidate Bond 在 Stage 0 为零，但权限仍必须验证。

新增错误：

```rust
CandidateSubmitterNotWorker
CandidateSubmitterNotAssigned
CandidateProducerNotSelected
```

---

# 6. Stage 0 经济参数

目标是保留执行限制，移除用户经济摩擦。

Stage 0 Runtime：

```text
WorkDeposit = 0
CandidateBond = 0
CandidateRejectionSlash = 0
AcceptedSubmitterReward = 0
TimelyVoteReward = 0
RefineGasPrice = 0
AccumulateGasPrice = 0
```

保留：

- Refine gas limit；
- Accumulate gas limit；
- MaxExecutionGas；
- Worker assignment；
- Worker signature；
- Vote threshold；
- Worker stake结构。

Service fuel 路径有两种可接受实现，优先选择 A：

### A. 推荐：Stage 0 零价格

- Gas price 为零；
- reserve/settle 仍运行；
- reserved/charged 值为零；
- 会计代码和 invariant 保持覆盖。

### B. 备选：自动无限额度

仅当零价格导致已有逻辑无法工作时：

- API 部署后自动 fund；
- UI 不显示余额；
- 测试证明不会阻断执行。

不得删除 Gas meter 或 Execution gas limit。

---

# 7. WorkPackage Builder

## 7.1 固定 Stage 0 约束

```text
core_index = 0
items.len = 1
imports = []
export_count = 0
prerequisites = []
authorization = fixed allow-all
refine_gas_limit = fixed config
accumulate_gas_limit = fixed config
anchor = finalized MiniJAM block
lookup_anchor = anchor
```

## 7.2 输入

```rust
pub struct BuildWorkInput {
    pub service_id: u32,
    pub service_code_hash: [u8; 32],
    pub payload: Vec<u8>,
    pub extrinsics: Vec<Vec<u8>>,
    pub anchor_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub lookup_anchor_slot: u32,
}
```

## 7.3 WorkItem

构造：

```rust
WorkItem {
    service: service_id,
    code_hash,
    refine_gas_limit: STAGE0_REFINE_GAS,
    accumulate_gas_limit: STAGE0_ACCUMULATE_GAS,
    export_count: 0,
    payload,
    import_segments: vec![],
    extrinsic: extrinsics.map(|bytes| ExtrinsicSpec {
        hash: blake2b_256(bytes),
        len: bytes.len() as u32,
    }),
}
```

External data 必须按照 WorkItem/extrinsic 顺序存入：

```text
Vec<Vec<ByteSequence>>
```

## 7.4 WorkPackage

```rust
WorkPackage {
    auth_code_host: STAGE0_AUTH_CODE_HOST,
    auth_code_hash: STAGE0_AUTH_CODE_HASH,
    context: RefineContext {
        anchor,
        state_root,
        beefy_root: zero,
        lookup_anchor: anchor,
        lookup_anchor_slot,
        prerequisites: vec![],
    },
    authorization: empty or fixed token,
    authorizer_config: empty or fixed config,
    items: vec![item],
}
```

## 7.5 Bundle

必须直接复用 Jambda：

```rust
jambda_refine::WorkReportInput
jambda_refine::MiniJamWorkBundleV1
WorkReportInput::encode_auditable_bundle()
```

不得重新定义相似但不兼容的 Bundle。

输出：

```rust
pub struct BuiltWork {
    pub work_package: WorkPackage,
    pub canonical_work_package: Vec<u8>,
    pub package_hash: [u8; 32],
    pub bundle_bytes: Vec<u8>,
    pub content_ref: ContentRef,
}
```

## 7.6 Golden tests

至少包括：

- Package JAM encode/decode；
- Package hash；
- Bundle Jambda decode；
- `package_hash_matches()`；
- external data hash/length；
- trailing bytes reject；
- empty imports/exports/prerequisites；
- fixed authorization；
- deterministic output；
- Coded fixture由 Worker成功生成 Candidate。

---

# 8. Finalized Context 与历史状态一致性

这是 P0 正确性要求。

## 8.1 当前问题

- Pending tasks 在 finalized block 查询；
- WorkPackage 有 anchor；
- Worker 的 `ProtocolStateSource` 当前通过 `minijam_getProtocolState` 查询 best state；
- 因此 Refine 可能在不同状态上执行。

## 8.2 目标

Builder、Worker 和 Candidate validation 必须使用同一个 finalized anchor。

## 8.3 RPC 修改

新增：

```text
minijam_getFinalizedContext
```

返回 SCALE 或结构化 JSON：

```rust
pub struct FinalizedContextV1 {
    pub block_hash: [u8; 32],
    pub block_number: u32,
    pub state_root: [u8; 32],
    pub slot: u32,
}
```

新增：

```text
minijam_getProtocolStateAt(block_hash, key)
minijam_getServiceInfoAt(block_hash, service_id)
minijam_getServiceStorageAt(block_hash, service_id, key)
minijam_getServicePreimageAt(block_hash, service_id, code_hash)
minijam_getServiceControllerAt(block_hash, service_id)
```

保留现有 RPC 兼容，但 Playground 和 Worker 必须使用 `At` 版本。

## 8.4 Worker 修改

修改 trait：

```rust
trait ProtocolStateSource {
    fn protocol_state_value_at(
        &self,
        block_hash: [u8; 32],
        key: [u8; 31],
    ) -> Result<Option<Vec<u8>>, WorkerError>;
}
```

`prepare_candidate_envelope`：

1. 先解码 Bundle；
2. 读取 `bundle.work_package.context.lookup_anchor`；
3. 使用该 hash 创建 `ProtocolStateDb`；
4. 所有 ServiceInfo、Preimage、Storage 读取都绑定该 block；
5. 拒绝非 finalized 或未知 anchor。

## 8.5 测试

- Anchor 状态与 best 状态不同，Worker必须使用 anchor；
- best 前进不改变 WorkReport；
- 非 finalized anchor reject；
- lookup anchor code preimage不存在时返回 BadCode；
- 重启后同一个 task 产生同一 report hash。

---

# 9. 默认 Is-Authorized

Stage 0 暂不实现动态 authorization，但不得破坏 WorkPackage 结构。

推荐实现：

1. 构建一个最小 `allow-all-authorizer.blob`；
2. 在 genesis 中为固定 host Service 提供该 preimage；
3. 初始化 core 0 对应 authorization state；
4. Builder 填固定 hash/config/token；
5. Worker仍走 Jambda正常 Is-Authorized；
6. 测试 report 中：
   - authorizer hash 正确；
   - auth output 确定；
   - auth gas used 可记录。

如果现有 TinySpec 初始化 authorization state 成本过高，允许临时使用 Worker adapter：

```text
stage0_allow_all_authorization = true
```

但必须：

- 只在 MiniJAM Worker 层；
- 不改通用 Jambda一致性语义；
- WorkPackage仍携带合法固定字段；
- 代码有明确 Stage0 配置开关；
- 发布文档列为 Stage0 deviation；
- 测试证明无法被用户切换为其他授权模式。

优先完成真实 allow-all authorizer。

---

# 10. Service SDK 与 Compiler

## 10.1 上游利用原则

优先利用 JamBrains/PolkaVM 已有工具链：

```text
clang 20
polkavm-cc
polkavm-c++
PolkaVM sysroot
polkatool
polkavm-to-jam
```

在复制任何 JamBrains SDK 源码前：

1. 查明许可证；
2. 记录上游 commit；
3. 在 `service-toolchain/sdk/LICENSES/` 保存许可证说明；
4. 若无明确许可证，不复制代码，基于公开 ABI 独立实现。

## 10.2 编译流程

```text
source.c / source.cpp
→ fixed MiniJAM SDK
→ polkavm-cc / polkavm-c++
→ ELF
→ polkatool link
→ .polkavm
→ polkavm-to-jam
→ .blob
```

## 10.3 支持范围

### C

- 单文件；
- C23 或工具链稳定支持版本；
- 固定 include path；
- 无文件、网络、线程。

### C++

- 单文件；
- `-fno-exceptions`
- `-fno-rtti`
- `-fno-threadsafe-statics`
- 不承诺 STL；
- 禁止动态链接；
- SDK headers 使用 `extern "C"`。

## 10.4 SDK 最小 API

```c
// lifecycle
MINIJAM_REFINE
MINIJAM_ACCUMULATE

// refine input
minijam_payload(...)
minijam_extrinsic_count(...)
minijam_extrinsic(...)

// refine output
minijam_refine_ok(...)
minijam_refine_error(...)

// accumulate input
minijam_result_count(...)
minijam_result(...)

// storage
minijam_storage_read(...)
minijam_storage_write(...)
minijam_storage_delete(...)

// diagnostics
minijam_log(...)

// completion
minijam_yield(...)
```

底层封装：

- register ABI；
- memory pointer/length；
- HostCall IDs；
- entry dispatch；
- minimal libc/memory；
- no allocator或固定 bump allocator。

## 10.5 Compiler API

内部接口：

```http
POST /internal/v1/compile
```

请求：

```json
{
  "language": "c",
  "source": "...",
  "optimization": "O0"
}
```

响应：

```json
{
  "success": true,
  "blobBase64": "...",
  "codeHash": "0x...",
  "codeLength": 1234,
  "diagnostics": [],
  "toolchain": {
    "clang": "...",
    "polkavm": "...",
    "polkatool": "...",
    "sdkCommit": "..."
  }
}
```

## 10.6 编译隔离

强制：

- 无网络；
- read-only rootfs；
- 临时工作目录；
- 非 root；
- seccomp/AppArmor 或容器默认强化；
- CPU limit；
- memory limit；
- process limit；
- source limit；
- output limit；
- timeout；
- 删除临时目录；
- 不接受自定义 compiler flags；
- 不接受 include 文件路径；
- 不接受 shell command。

默认限制建议：

```text
Source: 256 KiB
Blob: 1 MiB
Compile timeout: 15 s
Memory: 1 GiB
CPU: 1 core
Concurrent builds/IP: 2
Rate: 20 builds/hour/IP
```

## 10.7 编译验收

- C Counter 编译；
- C++ Counter 编译；
- 语法错误返回行列；
- 无限模板/大源码被限制；
- 网络不可访问；
- 工具链版本固定；
- 相同源码生成完全相同 Blob；
- Blob 可被 Jambda predecode；
- Blob 可在 Worker Refine 与 Runtime Accumulate 中执行。

---

# 11. Service 0 与升级

## 11.1 CreateService：上线前必须真实 PVM 执行

保留 MiniJamExecutive 将 system ops 转为 synthetic WorkReport 的逻辑。

删除或关闭 Jambda Accumulate 中对 CreateService 的直接 adapter：

```text
run_minijam_system_service(CreateService)
```

改为：

```text
Synthetic WorkReport
→ 普通 progress_accumulate
→ 加载 Service 0 Blob
→ PVM Accumulate
→ NEW HostCall
→ WRITE Controller / Receipt / Service index
→ YIELD
```

Service 0 C 程序：

1. FETCH WorkResult item；
2. 解码 `SystemOpBatch`；
3. 校验版本和 request_id；
4. 遍历 command；
5. CreateService 调用 `NEW`；
6. 写：
   - `system/last-nonce/<sender>`
   - `system/receipt/<request_id>`
   - `system/controller/<service_id>`
   - `system/services/<controller>/<service_id>`
7. YIELD。

## 11.2 Receipt

定义稳定格式：

```rust
pub enum SystemReceiptV1 {
    ServiceCreated {
        service_id: u32,
        controller: [u8; 32],
    },
    ServiceUpgraded {
        service_id: u32,
        controller: [u8; 32],
        code_hash: [u8; 32],
    },
    Rejected {
        code: u32,
    },
}
```

不要继续仅写裸 `u32` Service ID。

## 11.3 UpgradeService

标准 `upgrade` HostCall只能更新当前执行 Service 自身，Service 0 无法直接升级任意目标 Service。

Stage 0 采用一个明确、窄化的 MiniJAM deviation：

- CreateService 必须真实 PVM；
- UpgradeService 可由 Jambda MiniJAM system adapter处理；
- adapter只处理 `UpgradeService`；
- adapter校验：
  - Service存在；
  - command controller 等于链上 controller；
  - code length合法；
  - hash/len lookup request正确生成；
- adapter更新：
  - ServiceInfo code hash；
  - min item gas；
  - min memo gas；
  - code lookup request；
  - upgrade receipt；
- CreateService不得再由 adapter处理。

必须将该逻辑隔离到明确模块，例如：

```text
crates/minijam-executive/src/system_upgrade.rs
```

并在文档中标记为 Stage0-specific。后续可替换为正式 Service 管理 ABI。

## 11.4 Preimage

部署/升级流程：

```text
Create/Upgrade system op finalized
→ 查询 receipt
→ 获取 target service_id
→ 构造 Jambda Preimage { requester: service_id, blob }
→ JAM encode
→ submit_preimage
→ 等待 PendingPreimage 消失
→ 查询目标 Service lookup/preimage已可用
→ READY
```

不得把原始 Blob 直接传给 `submit_preimage`。

## 11.5 Service 0 测试

- C source reproducible build；
- Blob manifest；
- PVM predecode；
- CreateService真实 VM路径；
- Controller写入；
- Receipt格式；
- 重复 request id；
- nonce；
- 无效 command；
- insufficient system service balance；
- invalid code len；
- Upgrade owner成功；
- 非 Owner升级失败；
- Upgrade preimage ready；
- 创建/升级失败不会 panic Runtime。

---

# 12. Playground API

## 12.1 技术选择

推荐：

```text
Rust
Axum
Tokio
Tower HTTP
SQLx + SQLite
jsonrpsee / reqwest
Prometheus metrics
OpenAPI
```

SQLite 保存：

- challenges；
- sessions；
- builds metadata；
- jobs；
- deployments；
- observed state snapshots。

Blob 和 Bundle 保存在文件卷，不存 SQLite BLOB。

## 12.2 公开 API

### Network

```http
GET /api/v1/network
```

返回：

- chain name；
- genesis hash；
- finalized block；
- runtime version；
- API version；
- compiler version；
- Stage0 limits。

### Build

```http
POST /api/v1/builds
```

请求：

```json
{
  "language": "c",
  "source": "...",
  "optimization": "O0"
}
```

响应：

```json
{
  "buildId": "build_<random>",
  "status": "SUCCEEDED",
  "codeHash": "0x...",
  "codeLength": 1234,
  "diagnostics": [],
  "expiresAt": "..."
}
```

不默认永久保存源码。只保存：

- build ID；
- blob；
- code hash；
- language；
- diagnostics；
- toolchain；
- timestamps。

### Service read

```http
GET /api/v1/services/:id
GET /api/v1/services/:id/storage?keyEncoding=utf8&key=counter
```

返回 Controller、ServiceInfo、code/preimage状态和原始 KV。

## 12.3 Auth API

```http
POST /api/v1/auth/challenges
POST /api/v1/auth/sessions
GET  /api/v1/auth/me
DELETE /api/v1/auth/session
```

## 12.4 创建 Service

```http
POST /api/v1/services
Authorization: Bearer <session>
```

请求：

```json
{
  "buildId": "build_xxx"
}
```

返回：

```json
{
  "jobId": "job_xxx",
  "publicId": "public_xxx"
}
```

Job 流程：

```text
QUEUED
→ SUBMITTING_CREATE
→ WAITING_CREATE_FINALITY
→ READING_RECEIPT
→ SUBMITTING_PREIMAGE
→ WAITING_PREIMAGE
→ READY
```

## 12.5 升级 Service

```http
POST /api/v1/services/:id/upgrades
Authorization: Bearer <session>
```

流程：

```text
AUTHENTICATE
→ READ CONTROLLER AT FINALIZED
→ AUTHORIZE OWNER
→ SUBMIT UPGRADE
→ FINALIZE
→ READ RECEIPT
→ SUBMIT PREIMAGE
→ WAIT READY
→ COMPLETED
```

## 12.6 提交 Work

```http
POST /api/v1/services/:id/work
Authorization: Bearer <session>
```

请求：

```json
{
  "payload": {
    "encoding": "utf8|hex|base64",
    "value": "..."
  },
  "extrinsics": [
    {
      "encoding": "utf8|hex|base64",
      "value": "..."
    }
  ],
  "observeKeys": [
    {
      "encoding": "utf8",
      "value": "counter"
    }
  ]
}
```

API：

1. Authenticate；
2. 查询 finalized Controller；
3. Authorize；
4. 获取 finalized context；
5. 获取 finalized ServiceInfo；
6. 构造 Package/Bundle；
7. 记录观察 key 的 before 值；
8. Bundle store put；
9. relayer提交 `submit_work`；
10. 解析 `WorkSubmitted` event，得到 work_id；
11. 跟踪 Work；
12. Candidate；
13. Vote；
14. Accepted；
15. Execution receipt；
16. 查询 after state；
17. 返回结果。

## 12.7 Work Job 状态

只暴露可靠观察状态：

```text
QUEUED
BUILDING_PACKAGE
STORING_BUNDLE
SUBMITTING
AWAITING_CANDIDATE
VOTING
ACCEPTED
ACCUMULATING
COMPLETED
FAILED
```

不确定的内部 Worker步骤不要伪造。

响应示例：

```json
{
  "jobId": "job_xxx",
  "status": "COMPLETED",
  "workId": 42,
  "workPackageHash": "0x...",
  "candidate": {
    "round": 0,
    "reportHash": "0x...",
    "submitter": "5..."
  },
  "refine": {
    "results": [
      {
        "serviceId": 12,
        "status": "OK",
        "output": "0x...",
        "gasUsed": 1234
      }
    ]
  },
  "accumulate": {
    "receiptHash": "0x...",
    "block": 100
  },
  "state": {
    "counter": {
      "before": "0x...",
      "after": "0x..."
    }
  }
}
```

## 12.8 Job 恢复

API 重启后必须：

- 从 SQLite 加载非终态 Job；
- 从链查询真实状态；
- 继续轮询；
- 不重复提交已 finalized 的 extrinsic；
- 使用 request_id、package hash、tx hash 幂等；
- 对 orphaned/reorg 状态回退并重新观察；
- 不生成重复 Work。

---

# 13. Bundle Gateway

Stage 0 不运行 IPFS 节点，但保留 CID/ContentRef。

Playground API 提供 IPFS-compatible read endpoint：

```http
GET /ipfs/:cid
```

Bundle store：

```text
/data/bundles/<content_hash>
```

Put 流程：

1. 计算 Blake2b-256；
2. 构造 CIDv1，raw codec；
3. 原子写临时文件；
4. fsync；
5. rename；
6. 返回 ContentRef；
7. 相同 hash 幂等。

Gateway：

- 验证 CID；
- 校验文件 hash；
- 校验 size；
- Content-Type octet-stream；
- 限制范围和路径穿越；
- 不提供目录列表；
- 支持 Worker内部访问；
- 公网可选只读。

Worker配置：

```toml
[content]
ipfs_gateway = "http://playground:8080/ipfs"
```

必须确认 `IpfsGatewayFetcher` URL拼接规则，并用集成测试证明能读取。

---

# 14. Worker 加固

## 14.1 Task assignment

扩展 `WorkerTaskV1`：

```rust
pub struct WorkerTaskV1 {
    ...
    pub assignment_epoch: u32,
    pub assigned_workers: BoundedVec<WorkerId, ConstU32<8>>,
    pub candidate_producer: WorkerId,
}
```

或者提供按 Worker过滤的 RPC：

```text
minijam_getPendingWorkTasks(worker_id)
```

优先扩展 task，使 Worker可以审计 assignment。

## 14.2 Candidate producer

确定性选择：

```text
candidate_producer = assigned_workers.min()
```

流程：

- Producer执行 Refine并提交 Candidate；
- 其他 assigned Worker等待 Candidate；
- 若 Producer在截止前特定窗口未提交，允许下一个 Worker fallback；
- Stage 0 可以先只实现最小 WorkerId producer，无 fallback，但 E2E必须证明不会多 Worker race。

## 14.3 Vote

每个 assigned Worker：

1. 获取 Candidate canonical report；
2. 独立获取 Bundle；
3. 独立执行 Refine；
4. 比较 report hash；
5. 相同则 Support；
6. 不同或执行失败则 Oppose，对应稳定 reason。

当前 Worker仅自动 Support 的逻辑不得作为最终上线版本。

必须实现：

```text
InvalidRefine
MissingData
ContextMismatch
MalformedOutput
Other
```

## 14.4 Worker配置

Stage0 三个 Worker 配置必须包含：

```toml
[worker]
worker_id = <0/1/2...>
key = "<secret URI or mounted key reference>"
core_index = 0
submit_candidates = true
submit_support_votes = true
```

不要把真实 secret URI 提交到仓库。

建议支持：

```text
MINIJAM_WORKER_KEY_FILE
```

从只读 secret 文件加载，避免命令行暴露。

## 14.5 HTTP

将 Worker 手写 TCP HTTP 替换或补充为成熟客户端：

- reqwest/rustls；
- HTTP/HTTPS；
- timeout；
- response size limit；
- status code；
- chunked encoding；
- DNS；
- TLS校验。

如改动风险过大，Stage0内部 Bundle Gateway可以继续 HTTP，但 Node RPC client仍应使用 jsonrpsee，公开场景不依赖手写 HTTP解析。

## 14.6 Metrics

增加：

```text
minijam_worker_refine_success_total
minijam_worker_refine_failure_total
minijam_worker_candidate_submit_total
minijam_worker_candidate_submit_failure_total
minijam_worker_votes_support_total
minijam_worker_votes_oppose_total
minijam_worker_anchor_mismatch_total
minijam_worker_last_finalized_block
```

---

# 15. Counter Service

正式唯一示例：

## 15.1 输入

Payload：

```text
little-endian i64 increment
```

或 SDK示例允许 UTF-8 decimal，但必须文档固定一种 canonical format。推荐 little-endian i64。

## 15.2 Refine

- 检查 payload 8 字节；
- 输出相同 canonical increment；
- 无 extrinsic 时工作；
- 可提供一个带 extrinsic 的扩展示例测试。

## 15.3 Accumulate

- 读取 WorkResult；
- 错误结果忽略或记录；
- 读取 key `counter`；
- 缺失视为 0；
- checked/saturating 规则明确；
- 写回 little-endian i64；
- yield。

## 15.4 Upgrade 示例

v2 Counter 可：

- 将 key从 `counter` 改为相同 key但增加日志；
- 或将增量乘 2。

升级后提交同一输入，E2E证明使用新 code hash。

---

# 16. Playground 前端

## 16.1 技术

```text
React
TypeScript
Vite
Monaco Editor
@polkadot/extension-dapp
@polkadot/util-crypto
```

浏览器不连接 Node RPC，全部通过 Playground API。

## 16.2 页面布局

单页工作台：

### Header

- Network；
- finalized block；
- Connect Identity；
- 当前 Account；
- API/Compiler状态。

### Code

- Example selector；
- C/C++；
- Monaco；
- Build；
- diagnostics；
- code hash/size。

### Service

- Deploy；
- Service ID；
- Controller；
- code hash；
- preimage状态；
- Upgrade。

### Work

- payload encoding；
- payload；
- extrinsics动态列表；
- observe keys；
- Submit Work。

### Execution

- Job timeline；
- Work ID；
- Candidate；
- Vote；
- Refine output；
- Report hash；
- Accumulate receipt；
- Before/After state；
- raw values。

## 16.3 本地状态

源码默认保存在：

```text
localStorage / IndexedDB
```

不上传云端，除非用户点击 Build。

## 16.4 错误体验

必须区分：

- compile error；
- auth error；
- owner mismatch；
- chain tx reject；
- no workers；
- bundle unavailable；
- refine error；
- candidate rejected；
- out of gas；
- accumulate failure；
- timeout。

不得统一显示“Something went wrong”。

## 16.5 Accessibility / Security

- CSP；
- 不使用 `dangerouslySetInnerHTML` 显示 compiler/log；
- 用户日志按纯文本；
- no secret in localStorage，session优先内存或安全 cookie；
- CORS只允许配置域；
- 键盘可操作；
- 明确 testnet 标识。

---

# 17. Docker 与部署

## 17.1 开发 Compose

新增：

```text
deploy/dev/docker-compose.yml
```

服务：

```text
authority/node
public-rpc
worker-1
worker-2
worker-3
compiler
playground
prometheus
```

可简化 authority数量用于本地开发，但协议 Worker threshold必须满足。

## 17.2 Stage0 Compose

在现有 `deploy/stage0/docker-compose.yml` 中新增：

```text
compiler
playground
reverse-proxy（或部署层外部代理）
```

修复：

- 不存在的 `ipfs-gateway`；
- Worker缺少 key；
- Worker缺少 worker_id；
- Worker未开启 Candidate；
- Worker未开启 Vote；
- Playground relayer account未配置；
- Public RPC CORS只允许 polkadot.js；
- 缺少 health check；
- 缺少 image pin；
- 缺少 Playground/Compiler volume。

## 17.3 Secrets

不得提交：

- authority keys；
- worker keys；
- relayer seed；
- session secret；
- TLS private key。

模板：

```text
deploy/stage0/secrets.example/
```

真实 secrets 通过：

- Docker secrets；
- mounted files；
- systemd credentials；
- secret manager。

## 17.4 镜像

发布：

```text
ghcr.io/archelabs/minijam-node:<tag>
ghcr.io/archelabs/minijam-worker:<tag>
ghcr.io/archelabs/minijam-compiler:<tag>
ghcr.io/archelabs/minijam-playground:<tag>
```

禁止生产使用可变 `stage0` tag 作为唯一 pin；Compose生成 release时替换为 digest。

## 17.5 Health

```text
GET /health/live
GET /health/ready
GET /metrics
```

Playground ready条件：

- SQLite可读写；
- Bundle volume可写；
- Compiler ready；
- Node RPC reachable；
- genesis hash匹配；
- relayer可用。

---

# 18. 测试计划

## 18.1 单元测试

### Auth

- challenge format；
- signature scheme；
- wrong account；
- wrong domain；
- wrong genesis；
- expired；
- replay；
- session expiry；
- revoke；
- owner authorization。

### Builder

- canonical package；
- bundle；
- hash；
- extrinsic；
- limits；
- fixed auth；
- context。

### API

- public build；
- protected routes；
- owner mismatch；
- job transition；
- idempotency；
- database recovery；
- bundle path traversal；
- compiler timeout。

### Runtime/Pallet

- ingress origin；
- explicit controller；
- assigned candidate；
- Stage0 zero economics；
- receipt；
- upgrade controller；
- duplicate system op；
- preimage。

### Worker

- anchor state；
- assigned tasks；
- producer；
- independent vote；
- restart recovery；
- gateway failure；
- malformed bundle；
- candidate submission。

## 18.2 Component integration

- Compiler + SDK + Counter；
- API + Compiler；
- API + Node；
- Worker + Bundle gateway；
- Worker + historical RPC；
- Service0 PVM + Jambda；
- Upgrade adapter + Preimage。

## 18.3 Cross-process E2E

创建 `tests/e2e-stage0/`，至少包含：

1. `network_boot`
2. `public_compile_c`
3. `public_compile_cpp`
4. `auth_login`
5. `create_service`
6. `provide_code_preimage`
7. `submit_counter_work`
8. `worker_refine`
9. `candidate_permission`
10. `independent_votes`
11. `accumulate_counter`
12. `upgrade_service`
13. `owner_denied`
14. `direct_ingress_denied`
15. `bad_bundle`
16. `refine_failure`
17. `out_of_gas`
18. `worker_restart`
19. `playground_restart_job_recovery`
20. `bundle_gateway_interruption`
21. `duplicate_request`
22. `node_restart`
23. `no_runtime_panic`
24. `state_and_receipt_consistency`

## 18.4 正式浏览器 E2E

使用 Playwright：

```text
打开页面
→ Counter模板
→ Build
→ 模拟/真实 extension signer
→ Deploy
→ Service Ready
→ Submit increment
→ Completed
→ Counter changed
→ Build v2
→ Upgrade
→ Submit
→ 新逻辑生效
```

不能只测试 API。

---

# 19. CI

扩展 `.github/workflows/ci.yml`，或拆分 workflow。

## 19.1 必须 Job

```text
rust-format
rust-public-tests
runtime-tests
wasm-check
worker-tests
service0-reproducibility
sdk-compiler-smoke
playground-api-tests
playground-web-check
playwright-component
stage0-e2e-minimal
release-artifacts
container-build
container-scan
```

## 19.2 Service 0 drift

```bash
./scripts/build-system-service.sh
git diff --exit-code -- artifacts/system-service.blob artifacts/system-service.manifest.json
```

## 19.3 Frontend

```bash
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

## 19.4 Compiler

- 构建镜像；
- C smoke；
- C++ smoke；
- deterministic output；
- malicious limits。

## 19.5 E2E

PR：

- 单节点/最小网络；
- 3 Worker；
- Create + Work + Accumulate。

Release candidate：

- 完整 3 authority；
- 3 Worker；
- Upgrade；
- restart；
- gateway failure。

## 19.6 Release artifact

现有 release artifact增加：

- Playground binary/static；
- Compiler manifest；
- SDK；
- Service0 source/blob/manifest；
- OpenAPI spec；
- Compose；
- image digests；
- SBOM；
- hashes；
- Git commits。

---

# 20. 可观测性

## 20.1 Playground metrics

```text
minijam_playground_http_requests_total
minijam_playground_auth_failures_total
minijam_playground_builds_total
minijam_playground_build_failures_total
minijam_playground_jobs_by_status
minijam_playground_chain_rpc_failures_total
minijam_playground_bundle_bytes
minijam_playground_relayer_balance
minijam_playground_last_finalized_block
```

## 20.2 Compiler metrics

```text
compile_total
compile_failure_total
compile_timeout_total
compile_duration_seconds
compile_output_bytes
compile_active
```

## 20.3 Alerts

新增：

- Playground down；
- Compiler down；
- API finalized lag；
- jobs stuck；
- build failure surge；
- relayer unusable；
- bundle gateway error；
- Worker refine failure surge；
- no Candidate；
- no Vote quorum。

---

# 21. 分阶段实施顺序

以下顺序是依赖顺序。Codex不得先做完整 UI，再回头补协议。

---

## M0：基线与术语校准

### 任务

- 新建本文到仓库：
  `docs/minijam-stage0-playground-implementation-spec.md`
- 新建：
  `docs/STAGE0-PLAYGROUND-IMPLEMENTATION-CHECKLIST.md`
- 更新旧 checklist 的过期状态：
  - Worker Refine已存在；
  - Candidate提交已存在；
  - Service0仍为 adapter；
  - E2E仍缺失。
- 增加架构 ADR：
  - `submit_work` 当前语义；
  - API relayer；
  - Controller；
  - Create PVM / Upgrade adapter；
  - default authorization。

### 验收

- 文档与代码事实一致；
- 无未解释的“submit_work 是谁调用”歧义；
- 所有 Stage0 deviation有 ADR。

### 建议提交

```text
docs: define stage0 playground implementation baseline
```

---

## M1：经济参数、Ingress 与 Worker权限

### 任务

- Stage0经济参数归零；
- 加入 Work/System/Preimage ingress origin；
- genesis 配置 relayer；
- `submit_candidate` 验证 assigned Worker；
- task 暴露 assignment/candidate producer；
- 测试 direct unauthorized ingress；
- 测试 non-assigned candidate。

### 验收

```bash
cargo test -p pallet-minijam -p pallet-minijam-workers -p minijam-runtime
cargo check -p minijam-runtime --no-default-features --target wasm32v1-none
```

### 建议提交

```text
feat: enforce stage0 ingress and candidate permissions
refactor: disable stage0 economic charges
```

---

## M2：Controller 与 Upgrade协议

### 任务

- 修改 `SystemCommandV1`；
- 显式 Controller；
- 稳定 Receipt；
- `UpgradeService`；
- Node/runtime API查询 controller；
- CLI同步；
- upgrade adapter；
- preimage lookup；
- tests。

### 验收

- Relayer创建 Service，链上 controller是用户；
- Owner升级成功；
- 非 Owner升级失败；
- Receipt可恢复；
- Runtime无 panic。

### 建议提交

```text
feat: add controller-bound service management commands
feat: implement stage0 service upgrade path
```

---

## M3：真实 Service 0 与 SDK基础

### 任务

- 最小 C SDK；
- Service0 C source；
- compiler toolchain；
- CreateService真实 PVM；
- adapter不再处理 Create；
- reproducibility；
- Counter编译。

### 验收

- 删除 manifest 中 placeholder说明；
- 测试证明 VM执行；
- CreateService E2E；
- C/C++ Blob通过Jambda。

### 建议提交

```text
feat: build service zero as a real pvm service
feat: add minimal c and c++ service sdk
```

---

## M4：Finalized Context 与 WorkPackage Builder

### 任务

- Finalized context RPC；
- At-block state RPC；
- pure builder crate；
- Bundle；
- CID；
- golden tests；
- allow-all auth。

### 验收

- 同输入 deterministic；
- Worker用 anchor状态；
- best变化不改变 report；
- Counter bundle成功。

### 建议提交

```text
feat: add finalized work context rpc
feat: add stage0 work package builder
fix: bind worker refine state reads to package anchor
```

---

## M5：Chain Client 与 Compiler服务

### 任务

- 抽取 chain client；
- signed relayer extrinsics；
- Compiler API；
- container limits；
- build API internal；
- tests。

### 验收

- Compiler C/C++；
- chain client submit/finality/event；
- no duplicated fragile signing logic；
- compiler isolation测试。

### 建议提交

```text
refactor: extract reusable minijam chain client
feat: add isolated service compiler api
```

---

## M6：Playground Auth、Jobs 与 Bundle Gateway

### 任务

- Auth crate；
- SQLite migration；
- API skeleton；
- public build；
- protected create/upgrade/work；
- Owner查询；
- job state；
- restart recovery；
- `/ipfs/:cid`；
- metrics。

### 验收

- Auth replay测试；
- Create、Upgrade、Work API；
- owner mismatch 403；
- API restart恢复；
- Worker读取gateway。

### 建议提交

```text
feat: add signature authentication and service authorization
feat: add playground orchestration api and bundle gateway
```

---

## M7：Worker独立验证与部署配置

### 任务

- Candidate producer；
- assigned task过滤；
- independent vote；
- worker secret file；
- worker metrics；
- Compose修复；
- 3 Worker闭环。

### 验收

- Candidate来自assigned Worker；
- 两个独立Support；
- 错误report产生Oppose；
- recovery；
- 无 race。

### 建议提交

```text
feat: enforce assigned worker candidate production
feat: add independent refine-backed worker voting
fix: make stage0 worker deployment operational
```

---

## M8：Playground Web

### 任务

- Vite/React/Monaco；
- Build；
- wallet login；
- Deploy；
- Upgrade；
- Work；
- status；
- state；
- error；
- responsive/accessibility；
- Playwright。

### 验收

- 无命令行完成主流程；
- 不访问Node RPC；
- session安全；
- 无伪造状态。

### 建议提交

```text
feat: add minijam browser playground
```

---

## M9：完整 Docker 与 E2E

### 任务

- dev Compose；
- stage0 Compose；
- compiler/playground images；
- health；
- E2E harness；
- 全部 scenarios；
- docs。

### 验收

```bash
docker compose -f deploy/dev/docker-compose.yml up -d
./tests/e2e-stage0/run.sh
pnpm --filter playground test:e2e
```

所有关键路径通过。

### 建议提交

```text
test: add cross-process stage0 playground e2e
ops: integrate playground into stage0 deployment
```

---

## M10：CI、Release、Rehearsal 与 Canary

### 任务

- CI jobs；
- release artifacts；
- image digest；
- SBOM；
- checklists；
- rehearsal；
- backup restore；
- 48h canary；
- incident log；
- release evidence。

### 验收

- 三次连续 clean CI；
- rehearsal通过；
- 100 blocks upgrade后正常；
- 48h Canary；
- 无 critical unresolved；
- 所有 release checklist checked；
- Decision改为 approved。

### 建议提交

```text
ci: gate stage0 playground release artifacts
docs: record stage0 release evidence
```

---

# 22. 最终上线门槛

只有全部满足时，Stage 0 才可上线。

## Build

- Rust fmt/test/check；
- Runtime Wasm；
- Web lint/typecheck/test/build；
- Compiler C/C++；
- Service0 reproducibility；
- Containers；
- SBOM；
- 固定版本。

## Protocol

- CreateService真实 Service0 PVM；
- Upgrade controller；
- Preimage；
- WorkPackage标准；
- finalized anchor；
- Worker真实 Refine；
- assigned Candidate；
- independent votes；
- Accumulate；
- no user-input panic；
- zero user economics。

## Product

- public compile；
- signature login；
- create；
- upgrade；
- submit work；
- execution timeline；
- state before/after；
- clear errors；
- browser E2E。

## Security

- Compiler sandbox；
- auth replay防护；
- controller链上真相；
- direct ingress不能绕过；
- Candidate Worker权限；
- secrets不入库；
- HTTPS/CORS/CSP；
- rate limits；
- dependency/container scan。

## Operations

- 3 authorities；
- 3 workers；
- public RPC safe；
- Playground/Compiler health；
- metrics/alerts；
- backup restore；
- upgrade rehearsal；
- release hashes；
- 48h Canary。

## Local developer

```text
git clone
git submodule update --init
docker compose up
打开浏览器
完整完成 Build → Deploy → Work → Accumulate → Upgrade
```

## Release checklist

`docs/STAGE0-RELEASE-CHECKLIST.md` 中所有项目有证据且全部 checked：

```text
Decision: approved
```

---

# 23. 最终浏览器验收脚本

发布负责人必须人工完成一次：

1. 打开公开 Playground。
2. 不登录，编译 C Counter。
3. 不登录，编译 C++ Counter。
4. 连接钱包并签名登录。
5. 部署 C Counter。
6. 确认 Service ID 和 Controller。
7. 确认代码 Preimage ready。
8. 提交 increment = 1。
9. 观察 Work ID。
10. 观察 assigned Worker。
11. 观察 Candidate。
12. 观察 Vote quorum。
13. 观察 Work accepted。
14. 观察 Accumulate receipt。
15. 确认 counter 从 0 变为 1。
16. 用另一个账户登录。
17. 确认不能为该 Service提交 Work。
18. 确认不能升级该 Service。
19. Owner重新登录。
20. 编译 Counter v2。
21. 升级 Service。
22. 确认 code hash变化且 Preimage ready。
23. 再提交 increment = 1。
24. 确认 v2逻辑生效。
25. 重启 Playground API。
26. 确认 Service、Jobs和状态可恢复。
27. 在本地 Docker重复主流程。

任何一步失败，Stage 0 不得发布。

---

# 24. Codex 完成报告格式

Codex 最终必须提供：

```text
1. Final commit SHA
2. Jambda submodule SHA
3. Implemented milestones
4. Remaining deviations
5. CI links
6. Artifact hashes
7. E2E transcript
8. Rehearsal transcript
9. Canary start/end
10. Known risks
11. Release checklist status
12. Exact launch command
13. Exact rollback command
```

禁止仅回复“implemented”或“all tests pass”；必须附实际证据。
