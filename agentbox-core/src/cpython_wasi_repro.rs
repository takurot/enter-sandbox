use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use wasmtime::{Engine, Linker, Module, Store};
use wasmtime_wasi::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

const DEFAULT_REPRO_CODE: &str = "import json\nprint('ok')\n";
const OUTPUT_LIMIT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReproProfile {
    Cli,
    Sdk,
}

impl ReproProfile {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "cli" => Ok(Self::Cli),
            "sdk" => Ok(Self::Sdk),
            _ => bail!("profile must be either 'cli' or 'sdk'"),
        }
    }

    fn guest_preopen_path(self) -> &'static str {
        match self {
            Self::Cli => "/",
            Self::Sdk => "/sandbox",
        }
    }
}

#[derive(Debug)]
pub struct ReproRun {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

struct ReproSession {
    wasi_ctx: WasiP1Ctx,
    stdout_pipe: MemoryOutputPipe,
    stderr_pipe: MemoryOutputPipe,
}

pub fn default_code() -> &'static str {
    DEFAULT_REPRO_CODE
}

pub fn run(profile: ReproProfile, code: &str) -> Result<ReproRun> {
    let runtime_dir = cpython_runtime_dir();
    let wasm_path = runtime_dir.join("python.wasm");
    ensure_runtime_available(&runtime_dir, &wasm_path)?;

    let engine = Engine::default();
    let module = Module::from_file(&engine, &wasm_path).with_context(|| {
        format!(
            "Failed to load CPython WASI module: {}",
            wasm_path.display()
        )
    })?;

    let mut linker = Linker::new(&engine);
    preview1::add_to_linker_sync(&mut linker, |session: &mut ReproSession| {
        &mut session.wasi_ctx
    })
    .context("Failed to link WASI preview1")?;

    let stdout_pipe = MemoryOutputPipe::new(OUTPUT_LIMIT_BYTES);
    let stderr_pipe = MemoryOutputPipe::new(OUTPUT_LIMIT_BYTES);

    let mut builder = WasiCtxBuilder::new();
    builder
        .stdin(MemoryInputPipe::new(code.as_bytes().to_vec()))
        .stdout(stdout_pipe.clone())
        .stderr(stderr_pipe.clone())
        .preopened_dir(
            &runtime_dir,
            profile.guest_preopen_path(),
            DirPerms::all(),
            FilePerms::all(),
        )
        .with_context(|| {
            format!(
                "Failed to preopen CPython runtime at {}",
                runtime_dir.display()
            )
        })?;

    let session = ReproSession {
        wasi_ctx: builder.build_p1(),
        stdout_pipe,
        stderr_pipe,
    };
    let mut store = Store::new(&engine, session);

    let instance = linker
        .instantiate(&mut store, &module)
        .context("Failed to instantiate CPython WASI module")?;
    let start = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .context("Failed to resolve _start")?;

    let call_result = start.call(&mut store, ());
    let (success, error) = match call_result {
        Ok(()) => (true, None),
        Err(err) => (false, Some(err.to_string())),
    };

    let stdout = String::from_utf8_lossy(store.data().stdout_pipe.contents().as_ref()).to_string();
    let stderr = String::from_utf8_lossy(store.data().stderr_pipe.contents().as_ref()).to_string();

    Ok(ReproRun {
        success,
        stdout,
        stderr,
        error,
    })
}

fn cpython_runtime_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/cpython-wasi/runtime")
}

fn ensure_runtime_available(runtime_dir: &Path, wasm_path: &Path) -> Result<()> {
    if !runtime_dir.is_dir() {
        bail!(
            "CPython WASI runtime directory not found: {}. Run `python3 scripts/prepare_cpython_wasi_assets.py` from the repository root.",
            runtime_dir.display()
        );
    }
    if !wasm_path.is_file() {
        bail!(
            "CPython WASI module not found: {}. Run `python3 scripts/prepare_cpython_wasi_assets.py` from the repository root.",
            wasm_path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format_details(run: &ReproRun) -> String {
        format!(
            "stdout:\n{}\n---\nstderr:\n{}\n---\nerror:\n{}",
            run.stdout,
            run.stderr,
            run.error.as_deref().unwrap_or("<none>")
        )
    }

    #[test]
    fn test_cpython_wasi_cli_profile_succeeds() {
        let result = run(ReproProfile::Cli, default_code()).unwrap();
        assert!(result.success, "{}", format_details(&result));
        assert!(result.stdout.lines().any(|line| line.trim() == "ok"));
    }

    #[test]
    fn test_cpython_wasi_sdk_profile_fails_with_missing_encodings() {
        let result = run(ReproProfile::Sdk, default_code()).unwrap();
        assert!(!result.success, "{}", format_details(&result));
        assert!(
            result.stderr.contains("No module named 'encodings'"),
            "{}",
            format_details(&result)
        );
    }

    #[test]
    fn test_cpython_wasi_repro_matches_cli_success_sdk_failure_pattern() {
        let cli = run(ReproProfile::Cli, default_code()).unwrap();
        let sdk = run(ReproProfile::Sdk, default_code()).unwrap();

        assert!(cli.success, "{}", format_details(&cli));
        assert!(!sdk.success, "{}", format_details(&sdk));
    }
}
