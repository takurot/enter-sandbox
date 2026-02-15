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

#[derive(Clone, Debug, Eq, PartialEq)]
struct WasiContextView {
    argv: Vec<String>,
    env: Vec<(String, String)>,
    preopen_host_path: String,
    preopen_guest_path: String,
    stdin: String,
    stdout: String,
    stderr: String,
    wall_clock: String,
    monotonic_clock: String,
    secure_random: String,
    insecure_random: String,
    insecure_random_seed: String,
}

impl WasiContextView {
    fn for_profile(profile: ReproProfile, runtime_dir: &Path) -> Self {
        Self {
            argv: Vec::new(),
            env: Vec::new(),
            preopen_host_path: runtime_dir.display().to_string(),
            preopen_guest_path: profile.guest_preopen_path().to_string(),
            stdin: "memory-input-pipe(code via stdin)".to_string(),
            stdout: format!("memory-output-pipe(limit={} bytes)", OUTPUT_LIMIT_BYTES),
            stderr: format!("memory-output-pipe(limit={} bytes)", OUTPUT_LIMIT_BYTES),
            wall_clock: "host-default".to_string(),
            monotonic_clock: "host-default".to_string(),
            secure_random: "wasi-default-secure-rng".to_string(),
            insecure_random: "wasi-default-insecure-rng".to_string(),
            insecure_random_seed: "wasi-default-random-seed".to_string(),
        }
    }
}

pub fn default_code() -> &'static str {
    DEFAULT_REPRO_CODE
}

pub fn context_diff_report() -> String {
    let runtime_dir = cpython_runtime_dir();
    let cli = WasiContextView::for_profile(ReproProfile::Cli, &runtime_dir);
    let sdk = WasiContextView::for_profile(ReproProfile::Sdk, &runtime_dir);

    let rows = vec![
        ("argv", format_args(&cli.argv), format_args(&sdk.argv)),
        ("env", format_env(&cli.env), format_env(&sdk.env)),
        (
            "preopen.host_path",
            cli.preopen_host_path.clone(),
            sdk.preopen_host_path.clone(),
        ),
        (
            "preopen.guest_path",
            cli.preopen_guest_path.clone(),
            sdk.preopen_guest_path.clone(),
        ),
        ("stdio.stdin", cli.stdin.clone(), sdk.stdin.clone()),
        ("stdio.stdout", cli.stdout.clone(), sdk.stdout.clone()),
        ("stdio.stderr", cli.stderr.clone(), sdk.stderr.clone()),
        ("clock.wall", cli.wall_clock.clone(), sdk.wall_clock.clone()),
        (
            "clock.monotonic",
            cli.monotonic_clock.clone(),
            sdk.monotonic_clock.clone(),
        ),
        (
            "random.secure",
            cli.secure_random.clone(),
            sdk.secure_random.clone(),
        ),
        (
            "random.insecure",
            cli.insecure_random.clone(),
            sdk.insecure_random.clone(),
        ),
        (
            "random.insecure_seed",
            cli.insecure_random_seed.clone(),
            sdk.insecure_random_seed.clone(),
        ),
    ];

    let mut report = String::from("field | cli | sdk | status\n--- | --- | --- | ---\n");
    for (field, cli_value, sdk_value) in rows {
        let status = if cli_value == sdk_value {
            "same"
        } else {
            "different"
        };
        report.push_str(&format!(
            "{} | {} | {} | {}\n",
            field, cli_value, sdk_value, status
        ));
    }

    report
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

    let context_view = WasiContextView::for_profile(profile, &runtime_dir);
    let mut builder = WasiCtxBuilder::new();
    configure_wasi_builder(
        &mut builder,
        &context_view,
        &runtime_dir,
        code,
        &stdout_pipe,
        &stderr_pipe,
    )?;

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

fn configure_wasi_builder(
    builder: &mut WasiCtxBuilder,
    context_view: &WasiContextView,
    runtime_dir: &Path,
    code: &str,
    stdout_pipe: &MemoryOutputPipe,
    stderr_pipe: &MemoryOutputPipe,
) -> Result<()> {
    for arg in &context_view.argv {
        builder.arg(arg);
    }
    for (key, value) in &context_view.env {
        builder.env(key, value);
    }

    builder
        .stdin(MemoryInputPipe::new(code.as_bytes().to_vec()))
        .stdout(stdout_pipe.clone())
        .stderr(stderr_pipe.clone());

    builder
        .preopened_dir(
            runtime_dir,
            &context_view.preopen_guest_path,
            DirPerms::all(),
            FilePerms::all(),
        )
        .with_context(|| {
            format!(
                "Failed to preopen CPython runtime at {}",
                runtime_dir.display()
            )
        })?;

    Ok(())
}

fn format_args(args: &[String]) -> String {
    if args.is_empty() {
        return "[]".to_string();
    }

    let values = args
        .iter()
        .map(|value| format!("{:?}", value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{}]", values)
}

fn format_env(env: &[(String, String)]) -> String {
    if env.is_empty() {
        return "[]".to_string();
    }

    let values = env
        .iter()
        .map(|(key, value)| format!("({:?}, {:?})", key, value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{}]", values)
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

    #[test]
    fn test_cpython_wasi_context_diff_report_includes_required_dimensions() {
        let report = context_diff_report();

        assert!(report.contains("argv"));
        assert!(report.contains("env"));
        assert!(report.contains("preopen.guest_path"));
        assert!(report.contains("stdio.stdin"));
        assert!(report.contains("stdio.stdout"));
        assert!(report.contains("stdio.stderr"));
        assert!(report.contains("clock.wall"));
        assert!(report.contains("clock.monotonic"));
        assert!(report.contains("random.secure"));
        assert!(report.contains("random.insecure"));
        assert!(report.contains("random.insecure_seed"));
    }

    #[test]
    fn test_cpython_wasi_context_diff_report_detects_preopen_path_difference() {
        let report = context_diff_report();

        assert!(report.contains("preopen.guest_path"));
        assert!(report.contains(" | / | /sandbox | different"));
    }
}
