# 仅供 Maintainer / Operator：Stage 0 Docker 部署

本文不是公开用户 Quickstart。普通用户必须使用可下载的本地镜像 bundle；MiniJAM 不会分发官方 Authority、Worker、Relayer、Sudo 或 Faucet 凭据。

完整 MiniJAM Stack 当前无法仅使用公开源码独立构建，因为 Node、Runtime、Worker 和执行路径依赖固定的私有 Jambda revision。

当前支持的分发和部署方式是已经发布的 Stage 0 Docker release。目标主机不需要安装 Rust、Cargo、LLVM、Node.js，也不需要访问 Jambda 源码。

:::warning 需要 Operator 凭据

Stage 0 release stack 不是无需凭据的本地开发链。Authority keystore、三个 Worker seed 和 Playground Relayer URI 必须与 release chain spec 中的公开身份匹配。任意替换密钥会导致 Node、Worker 或 Playground 保持不健康。

启动 Stack 前，请从 MiniJAM maintainer 获取匹配的 release bundle 和运行凭据。

:::

## Stack 包含什么

release Compose Stack 会启动一个 MiniJAM Node、一个 Compiler API、一个 Playground API 和 bundle gateway、三个使用不同身份的 Worker，以及一个 Playground Web 前端。默认只有 Playground Web 发布到主机，其他服务和健康端点保留在私有 Compose 网络内。

## 要求

安装 Docker Engine、Docker Compose plugin、Git 和 `sha256sum`：

```bash
docker --version
docker compose version
git --version
sha256sum --version
```

Docker 是当前完整 Stack 唯一支持的路径。不要使用公开源码仓库构建完整 Node、Runtime、Worker 或执行 Stack。

## 1. 检出匹配的 release

使用与 release bundle 对应的精确 release tag。此处使用占位符，因为本文档不发布具体 release tag：

```bash
git clone --branch <RELEASE_TAG> --depth 1 \
  https://github.com/ArcheLabs/minijam-client.git
cd minijam-client
```

不要使用一个 release 的 artifact 部署另一个 release 的 `main`。

## 2. 获取完整 release bundle

从同一个成功的 Stage 0 release 获取：

```text
release-manifest.json
stage0-raw.json
SHA256SUMS
```

manifest 是 MiniJAM commit、固定的 Jambda commit、genesis hash、公开身份和五个镜像 digest 的事实来源。不要混用不同 release 的 manifest、chain spec、镜像 digest 或数据库。

```bash
cp /path/to/release-bundle/stage0-raw.json chain-specs/stage0-raw.json
cd /path/to/release-bundle
sha256sum -c SHA256SUMS
cd /path/to/minijam-client
```

raw chain spec、genesis hash、五个镜像 digest 和凭据必须全部来自同一个 release bundle。

## 3. 配置未跟踪的环境文件

```bash
cp deploy/stage0/.env.example deploy/stage0/.env
```

替换 `deploy/stage0/.env` 中的所有占位符。五个镜像值必须使用从 `release-manifest.json` 复制的不可变 digest 引用：

```dotenv
MINIJAM_NODE_IMAGE=ghcr.io/archelabs/minijam-node@sha256:<digest>
MINIJAM_WORKER_IMAGE=ghcr.io/archelabs/minijam-worker@sha256:<digest>
MINIJAM_COMPILER_IMAGE=ghcr.io/archelabs/minijam-compiler-api@sha256:<digest>
MINIJAM_PLAYGROUND_API_IMAGE=ghcr.io/archelabs/minijam-playground-api@sha256:<digest>
MINIJAM_PLAYGROUND_WEB_IMAGE=ghcr.io/archelabs/minijam-playground-web@sha256:<digest>
```

设置匹配的链配置和 release-specific Relayer URI：

```dotenv
MINIJAM_CHAIN_SPEC_PATH=./chain-specs/stage0-raw.json
MINIJAM_GENESIS_HASH=<genesis-hash-from-release-manifest>
MINIJAM_RELAYER_URI=<release-specific-relayer-uri>
MINIJAM_WEB_BIND=127.0.0.1
MINIJAM_WEB_PORT=4173
```

保持 `.env` 未跟踪。不要将 digest 引用替换为可变 tag。

## 4. 配置匹配的运行凭据

使用同一 release 提供的凭据创建：

```text
deploy/stage0/secrets/node-keystore.tar.gz
deploy/stage0/secrets/worker-1.seed
deploy/stage0/secrets/worker-2.seed
deploy/stage0/secrets/worker-3.seed
```

Node archive 必须包含与 chain spec Authority 身份匹配的 Aura 和 GRANDPA keystore 文件。每个 Worker seed 必须匹配其注册的 Worker session key。Playground Relayer URI 也必须匹配该 release 部署。没有匹配凭据的用户，应在公共 Playground 可用时使用托管 Playground，而不是自行生成任意 seed。

在 `.env` 中设置 secret 路径：

```dotenv
NODE_KEY_OR_SEED_PATH=./deploy/stage0/secrets/node-keystore.tar.gz
WORKER_1_SEED_PATH=./deploy/stage0/secrets/worker-1.seed
WORKER_2_SEED_PATH=./deploy/stage0/secrets/worker-2.seed
WORKER_3_SEED_PATH=./deploy/stage0/secrets/worker-3.seed
```

应用严格权限：

```bash
sudo chown 10001:10001 deploy/stage0/secrets/node-keystore.tar.gz \
  deploy/stage0/secrets/worker-1.seed deploy/stage0/secrets/worker-2.seed \
  deploy/stage0/secrets/worker-3.seed
sudo chmod 0400 deploy/stage0/secrets/node-keystore.tar.gz \
  deploy/stage0/secrets/worker-1.seed deploy/stage0/secrets/worker-2.seed \
  deploy/stage0/secrets/worker-3.seed
```

不要将凭据提交或粘贴到 Compose、源码、日志、截图或支持工单中。

## 5. 校验并拉取 release

拉取镜像前先解析所有环境变量：

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml config >/dev/null
```

如果存在缺失变量、空 Relayer URI、占位符或 tag 镜像 digest、无效镜像引用或缺失 secret 文件，不要继续。

拉取五个不可变镜像：

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml pull
```

## 6. 启动并检查健康状态

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml up -d
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml ps
```

等待 `node`、`compiler-api`、`playground-api`、`worker-1`、`worker-2`、`worker-3` 和 `playground-web` 全部健康。需要时查看日志：

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml logs --tail=200 node playground-api \
  worker-1 worker-2 worker-3
```

## 7. 打开 Playground

Docker 部署健康后，默认本地地址为 [http://127.0.0.1:4173](http://127.0.0.1:4173)。公共托管 Playground 上线后，应单独列出其地址；本文档不编造公共地址。

浏览器只能访问 Playground Web。不要暴露 Node RPC、Compiler API、Playground API、Worker health endpoint 或 metrics 端口。公共主机应使用 Operator 管理的 HTTPS reverse proxy，并且只发布 Web 端口。

## 8. 停止、重启或重置

停止但不删除状态：

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml down
```

使用同一 release 再次启动：

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml up -d
```

重置 Stage 0 并删除 Node、Playground、bundle 和 Worker 状态：

```bash
docker compose --env-file deploy/stage0/.env \
  -f compose.stage0.yml down --volumes --remove-orphans
```
