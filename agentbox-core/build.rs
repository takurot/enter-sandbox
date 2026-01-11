use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../runner-wasm/src");
    println!("cargo:rerun-if-changed=../runner-wasm/Cargo.toml");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("runner-wasm.wasm");

    // Only build if we are not in a check/clippy build (optimization)
    // But for simplicity/correctness, let's just try to build or copy.

    // We assume the user has set up the environment (rustup target add wasm32-wasip1)
    let status = Command::new("cargo")
        .args([
            "build",
            "--package",
            "runner-wasm",
            "--target",
            "wasm32-wasip1",
            "--release",
            "--manifest-path",
            "../runner-wasm/Cargo.toml",
        ])
        .status();

    // If cargo build fails (e.g. locking), we might just fail.
    // BUT, recursive cargo build is dangerous.
    // Alternative: The user (TaskFile) builds it.
    // Reviewer suggested: "Use build.rs in agentbox-core to build runner-wasm ... copy to OUT_DIR"

    // Let's allow failure but try to copy the artifact if it exists.
    let target_dir_wasm = Path::new("../runner-wasm/target/wasm32-wasip1/release/runner-wasm.wasm");

    if target_dir_wasm.exists() {
        std::fs::copy(target_dir_wasm, &dest_path).expect("Failed to copy wasm binary");
    } else {
        // Fallback or Error?
        // Let's panic if we can't find it.
        // Or create a dummy empty file to allow check to pass?
        // No, we need it.
        // panic!("runner-wasm.wasm not found. Please run 'cargo build --target wasm32-wasip1 --release -p runner-wasm' first.");

        // Actually, let's try to run the cargo build command. It might work if different target dirs are used?
        // But they share the workspace probably?
        // runner-wasm has its own Cargo.toml.

        // For this environment, let's assume valid pre-build or just try to trigger it.
        // Since we are in agentbox-core, which is part of the workspace...
        // Let's just point to the relative path in the includes! macro if possible,
        // OR rely on the `include_bytes` with absolute/relative path if we can fix the fragility.

        // The reviewer suggested using OUT_DIR.
        // "include_bytes!(concat!(env!("OUT_DIR"), "/runner-wasm.wasm"))"

        // So we MUST put it in OUT_DIR.

        // Let's Try executing cargo build.
        if let Ok(exit_status) = status {
            if !exit_status.success() {
                eprintln!("Cargo build for runner-wasm failed or was skipped.");
            }
        }

        // Retry copy
        if target_dir_wasm.exists() {
            std::fs::copy(target_dir_wasm, &dest_path).expect("Failed to copy wasm binary");
        } else {
            // Create a dummy file if we are just checking?
            // No, bad idea.
            eprintln!("Warning: Wasm binary not found at {:?}", target_dir_wasm);
        }
    }
}
