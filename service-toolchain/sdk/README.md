# MiniJAM Stage 0 Service SDK

This Apache-2.0 SDK is an independent wrapper over the public JAM host-call ABI.
It supports allocation-free, single-file C and restricted C++ services.

The ABI is pinned by `service-toolchain/compiler/toolchain.lock`. Production
builds must use the pinned PolkaVM target; defining `MINIJAM_HOST_TEST` is only
for native compile/link smoke tests with `tests/host_stub.c`.

Payload bytes use Refine `FETCH` mode 13. Stage 0 extrinsics use external-data
mode 4. Accumulate results use modes 14/15. Storage uses the standard READ and
WRITE calls, and completion uses YIELD.

Run `./scripts/check-service-sdk.sh` to compile the counter example natively
and, when LLVM 20 and Cargo are available, through the pinned
RISC-V ELF → PolkaVM → JAM blob pipeline. Set `MINIJAM_CLANG` to an alternate
LLVM 20 `clang` path when it is not installed under `/usr/lib/llvm-20/bin`.
