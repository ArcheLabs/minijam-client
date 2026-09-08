# Stage-1 Docker 部署

Stage-1 是当前支持的 MiniJAM 部署。发布单元是一组不可变镜像 digest，以及从
精确 node 镜像生成的 chain specification。仓库不再提供独立的本地、Stage-0
或 Web 部署栈。

## 组件

compact 和 split 配置包含相同的三个角色：

- `node`：validator 和安全 JSON-RPC 端点；
- `worker`：使用独立签名密钥和状态 volume 的 Worker daemon；
- `formal-rpc`：应用无关的 Work 与 bundle gateway，持有 Work-ingress relayer
  密钥和 bundle volume。

可选 Service compiler 位于 `deploy/compiler`，不属于 Stage-1 runtime 网络。

## 准备发布产物

从 Stage-1 发布产物获取三个镜像 digest，再从精确 node 镜像生成匹配的 chain
spec：

```bash
MINIJAM_NODE_IMAGE=ghcr.io/archelabs/minijam-node@sha256:<digest> \
MINIJAM_STAGE1_CHAIN_SPEC_DIR=./chain-specs \
./scripts/export-stage1-chain-specs-image.sh
```

生成文件为：

```text
chain-specs/stage1.json
chain-specs/stage1-raw.json
```

生成文件必须与 release manifest 中的 hash 一致，不得与其他 node 镜像混用，且
不得提交回仓库。

## Compact 部署

设置镜像、chain spec 和 secret 文件路径，然后验证并启动：

```bash
export MINIJAM_NODE_IMAGE=ghcr.io/archelabs/minijam-node@sha256:<digest>
export MINIJAM_WORKER_IMAGE=ghcr.io/archelabs/minijam-worker@sha256:<digest>
export MINIJAM_FORMAL_RPC_IMAGE=ghcr.io/archelabs/minijam-formal-rpc@sha256:<digest>
export MINIJAM_STAGE1_CHAIN_SPEC_FILE=./chain-specs/stage1.json
export MINIJAM_WORKER_KEY_FILE=/secure/path/worker.seed
export MINIJAM_FORMAL_RPC_RELAYER_KEY_FILE=/secure/path/ingress-relayer.seed

docker compose -f deploy/stage1/compose.compact.yml config
docker compose -f deploy/stage1/compose.compact.yml up -d
docker compose -f deploy/stage1/compose.compact.yml ps
```

默认只在 loopback 发布 operator 选择的 Node RPC 和 Formal RPC 端口。不要将
Worker health 或 metrics 端点暴露到公网。

## Split 部署

在参与部署的主机上创建共享的外部 `chain` 网络。使用相同的镜像 digest、生成
的 chain spec 和 secret 约定运行 `deploy/stage1/compose.split.yml`，并将
`MINIJAM_RPC_URL` 设置为 Formal RPC 主机可访问的私有 Node RPC 地址。

```bash
docker network create chain
docker compose -f deploy/stage1/compose.split.yml config
docker compose -f deploy/stage1/compose.split.yml up -d
```

Worker 和 Formal RPC 的签名密钥职责分离。不得将密钥复制到镜像、提交到仓库，
或在公网复用开发 seed。

## 验证和停止

发布 gate 会验证 Compose、候选镜像 smoke、Node 重启恢复、Formal RPC readiness、
Worker readiness 和产物 secret hygiene。运维部署可使用：

```bash
docker compose -f deploy/stage1/compose.compact.yml ps
docker compose -f deploy/stage1/compose.compact.yml logs --tail=200 node worker formal-rpc
docker compose -f deploy/stage1/compose.compact.yml down
```
