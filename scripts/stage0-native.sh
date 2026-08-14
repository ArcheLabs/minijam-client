#!/usr/bin/env bash
set -euo pipefail

repository="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
runtime="${repository}/.local/stage0-native"
data="${runtime}/data"
logs="${runtime}/logs"
run="${runtime}/run"

clang="${MINIJAM_CLANG:-/usr/lib/llvm-20/bin/clang}"
clangxx="${MINIJAM_CLANGXX:-/usr/lib/llvm-20/bin/clang++}"
converter="${repository}/service-toolchain/compiler/polkavm-to-jam/target/release/polkavm-to-jam"

services=(
  node
  compiler-api
  playground-api
  worker-1
  worker-2
  worker-3
  playground-web
)
stop_order=(
  playground-web
  worker-3
  worker-2
  worker-1
  playground-api
  compiler-api
  node
)

pid_file() {
  printf '%s/%s.pid\n' "${run}" "$1"
}

service_pid() {
  local file
  file="$(pid_file "$1")"
  [[ -f "${file}" ]] || return 1

  local pid
  pid="$(<"${file}")"
  if [[ ! "${pid}" =~ ^[1-9][0-9]*$ ]] || ! kill -0 "${pid}" 2>/dev/null; then
    rm -f "${file}"
    return 1
  fi
  printf '%s\n' "${pid}"
}

check_artifacts() {
  local missing=0
  local executable
  for executable in \
    "${repository}/target/release/minijam-node" \
    "${repository}/target/release/minijam-compiler-api" \
    "${repository}/target/release/minijam-playground-api" \
    "${repository}/target/release/minijam-worker" \
    "${converter}"; do
    if [[ ! -x "${executable}" ]]; then
      echo "missing executable: ${executable}" >&2
      missing=1
    fi
  done
  if [[ ! -f "${repository}/apps/playground-web/dist/index.html" ]]; then
    echo "missing Web build: ${repository}/apps/playground-web/dist/index.html" >&2
    missing=1
  fi
  ((missing == 0)) || {
    echo "run ./scripts/stage0-native.sh build first" >&2
    return 1
  }
}

start_service() {
  local service="$1"
  shift
  local file
  file="$(pid_file "${service}")"
  if service_pid "${service}" >/dev/null; then
    echo "${service} is already running" >&2
    return 1
  fi

  setsid "$@" >>"${logs}/${service}.log" 2>&1 &
  local pid=$!
  printf '%s\n' "${pid}" >"${file}"
}

wait_http() {
  local service="$1"
  local url="$2"
  local attempt
  for attempt in $(seq 1 120); do
    if curl -fsS --max-time 2 "${url}" >/dev/null 2>&1; then
      return 0
    fi
    if ! service_pid "${service}" >/dev/null; then
      break
    fi
    sleep 0.5
  done
  echo "${service} did not become ready within 60 seconds" >&2
  tail -n 100 "${logs}/${service}.log" >&2 || true
  return 1
}

wait_node() {
  local attempt
  for attempt in $(seq 1 120); do
    if curl -fsS --max-time 2 \
      -H "content-type: application/json" \
      --data '{"id":1,"jsonrpc":"2.0","method":"system_health","params":[]}' \
      http://127.0.0.1:9944 >/dev/null 2>&1; then
      return 0
    fi
    if ! service_pid node >/dev/null; then
      break
    fi
    sleep 0.5
  done
  echo "node did not become ready within 60 seconds" >&2
  tail -n 100 "${logs}/node.log" >&2 || true
  return 1
}

stop_service() {
  local service="$1"
  local file pid
  file="$(pid_file "${service}")"
  if ! pid="$(service_pid "${service}")"; then
    rm -f "${file}"
    return 0
  fi

  kill -TERM -- "-${pid}" 2>/dev/null || true
  local attempt
  for attempt in $(seq 1 50); do
    kill -0 "${pid}" 2>/dev/null || break
    sleep 0.1
  done
  if kill -0 "${pid}" 2>/dev/null; then
    kill -KILL -- "-${pid}" 2>/dev/null || true
  fi
  rm -f "${file}"
}

down_stack() {
  mkdir -p "${run}"
  local service
  for service in "${stop_order[@]}"; do
    stop_service "${service}"
  done
}

stack_ready() {
  local service
  for service in "${services[@]}"; do
    service_pid "${service}" >/dev/null || return 1
  done
  curl -fsS --max-time 2 http://127.0.0.1:4173 >/dev/null 2>&1 &&
    curl -fsS --max-time 2 http://127.0.0.1:8080/health/ready >/dev/null 2>&1 &&
    curl -fsS --max-time 2 http://127.0.0.1:8081/health/ready >/dev/null 2>&1 &&
    curl -fsS --max-time 2 http://127.0.0.1:8082/health/ready >/dev/null 2>&1 &&
    curl -fsS --max-time 2 http://127.0.0.1:8083/health/ready >/dev/null 2>&1 &&
    curl -fsS --max-time 2 http://127.0.0.1:8084/health/ready >/dev/null 2>&1
}

print_ready() {
  cat <<'EOF'
MiniJAM Stage 0 native stack is ready

Playground:     http://127.0.0.1:4173
Playground API: http://127.0.0.1:8080
Compiler API:   http://127.0.0.1:8081
Node RPC:       http://127.0.0.1:9944

Logs:
  ./scripts/stage0-native.sh logs

Stop:
  ./scripts/stage0-native.sh down
EOF
}

deps() {
  local missing=0
  local command
  for command in git cargo rustup node npm curl setsid; do
    if ! command -v "${command}" >/dev/null 2>&1; then
      echo "missing command: ${command}" >&2
      missing=1
    fi
  done
  for command in "${clang}" "${clangxx}"; do
    if [[ ! -x "${command}" ]]; then
      echo "missing LLVM compiler: ${command}" >&2
      missing=1
    fi
  done
  ((missing == 0)) || {
    cat >&2 <<'EOF'
Install the missing system dependencies with your operating system package
manager, or set MINIJAM_CLANG and MINIJAM_CLANGXX to existing LLVM 20 binaries.
No system packages were installed.
EOF
    return 1
  }

  git -C "${repository}" submodule sync -- external/jambda
  git -C "${repository}" submodule update --init external/jambda
  rustup target add wasm32-unknown-unknown
  rustup target add wasm32v1-none
  npm --prefix "${repository}/apps/playground-web" ci
}

build() {
  (
    cd "${repository}"
    cargo build --locked --release \
      -p minijam-node \
      -p minijam-compiler-api \
      -p minijam-playground-api \
      -p minijam-worker
    cargo build --locked --release \
      --manifest-path service-toolchain/compiler/polkavm-to-jam/Cargo.toml
  )

  local built_converter="${repository}/service-toolchain/compiler/polkavm-to-jam/target/release/minijam-polkavm-to-jam"
  [[ -x "${built_converter}" ]] || {
    echo "missing executable: ${built_converter}" >&2
    return 1
  }
  ln -sfn "minijam-polkavm-to-jam" "${converter}"

  VITE_TEST_WALLET=true npm --prefix "${repository}/apps/playground-web" run build
  check_artifacts
}

up() {
  check_artifacts
  mkdir -p \
    "${data}/node" \
    "${data}/playground/bundles" \
    "${data}/worker-1" \
    "${data}/worker-2" \
    "${data}/worker-3" \
    "${logs}" \
    "${run}"

  if stack_ready; then
    print_ready
    return 0
  fi
  down_stack

  if ! (
    start_service node \
      "${repository}/target/release/minijam-node" \
      --dev \
      --alice \
      --base-path="${data}/node" \
      --rpc-external \
      --rpc-cors=all \
      --rpc-methods=safe
    wait_node || exit 1

    start_service compiler-api env \
      MINIJAM_COMPILER_DIRECT=true \
      MINIJAM_COMPILER_BIND=127.0.0.1:8081 \
      MINIJAM_REPOSITORY="${repository}" \
      MINIJAM_CONVERTER_BIN="${converter}" \
      MINIJAM_CLANG="${clang}" \
      MINIJAM_CLANGXX="${clangxx}" \
      "${repository}/target/release/minijam-compiler-api"
    wait_http compiler-api http://127.0.0.1:8081/health/ready || exit 1

    start_service playground-api env \
      MINIJAM_PLAYGROUND_BIND=127.0.0.1:8080 \
      MINIJAM_PLAYGROUND_DB="${data}/playground/playground.sqlite" \
      MINIJAM_BUNDLE_DIR="${data}/playground/bundles" \
      MINIJAM_COMPILER_URL=http://127.0.0.1:8081 \
      MINIJAM_RPC_URL=ws://127.0.0.1:9944 \
      MINIJAM_GENESIS_HASH=rpc \
      MINIJAM_RELAYER_URI=0x9292929292929292929292929292929292929292929292929292929292929292 \
      "${repository}/target/release/minijam-playground-api"
    wait_http playground-api http://127.0.0.1:8080/health/ready || exit 1

    start_service worker-1 \
      "${repository}/target/release/minijam-worker" \
      --rpc-url http://127.0.0.1:9944 \
      --worker-id 0 \
      --key //Alice \
      --submit-candidates \
      --submit-support-votes \
      --poll-interval-ms 500 \
      --state-db "${data}/worker-1/state.toml" \
      --health-bind 127.0.0.1:8082 \
      --metrics-bind 127.0.0.1:9616 \
      --ipfs-gateway http://127.0.0.1:8080
    wait_http worker-1 http://127.0.0.1:8082/health/ready || exit 1

    start_service worker-2 \
      "${repository}/target/release/minijam-worker" \
      --rpc-url http://127.0.0.1:9944 \
      --worker-id 1 \
      --key //Bob \
      --submit-candidates \
      --submit-support-votes \
      --poll-interval-ms 500 \
      --state-db "${data}/worker-2/state.toml" \
      --health-bind 127.0.0.1:8083 \
      --metrics-bind 127.0.0.1:9617 \
      --ipfs-gateway http://127.0.0.1:8080
    wait_http worker-2 http://127.0.0.1:8083/health/ready || exit 1

    start_service worker-3 \
      "${repository}/target/release/minijam-worker" \
      --rpc-url http://127.0.0.1:9944 \
      --worker-id 2 \
      --key //Charlie \
      --submit-candidates \
      --submit-support-votes \
      --poll-interval-ms 500 \
      --state-db "${data}/worker-3/state.toml" \
      --health-bind 127.0.0.1:8084 \
      --metrics-bind 127.0.0.1:9618 \
      --ipfs-gateway http://127.0.0.1:8080
    wait_http worker-3 http://127.0.0.1:8084/health/ready || exit 1

    start_service playground-web \
      npm --prefix "${repository}/apps/playground-web" run preview -- \
      --host 127.0.0.1 \
      --port 4173 \
      --strictPort
    wait_http playground-web http://127.0.0.1:4173 || exit 1
  ); then
    down_stack
    return 1
  fi

  print_ready
}

show_logs() {
  mkdir -p "${logs}"
  shopt -s nullglob
  local files=("${logs}"/*.log)
  if ((${#files[@]} == 0)); then
    echo "no Stage 0 native logs found in ${logs}" >&2
    return 1
  fi
  tail -n 100 -F "${files[@]}"
}

reset_stack() {
  down_stack
  rm -rf "${data}"
}

case "${1:-}" in
  deps)
    deps
    ;;
  build)
    build
    ;;
  up)
    up
    ;;
  logs)
    show_logs
    ;;
  down)
    down_stack
    ;;
  reset)
    reset_stack
    ;;
  *)
    echo "usage: $0 {deps|build|up|logs|down|reset}" >&2
    exit 2
    ;;
esac
