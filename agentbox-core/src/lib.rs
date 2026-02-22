use anyhow::{anyhow, Context, Result};
use pyo3::prelude::*;

mod cpython_wasi_repro;
mod import_policy;
mod runtime;
mod vfs;

use runtime::WasmRuntime;
use vfs::VirtualFS;

#[pyfunction]
#[pyo3(signature = (profile, code=None, timeout_ms=None, max_output_bytes=None))]
fn _debug_run_cpython_wasi_repro(
    profile: &str,
    code: Option<String>,
    timeout_ms: Option<u64>,
    max_output_bytes: Option<usize>,
) -> PyResult<(bool, String, String, Option<String>)> {
    let profile = cpython_wasi_repro::ReproProfile::parse(profile)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
    let source = code.unwrap_or_else(|| cpython_wasi_repro::default_code().to_string());
    let run = if timeout_ms.is_none() && max_output_bytes.is_none() {
        cpython_wasi_repro::run(profile, &source)
    } else {
        let options = cpython_wasi_repro::ReproRunOptions {
            timeout_ms,
            max_output_bytes,
        };
        cpython_wasi_repro::run_with_options(profile, &source, options)
    }
    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

    Ok((run.success, run.stdout, run.stderr, run.error))
}

#[pyfunction]
fn _debug_describe_cpython_wasi_context_diff() -> String {
    cpython_wasi_repro::context_diff_report()
}

#[pyclass]
#[derive(Clone)]
pub struct SandboxConfig {
    #[pyo3(get, set)]
    memory_limit_mb: Option<usize>,
    #[pyo3(get, set)]
    timeout_ms: Option<u64>,
    #[pyo3(get, set)]
    max_output_bytes: Option<usize>,
    #[pyo3(get, set)]
    allowed_modules: Option<Vec<String>>,
}

#[pymethods]
impl SandboxConfig {
    #[new]
    #[pyo3(signature = (memory_limit_mb=None, timeout_ms=None, max_output_bytes=None, allowed_modules=None))]
    fn new(
        memory_limit_mb: Option<usize>,
        timeout_ms: Option<u64>,
        max_output_bytes: Option<usize>,
        allowed_modules: Option<Vec<String>>,
    ) -> Self {
        SandboxConfig {
            memory_limit_mb,
            timeout_ms,
            max_output_bytes,
            allowed_modules,
        }
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct SandboxResult {
    #[pyo3(get)]
    stdout: String,
    #[pyo3(get)]
    stderr: String,
    #[pyo3(get)]
    exit_code: i32,
}

impl SandboxResult {
    fn success(stdout: String, stderr: String) -> Self {
        Self {
            stdout,
            stderr,
            exit_code: 0,
        }
    }
}

const DEFAULT_MEMORY_LIMIT_MB: usize = 512;
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const BYTES_PER_MB: usize = 1024 * 1024;

fn default_runtime_config() -> SandboxConfig {
    SandboxConfig {
        memory_limit_mb: Some(DEFAULT_MEMORY_LIMIT_MB),
        timeout_ms: Some(DEFAULT_TIMEOUT_MS),
        max_output_bytes: Some(runtime::DEFAULT_MAX_OUTPUT_BYTES),
        allowed_modules: None,
    }
}

fn execute_sandbox_run(
    runtime: &WasmRuntime,
    vfs: &VirtualFS,
    config: &SandboxConfig,
    code: &str,
) -> Result<SandboxResult> {
    import_policy::enforce_allowed_modules(code, config.allowed_modules.as_deref())
        .map_err(|e| anyhow!(e.to_string()))?;

    let memory_bytes = resolve_memory_limit_bytes(config.memory_limit_mb)?;
    let max_output_bytes = config.max_output_bytes;

    let session = runtime::WasmSession::new(memory_bytes, max_output_bytes, code, vfs)
        .context("Failed to initialize wasm session")?;
    let linker = runtime
        .create_linker()
        .context("Failed to initialize wasm linker")?;

    let mut store = runtime.create_store(session);
    runtime.arm_epoch_timeout(&mut store, config.timeout_ms);

    const WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/runner-wasm.wasm"));
    let module = wasmtime::Module::new(runtime.engine(), WASM)
        .context("Failed to compile runner wasm module")?;

    let instance = linker
        .instantiate(&mut store, &module)
        .context("Failed to instantiate runner wasm module")?;
    let start = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .context("Failed to resolve _start export from runner wasm module")?;

    if let Err(error) = start.call(&mut store, ()) {
        if store.data().output_limit_exceeded() {
            return Err(anyhow!(
                "Execution output exceeded max_output_bytes={} bytes",
                store.data().output_limit_bytes()
            ));
        }

        if let Some(timeout_ms) = config.timeout_ms {
            if WasmRuntime::is_epoch_timeout_error(&error) {
                return Err(anyhow!("Execution timed out after {timeout_ms} ms"));
            }
        }

        return Err(anyhow!(error.to_string()));
    }

    store
        .data()
        .sync_back_to_vfs(vfs)
        .context("Failed to synchronize sandbox filesystem")?;

    let stdout = String::from_utf8_lossy(store.data().stdout_pipe.contents().as_ref()).to_string();
    let stderr = String::from_utf8_lossy(store.data().stderr_pipe.contents().as_ref()).to_string();
    Ok(SandboxResult::success(stdout, stderr))
}

fn resolve_memory_limit_bytes(memory_limit_mb: Option<usize>) -> Result<Option<usize>> {
    memory_limit_mb
        .map(|mb| {
            mb.checked_mul(BYTES_PER_MB)
                .ok_or_else(|| anyhow!("memory_limit_mb={mb} is too large to convert to bytes"))
        })
        .transpose()
}

/// Execute one full `Sandbox.run()`-equivalent cycle with a fresh runtime.
/// This is intended for cold-start benchmarks and non-Python Rust callers.
pub fn run_code_once_for_benchmark(code: &str) -> Result<()> {
    let runtime = WasmRuntime::new().context("Failed to create Wasmtime runtime")?;
    let vfs = VirtualFS::new();
    let config = default_runtime_config();
    execute_sandbox_run(&runtime, &vfs, &config, code).map(|_| ())
}

#[pyclass]
struct Sandbox {
    runtime: WasmRuntime,
    #[allow(dead_code)]
    vfs: VirtualFS,
    config: SandboxConfig,
}

#[pymethods]
impl Sandbox {
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(config: Option<SandboxConfig>) -> PyResult<Self> {
        let config = config.unwrap_or_else(default_runtime_config);

        let runtime = WasmRuntime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let vfs = VirtualFS::new();
        Ok(Sandbox {
            runtime,
            vfs,
            config,
        })
    }

    fn run(&self, code: String) -> PyResult<SandboxResult> {
        execute_sandbox_run(&self.runtime, &self.vfs, &self.config, &code)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    // Config getter
    #[getter]
    fn config(&self) -> SandboxConfig {
        self.config.clone()
    }
}

/// A Python module implemented in Rust.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Sandbox>()?;
    m.add_class::<SandboxConfig>()?;
    m.add_class::<SandboxResult>()?;
    m.add_function(pyo3::wrap_pyfunction!(_debug_run_cpython_wasi_repro, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(
        _debug_describe_cpython_wasi_context_diff,
        m
    )?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_memory_limit_bytes_converts_mb_to_bytes() {
        let bytes = resolve_memory_limit_bytes(Some(16)).expect("conversion should succeed");
        assert_eq!(bytes, Some(16 * BYTES_PER_MB));
    }

    #[test]
    fn resolve_memory_limit_bytes_rejects_overflowing_values() {
        let overflowing_mb = (usize::MAX / BYTES_PER_MB) + 1;
        let error = resolve_memory_limit_bytes(Some(overflowing_mb))
            .expect_err("overflowing values must return an error");
        assert!(
            error
                .to_string()
                .contains("is too large to convert to bytes"),
            "unexpected error: {error}"
        );
    }
}
