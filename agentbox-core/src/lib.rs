use pyo3::prelude::*;

mod cpython_wasi_repro;
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
}

#[pymethods]
impl SandboxConfig {
    #[new]
    #[pyo3(signature = (memory_limit_mb=None, timeout_ms=None, max_output_bytes=None))]
    fn new(
        memory_limit_mb: Option<usize>,
        timeout_ms: Option<u64>,
        max_output_bytes: Option<usize>,
    ) -> Self {
        SandboxConfig {
            memory_limit_mb,
            timeout_ms,
            max_output_bytes,
        }
    }
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
        let config = config.unwrap_or(SandboxConfig {
            memory_limit_mb: Some(512),
            timeout_ms: Some(10000),
            max_output_bytes: Some(runtime::DEFAULT_MAX_OUTPUT_BYTES),
        });

        let runtime = WasmRuntime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let vfs = VirtualFS::new();
        Ok(Sandbox {
            runtime,
            vfs,
            config,
        })
    }

    fn run(&self, code: String) -> PyResult<String> {
        let memory_bytes = self.config.memory_limit_mb.map(|mb| mb * 1024 * 1024);
        let max_output_bytes = self.config.max_output_bytes;

        let session =
            runtime::WasmSession::new(memory_bytes, max_output_bytes, &code, &self.vfs)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let linker = self
            .runtime
            .create_linker()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let mut store = self.runtime.create_store(session);
        self.runtime
            .arm_epoch_timeout(&mut store, self.config.timeout_ms);

        // Load WASM
        const WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/runner-wasm.wasm"));
        let module = wasmtime::Module::new(self.runtime.engine(), WASM)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        if let Err(error) = start.call(&mut store, ()) {
            if store.data().output_limit_exceeded() {
                return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Execution output exceeded max_output_bytes={} bytes",
                    store.data().output_limit_bytes()
                )));
            }

            let error_text = error.to_string();
            if let Some(timeout_ms) = self.config.timeout_ms {
                if WasmRuntime::is_epoch_timeout_error(&error) {
                    return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Execution timed out after {timeout_ms} ms"
                    )));
                }
            }

            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                error_text,
            ));
        }

        store
            .data()
            .sync_back_to_vfs(&self.vfs)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let stdout = store.data().stdout_pipe.contents();
        let stderr = store.data().stderr_pipe.contents();

        let mut combined = String::from_utf8_lossy(stdout.as_ref()).to_string();
        if !stderr.is_empty() {
            combined.push_str(&String::from_utf8_lossy(stderr.as_ref()));
        }

        Ok(combined)
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
    m.add_function(pyo3::wrap_pyfunction!(_debug_run_cpython_wasi_repro, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(
        _debug_describe_cpython_wasi_context_diff,
        m
    )?)?;
    Ok(())
}
