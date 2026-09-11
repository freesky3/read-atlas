fn main() {
    tauri_build::build();
    // Windows defaults the main thread to a 1 MiB stack. The debug
    // `generate_handler!` for this app's ~80 commands overflows that
    // (STATUS_STACK_OVERFLOW / 0xc00000fd). 8 MiB matches the common
    // Tauri/Windows workaround and is reserve, not commit.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg=/STACK:8388608");
    }
}
