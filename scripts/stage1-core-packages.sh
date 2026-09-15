#!/usr/bin/env bash

# Keep this list explicit. The current root workspace has one LLVM/libclang
# owner, minijam-node. Native production-role tests run in the Docker builder;
# this list is the LLVM-free source-validation surface.
STAGE1_CORE_PACKAGES=(
  minijam-protocol
  minijam-rpc-runtime-api
  minijam-jamcore-api
  minijam-jamcore-mock
  minijam-work-package-builder
  minijam-chain-client
  minijam-formal-rpc
  minijam-worker
  minijam-runtime
  pallet-minijam
  minijam-bridge-engine
  minijam-state-adapter
  minijam-cli
  minijam-pvm-executor
  minijam-bulletin-api
  minijam-bulletin-simulator
  pallet-minijam-bridge
)
