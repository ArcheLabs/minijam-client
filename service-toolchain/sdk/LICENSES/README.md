# Licensing

The MiniJAM SDK sources in this directory are independently implemented and
licensed under Apache-2.0. No JamBrains or Parity SDK source was copied.

The build toolchain consumes external binaries under their own licenses:

- PolkaVM / polkatool: Apache-2.0 OR GPL-3.0-only WITH Classpath-exception-2.0.
- LLVM/Clang: Apache-2.0 WITH LLVM-exception.

Release images must include the exact upstream license files corresponding to
the commits pinned in `service-toolchain/compiler/toolchain.lock`.
