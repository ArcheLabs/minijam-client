fn main() {
    #[cfg(feature = "std")]
    {
        // nightly-2026-05-02's wasm linker requires host ABI symbols to be
        // explicitly retained as imports. The node executor resolves these
        // standard `ext_*` symbols when the runtime is instantiated.
        std::env::set_var(
            "WASM_BUILD_RUSTFLAGS",
            "-C link-arg=--allow-undefined",
        );
        substrate_wasm_builder::WasmBuilder::build_using_defaults();
    }
}
