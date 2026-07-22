# MiniJAM Quint 形式化规格

本目录包含 MiniJAM MVP 的可执行 [Quint](https://quint-lang.org) 模型。

该模型把 MiniJAM Runtime 执行视为虚拟 JAM 区块导入。Runtime 不解释报告
delta、service root、service output、preimage 或 bridge effect 为 MiniJAM
自定义状态；通过 Worker 投票确认的 canonical JAM `WorkReport` 投影和
canonical preimage 会进入抽象 Executive，由 Executive 执行保留的标准 JAM
STF 阶段，并返回不透明 JAM 状态承诺和导入收据。

## 范围

当前模型覆盖：

- Worker 注册、active snapshot、确定性 assignment、投票、奖励和惩罚；
- Work 提交、候选报告提交、投票阈值、deadline settlement 和执行队列；
- 错误 chain ID、protocol version、candidate hash、未分配 Worker 和过期
  deadline 的 malformed vote 拒绝；
- canonical WorkReport 投影导入虚拟 JAM 区块；
- 保留 STF 顺序：last history、空 tickets 的 Safrole、preimage prepare、
  Accumulate、current history、authorization、statistics、preimage apply；
- 被移除 STF 的边界：disputes、assurances、reports/guarantees 永远不运行；
- Runtime import 语义：报告可以被标记为 `Imported`，但不要求同块完成
  Accumulate；
- 原生 bridge 作为 Runtime-only 账本，通过观察标准 JAM state key 工作，不接收
  Executive bridge effect；
- Bulletin-compatible 授权、内容保留和故障边界。

## 文件

| 文件 | 作用 |
|---|---|
| [`types.qnt`](./types.qnt) | Opaque 基础类型、有界常量、Work 状态和 Executive 阶段枚举。 |
| [`messages.qnt`](./messages.qnt) | Work package、canonical WorkReport/preimage 投影、Worker vote 和执行 I/O。 |
| [`jam_projection.qnt`](./jam_projection.qnt) | 被保留 STF 触及的标准 JAM 状态抽象投影。 |
| [`executive.qnt`](./executive.qnt) | 抽象虚拟 JAM 区块 executor 和 preimage queue admission。 |
| [`state.qnt`](./state.qnt) | Runtime 状态、不透明 `JamProjection`、bridge 和 Bulletin 状态。 |
| [`state_vars.qnt`](./state_vars.qnt) | 共享变量和 transition ghost record。 |
| [`workers.qnt`](./workers.qnt) | Worker assignment、vote accounting、reward 和 slash。 |
| [`work.qnt`](./work.qnt) | Work/candidate 生命周期和 execution queue admission。 |
| [`refine.qnt`](./refine.qnt) | canonical WorkReport 投影构造。 |
| [`bridge.qnt`](./bridge.qnt) | Runtime bridge ledger 抽象。 |
| [`bulletin.qnt`](./bulletin.qnt) | Bulletin-compatible storage/fault 抽象。 |
| [`invariants.qnt`](./invariants.qnt) | Invariant 目录。 |
| [`main.qnt`](./main.qnt) | 顶层状态转移系统。 |
| [`tests.qnt`](./tests.qnt) | 确定性场景测试。 |

## 关键 Invariant

- `virtual_block_atomicity_invariant`
- `retained_stf_order_invariant`
- `tickets_always_empty_invariant`
- `reports_bypass_assurance_invariant`
- `reports_bypass_guarantee_invariant`
- `history_uses_imported_reports_invariant`
- `authorization_uses_imported_reports_invariant`
- `preimage_snapshot_invariant`
- `runtime_import_not_accumulation_invariant`
- `removed_stfs_never_run_invariant`

## 运行

```sh
cd designs/minijam/quint
npx --yes @informalsystems/quint@0.32.0 typecheck main.qnt
npx --yes @informalsystems/quint@0.32.0 typecheck tests.qnt
npx --yes @informalsystems/quint@0.32.0 test --backend=typescript tests.qnt
npx --yes @informalsystems/quint@0.32.0 run --backend=typescript main.qnt --invariant=invariants --max-samples=100 --max-steps=30 --verbosity=0
```

确定性测试套件当前包含 12 个场景。
