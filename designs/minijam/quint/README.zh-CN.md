# MiniJAM 直接 Refine Quint 模型

本目录包含单 Worker 直接 Refine 架构的可执行模型。

模型只保留清晰的边界：Formal RPC 为交易生成稳定的 transaction ID，按
`(service_id, service_code_hash)` 在固定上下文下批量构造 WorkPackage，并为
每笔交易生成一个 WorkItem。直接报告只接受配置的 Worker origin。Package
状态为 `Pending`、`Imported`、`Failed`；交易状态覆盖 queued、packaged、
refining、reported 和终态。

旧的 Worker market、WorkRecord、assignment、candidate 和 voting 模型以及
对应 runtime 链路已经删除。

## 运行

```sh
cd designs/minijam/quint
npx --yes @informalsystems/quint@0.32.0 typecheck direct_ingress.qnt
npx --yes @informalsystems/quint@0.32.0 test --backend=typescript direct_ingress.qnt
```
