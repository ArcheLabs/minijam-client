# MiniJAM Quint 形式化规格

本目录包含 MiniJAM MVP 执行网络的可执行
[Quint](https://quint-lang.org) 模型。

MiniJAM 是一个简化的、与 JAM 兼容的执行网络。本模型关注协议层面的安全
属性，包括 worker、work package、candidate report、投票、accumulate
执行、service state、preimage、export、Bulletin-compatible 数据引用以及
原生 bridge effect。

## 这个规格是什么

这个规格把 MiniJAM MVP 建模为一个有界、可执行的状态机。它的目标是明确核
心协议状态转移，并通过随机 trace 和确定性场景检查安全 invariant。

当前模型覆盖：

- worker 注册、active worker snapshot、assignment、投票、奖励和惩罚；
- work package 提交、candidate report 提交、投票阈值、deadline settlement
  和 execution queue；
- 对 malformed vote 的拒绝，包括错误 chain ID、protocol version、candidate
  hash、未分配 worker 和过期 deadline；
- 与 work package、service code hash、parent service root、post service
  root 绑定的抽象 refine 输出；
- 抽象 accumulate 执行，包括有序 delta 校验、namespace allowlist 和原子回滚；
- stale report 拒绝、report replay 拒绝和 service-root continuity；
- service storage、service lookup、preimage 和 export effect；
- 原生 bridge escrow、admin bridge record 和 outbound nonce exactly-once；
- Bulletin-compatible authorization、content retention 和 fault boundary。

## 建模抽象

本模型会抽象掉与这里检查的状态机性质无直接关系的实现细节：

- cryptography、signature、hash function、CID 和 canonical report byte encoding
  用 opaque symbolic value 表示；
- PVM execution、canonical codec behavior 和 storage-root construction 用受约束
  的协议状态转移表示；
- Worker client networking 和真实 report corpus execution 用协议层 report/vote
  action 表示。

hash、signature、report bytes 和 CID 等 opaque value 在模型中用小型符号值表
示。本模型检查相等性、排序、状态转移规则和安全属性，不检查字节级编码正确
性。

## 文件结构

| 文件 | 作用 |
|---|---|
| [`types.qnt`](./types.qnt) | 基础 opaque type、有界模型常量、namespace、status 和 error enum。 |
| [`messages.qnt`](./messages.qnt) | Work package、report envelope、worker vote、state delta、execution output 和 bridge effect。 |
| [`state.qnt`](./state.qnt) | `MiniJamState`：worker、work、candidate、execution、service、preimage、export、bridge 和 Bulletin state。 |
| [`state_vars.qnt`](./state_vars.qnt) | `main.qnt` 和 `invariants.qnt` 共享的 Quint 变量。 |
| [`workers.qnt`](./workers.qnt) | Worker 注册抽象、active snapshot、确定性 assignment、vote accounting、reward 和 slash。 |
| [`work.qnt`](./work.qnt) | Work 提交、candidate 提交、投票、equivocation 处理和 deadline settlement。 |
| [`refine.qnt`](./refine.qnt) | 从 work package 和 service context 构造抽象 refine report。 |
| [`accumulate.qnt`](./accumulate.qnt) | 抽象 MiniJAM executive、delta validation、root check、replay check 和原子 commit/rollback。 |
| [`service.qnt`](./service.qnt) | Service storage 和 service lookup namespace 更新。 |
| [`preimage.qnt`](./preimage.qnt) | Preimage namespace 更新。 |
| [`export.qnt`](./export.qnt) | Service output 和 export 记录。 |
| [`bridge.qnt`](./bridge.qnt) | 原生 bridge escrow、admin bridge record 和 outbound nonce exactly-once。 |
| [`bulletin.qnt`](./bulletin.qnt) | Bulletin-compatible authorization、content storage 和 fault abstraction。 |
| [`invariants.qnt`](./invariants.qnt) | Nullary invariant 目录和组合 invariant `invariants`。 |
| [`main.qnt`](./main.qnt) | 顶层 `init`、`step` 和 nondeterministic trace 生成。 |
| [`tests.qnt`](./tests.qnt) | 确定性场景测试。 |

## 关键 invariants

Invariant 目录位于 [`invariants.qnt`](./invariants.qnt)。重要属性包括：

- `assignment_invariant`：assignment 包含预期数量的已知 active worker。
- `vote_threshold_invariant`：accepted/rejected work 必须满足对应 support/oppose
  阈值。
- `single_vote_or_equivocation_invariant`：冲突 worker vote 会被记录为
  equivocation。
- `execution_requires_acceptance_invariant`：只有 accepted work 可以进入执行队列。
- `paused_or_quarantined_not_executed_invariant`：paused 或 quarantined execution
  item 不会被执行。
- `delta_allowlist_invariant`：成功执行只能提交允许的 protocol namespace。
- `delta_order_unique_invariant`：成功执行提交的 delta 必须有序且无重复 key。
- `execution_atomicity_invariant`：fatal execution error 不会半提交 state、bridge
  record 或 report application state。
- `service_root_continuity_invariant`：applied report 根据 parent/post root 推进
  service root。
- `accepted_report_parent_continuity_invariant`：成功执行会记录 report，并更新对应
  service root。
- `malformed_vote_rejected_invariant`：malformed vote 不改变状态。
- `bridge_nonce_invariant`：bridge inbound nonce 单调递增，outbound nonce 至多消
  费一次。
- `bridge_escrow_invariant`：bridge escrow balance 不会为负。
- `reward_slash_conservation_invariant`：抽象 reward pool 和 worker stake balance
  不会为负。
- `preimage_consistency_invariant`：service lookup reference 需要已知 preimage
  或显式 Bulletin fault boundary。
- `export_determinism_invariant`：每个 applied report 都会记录其 metadata 声明的
  export root。

## 确定性场景

[`tests.qnt`](./tests.qnt) 包含以下代表性协议流程：

- happy-path report execution；
- candidate 被 oppose threshold 拒绝；
- worker 数量不足；
- equivocation slash 和 suspension；
- invalid delta rollback；
- bridge inbound/admin-record atomicity；
- stale report rejection；
- report replay rejection；
- malformed vote rejection；
- deadline settlement 和 absence slash；
- service lookup 加 preimage update；
- Bulletin fault isolation。

## 运行规格

从仓库根目录开始：

```sh
cd designs/minijam/quint
```

如果已全局安装 `quint`：

```sh
quint typecheck main.qnt
quint typecheck tests.qnt
quint test --backend=typescript tests.qnt
quint run --backend=typescript main.qnt --invariant=invariants --max-samples=50 --max-steps=20
```

如果没有全局安装：

```sh
npx @informalsystems/quint typecheck main.qnt
npx @informalsystems/quint typecheck tests.qnt
npx @informalsystems/quint test --backend=typescript tests.qnt
npx @informalsystems/quint run --backend=typescript main.qnt --invariant=invariants --max-samples=50 --max-steps=20
```

当可用的 symbolic backend 兼容时，也可以使用 `quint verify main.qnt`。上面的命
令使用 TypeScript backend，因为它在常见开发环境中更便携。

## 当前验证状态

准备此版本时，以下命令已通过 `@informalsystems/quint` 0.32.0 验证：

```sh
npx --yes @informalsystems/quint@0.32.0 typecheck main.qnt
npx --yes @informalsystems/quint@0.32.0 typecheck tests.qnt
npx --yes @informalsystems/quint@0.32.0 test --backend=typescript tests.qnt
npx --yes @informalsystems/quint@0.32.0 run --backend=typescript main.qnt --invariant=invariants --max-samples=50 --max-steps=20 --verbosity=0
```

确定性测试套件目前包含 12 个场景。

## 维护说明

- 随着 MiniJAM protocol type 和 pallet 行为演进，保持本规格同步。
- 对安全关键行为，优先强化 invariant，而不是只增加场景测试。
- 公开文档中不要包含本地文件系统路径或非公开实现细节。
- 如果模型变更有意抽象掉某些实现行为，应在本 README 中明确说明该抽象。
