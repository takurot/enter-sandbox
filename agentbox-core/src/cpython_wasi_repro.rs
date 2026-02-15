use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use wasmtime::{Config, Engine, Linker, Module, Store, WasmBacktrace};
use wasmtime_wasi::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

const DEFAULT_REPRO_CODE: &str = "import json\nprint('ok')\n";
const OUTPUT_LIMIT_BYTES: usize = 1024 * 1024;
const RUNTIME_HOST_PATH_LABEL: &str = "<assets/cpython-wasi/runtime>";
const TRACE_CAPTURE_MARKER: &str = "trace.capture=wasm-backtrace-v1";
const STATUS_SAME: &str = "same";
const STATUS_DIFFERENT: &str = "different";
const STATUS_SAME_SOURCE: &str = "same-source";
const STATUS_RUNTIME_GENERATED: &str = "runtime-generated";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReproProfile {
    Cli,
    Sdk,
    SdkLegacy,
}

impl ReproProfile {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "cli" => Ok(Self::Cli),
            "sdk" => Ok(Self::Sdk),
            "sdk-legacy" => Ok(Self::SdkLegacy),
            _ => bail!("profile must be one of: 'cli', 'sdk', 'sdk-legacy'"),
        }
    }

    fn guest_preopen_path(self) -> &'static str {
        match self {
            Self::Cli => "/",
            // P1-074: Align SDK guest preopen path with CLI to allow CPython to
            // resolve stdlib modules such as `encodings` from `/lib/python3.13`.
            Self::Sdk => "/",
            Self::SdkLegacy => "/sandbox",
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
struct DiffRow {
    field: &'static str,
    cli: String,
    sdk: String,
    status: &'static str,
    note: &'static str,
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
}

impl WasiContextView {
    fn for_profile(profile: ReproProfile) -> Self {
        Self {
            argv: Vec::new(),
            env: Vec::new(),
            preopen_host_path: RUNTIME_HOST_PATH_LABEL.to_string(),
            preopen_guest_path: profile.guest_preopen_path().to_string(),
            stdin: "memory-input-pipe(code via stdin)".to_string(),
            stdout: format!("memory-output-pipe(limit={} bytes)", OUTPUT_LIMIT_BYTES),
            stderr: format!("memory-output-pipe(limit={} bytes)", OUTPUT_LIMIT_BYTES),
            wall_clock: "host-default".to_string(),
            monotonic_clock: "host-default".to_string(),
            secure_random: "wasi-default-secure-rng".to_string(),
            insecure_random: "wasi-default-insecure-rng".to_string(),
        }
    }
}

pub fn default_code() -> &'static str {
    DEFAULT_REPRO_CODE
}

fn context_diff_rows() -> Vec<DiffRow> {
    let cli = WasiContextView::for_profile(ReproProfile::Cli);
    let sdk = WasiContextView::for_profile(ReproProfile::Sdk);

    vec![
        compare_row("argv", format_args(&cli.argv), format_args(&sdk.argv)),
        compare_row("env", format_env(&cli.env), format_env(&sdk.env)),
        compare_row(
            "preopen.host_path",
            cli.preopen_host_path.clone(),
            sdk.preopen_host_path.clone(),
        ),
        compare_row(
            "preopen.guest_path",
            cli.preopen_guest_path.clone(),
            sdk.preopen_guest_path.clone(),
        ),
        compare_row("stdio.stdin", cli.stdin.clone(), sdk.stdin.clone()),
        compare_row("stdio.stdout", cli.stdout.clone(), sdk.stdout.clone()),
        compare_row("stdio.stderr", cli.stderr.clone(), sdk.stderr.clone()),
        same_source_row(
            "clock.wall",
            cli.wall_clock.clone(),
            sdk.wall_clock.clone(),
            "Both profiles use WasiCtxBuilder default host wall clock.",
        ),
        same_source_row(
            "clock.monotonic",
            cli.monotonic_clock.clone(),
            sdk.monotonic_clock.clone(),
            "Both profiles use WasiCtxBuilder default host monotonic clock.",
        ),
        same_source_row(
            "random.secure",
            cli.secure_random.clone(),
            sdk.secure_random.clone(),
            "Both profiles use WasiCtxBuilder default secure RNG source.",
        ),
        same_source_row(
            "random.insecure",
            cli.insecure_random.clone(),
            sdk.insecure_random.clone(),
            "Both profiles use WasiCtxBuilder default insecure RNG source.",
        ),
        runtime_generated_row(
            "random.insecure_seed",
            "<generated-per-context>".to_string(),
            "<generated-per-context>".to_string(),
            "WasiCtxBuilder generates a fresh seed per context; values are intentionally not compared.",
        ),
    ]
}

fn compare_row(field: &'static str, cli: String, sdk: String) -> DiffRow {
    let status = if cli == sdk {
        STATUS_SAME
    } else {
        STATUS_DIFFERENT
    };
    DiffRow {
        field,
        cli,
        sdk,
        status,
        note: "",
    }
}

fn same_source_row(field: &'static str, cli: String, sdk: String, note: &'static str) -> DiffRow {
    DiffRow {
        field,
        cli,
        sdk,
        status: STATUS_SAME_SOURCE,
        note,
    }
}

fn runtime_generated_row(
    field: &'static str,
    cli: String,
    sdk: String,
    note: &'static str,
) -> DiffRow {
    DiffRow {
        field,
        cli,
        sdk,
        status: STATUS_RUNTIME_GENERATED,
        note,
    }
}

pub fn context_diff_report() -> String {
    let rows = context_diff_rows();
    let mut report =
        String::from("field | cli | sdk | status | note\n--- | --- | --- | --- | ---\n");
    for row in rows {
        report.push_str(&format!(
            "{} | {} | {} | {} | {}\n",
            row.field, row.cli, row.sdk, row.status, row.note
        ));
    }

    report
}

pub fn run(profile: ReproProfile, code: &str) -> Result<ReproRun> {
    let runtime_dir = cpython_runtime_dir();
    let wasm_path = runtime_dir.join("python.wasm");
    ensure_runtime_available(&runtime_dir, &wasm_path)?;

    let engine = create_engine()?;
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

    let context_view = WasiContextView::for_profile(profile);
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
        Err(err) => (false, Some(format_start_call_failure(&err))),
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

fn create_engine() -> Result<Engine> {
    let mut config = Config::new();
    config.wasm_backtrace(true);
    Engine::new(&config).context("Failed to create Wasmtime engine for CPython WASI repro")
}

fn format_start_call_failure(error: &anyhow::Error) -> String {
    let mut lines = vec![
        TRACE_CAPTURE_MARKER.to_string(),
        "start.call.error".to_string(),
        format!("display: {error}"),
        "chain:".to_string(),
    ];

    for (index, cause) in error.chain().enumerate() {
        lines.push(format!("  [{index}] {cause}"));
    }

    match error.downcast_ref::<WasmBacktrace>() {
        Some(backtrace) => {
            lines.push("trace.status=attached".to_string());
            lines.push(format!(
                "wasm_backtrace.frames={}",
                backtrace.frames().len()
            ));
            for (index, frame) in backtrace.frames().iter().enumerate() {
                let module_name = frame.module().name().unwrap_or("<unknown>");
                let func_name = frame.func_name().unwrap_or("<unknown>");
                let module_offset = frame
                    .module_offset()
                    .map(|offset| format!("{offset:#x}"))
                    .unwrap_or_else(|| "<none>".to_string());
                lines.push(format!(
                    "  frame[{index}] module={module_name} func={func_name} func_index={} module_offset={module_offset}",
                    frame.func_index()
                ));
            }
        }
        None => {
            lines.push("trace.status=missing".to_string());
            lines.push("wasm_backtrace.frames=0".to_string());
            lines.push("wasm_backtrace.note=not-attached".to_string());
        }
    }

    lines.push("debug:".to_string());
    lines.push(format!("{error:#}"));
    lines.join("\n")
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

    fn row_by_field<'a>(rows: &'a [DiffRow], field: &str) -> &'a DiffRow {
        rows.iter()
            .find(|row| row.field == field)
            .unwrap_or_else(|| {
                panic!("missing diff row for field: {field}");
            })
    }

    #[test]
    fn test_cpython_wasi_cli_profile_succeeds() {
        let result = run(ReproProfile::Cli, default_code()).unwrap();
        assert!(result.success, "{}", format_details(&result));
        assert!(result.stdout.lines().any(|line| line.trim() == "ok"));
    }

    #[test]
    fn test_cpython_wasi_sdk_profile_succeeds_after_runtime_fix() {
        let result = run(ReproProfile::Sdk, default_code()).unwrap();
        assert!(result.success, "{}", format_details(&result));
        assert!(result.stdout.lines().any(|line| line.trim() == "ok"));
    }

    #[test]
    fn test_cpython_wasi_sdk_legacy_profile_fails_with_missing_encodings() {
        let result = run(ReproProfile::SdkLegacy, default_code()).unwrap();
        assert!(!result.success, "{}", format_details(&result));
        assert!(
            result.stderr.contains("No module named 'encodings'"),
            "{}",
            format_details(&result)
        );
    }

    #[test]
    fn test_cpython_wasi_sdk_legacy_failure_includes_structured_trace_log() {
        let result = run(ReproProfile::SdkLegacy, default_code()).unwrap();
        assert!(!result.success, "{}", format_details(&result));

        let error = result
            .error
            .as_deref()
            .expect("error details should be present when _start fails");
        assert!(error.contains("trace.capture=wasm-backtrace-v1"));
        assert!(error.contains("start.call.error"));
        assert!(error.contains("trace.status=attached"));
        assert!(!error.contains("wasm_backtrace.note=not-attached"));

        let frames = error
            .lines()
            .find_map(|line| line.strip_prefix("wasm_backtrace.frames="))
            .and_then(|value| value.parse::<usize>().ok())
            .expect("wasm_backtrace.frames should be present and parseable");
        assert!(frames > 0, "expected at least one wasm backtrace frame");
    }

    #[test]
    fn test_cpython_wasi_repro_matches_cli_success_sdk_success_pattern() {
        let cli = run(ReproProfile::Cli, default_code()).unwrap();
        let sdk = run(ReproProfile::Sdk, default_code()).unwrap();

        assert!(cli.success, "{}", format_details(&cli));
        assert!(sdk.success, "{}", format_details(&sdk));
    }

    #[test]
    fn test_cpython_wasi_context_diff_report_includes_required_dimensions() {
        let rows = context_diff_rows();

        assert_eq!(row_by_field(&rows, "argv").status, STATUS_SAME);
        assert_eq!(row_by_field(&rows, "env").status, STATUS_SAME);
        assert_eq!(row_by_field(&rows, "preopen.host_path").status, STATUS_SAME);
        assert_eq!(
            row_by_field(&rows, "preopen.guest_path").status,
            STATUS_SAME
        );
        assert_eq!(row_by_field(&rows, "stdio.stdin").status, STATUS_SAME);
        assert_eq!(row_by_field(&rows, "stdio.stdout").status, STATUS_SAME);
        assert_eq!(row_by_field(&rows, "stdio.stderr").status, STATUS_SAME);
        assert_eq!(row_by_field(&rows, "clock.wall").status, STATUS_SAME_SOURCE);
        assert_eq!(
            row_by_field(&rows, "clock.monotonic").status,
            STATUS_SAME_SOURCE
        );
        assert_eq!(
            row_by_field(&rows, "random.secure").status,
            STATUS_SAME_SOURCE
        );
        assert_eq!(
            row_by_field(&rows, "random.insecure").status,
            STATUS_SAME_SOURCE
        );
        assert_eq!(
            row_by_field(&rows, "random.insecure_seed").status,
            STATUS_RUNTIME_GENERATED
        );
    }

    #[test]
    fn test_cpython_wasi_context_diff_report_shows_preopen_path_aligned() {
        let rows = context_diff_rows();
        let row = row_by_field(&rows, "preopen.guest_path");

        assert_eq!(row.cli, "/");
        assert_eq!(row.sdk, "/");
        assert_eq!(row.status, STATUS_SAME);
    }

    #[test]
    fn test_cpython_wasi_context_diff_report_masks_host_runtime_path() {
        let rows = context_diff_rows();
        let row = row_by_field(&rows, "preopen.host_path");

        assert_eq!(row.cli, RUNTIME_HOST_PATH_LABEL);
        assert_eq!(row.sdk, RUNTIME_HOST_PATH_LABEL);
        assert_eq!(row.status, STATUS_SAME);
    }
}
