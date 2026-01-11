use pyo3::prelude::*;

mod runtime;
mod vfs;

use runtime::WasmRuntime;
use vfs::VirtualFS;

#[pyclass]
#[derive(Clone)]
pub struct SandboxConfig {
    #[pyo3(get, set)]
    memory_limit_mb: Option<usize>,
    #[pyo3(get, set)]
    timeout_ms: Option<u64>,
}

#[pymethods]
impl SandboxConfig {
    #[new]
    #[pyo3(signature = (memory_limit_mb=None, timeout_ms=None))]
    fn new(memory_limit_mb: Option<usize>, timeout_ms: Option<u64>) -> Self {
        SandboxConfig {
            memory_limit_mb,
            timeout_ms,
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

        let session = runtime::WasmSession::new(memory_bytes, &code);
        let linker = self
            .runtime
            .create_linker()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let mut store = self.runtime.create_store(session);

        // Timeout handling via timeout_ms?
        // Runtime has consume_fuel enabled.
        // We can add fuel.
        if let Some(ms) = self.config.timeout_ms {
            // Heuristic: 1ms ~ 10_000 fuel?
            // Just setting a limit for now.
            store.set_fuel(ms * 10_000).ok();
        } else {
            store.set_fuel(u64::MAX).ok();
        }

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

        // start.call(&mut store, ()) ... map error
        match start.call(&mut store, ()) {
            Ok(_) => {}
            Err(e) => {
                // Capture stderr?
                // Or return output anyway?
                // Usually output is present even on error.
                eprintln!("Execution error: {}", e);
            }
        }

        // Get Output
        let output_lock = store.data().stdout_buf.read().unwrap();
        Ok(String::from_utf8_lossy(&output_lock).to_string())
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
    Ok(())
}
