use anyhow::{Context, Result};
use wasmtime::{Config, Engine, Linker, ResourceLimiter, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::WasiCtxBuilder;
use wasi_common::pipe::{ReadPipe, WritePipe};
use std::sync::{Arc, RwLock};
use std::io::Write;

#[derive(Clone)]
pub struct WasmRuntime {
    engine: Engine,
}

impl WasmRuntime {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true); // Enable fuel consumption for timeouts
        config.async_support(false); // We use sync for now
        
        let engine = Engine::new(&config).context("Failed to create Wasmtime Engine")?;
        Ok(Self { engine })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn create_linker(&self) -> Result<Linker<WasmSession>> {
        let mut linker = Linker::new(&self.engine);
        // Link WASI preview1
        preview1::add_to_linker_sync(&mut linker, |s: &mut WasmSession| &mut s.wasi_ctx)
            .context("Failed to link WASI preview1")?;
        Ok(linker)
    }

    pub fn create_store(&self, session: WasmSession) -> Store<WasmSession> {
        let mut store = Store::new(&self.engine, session);
        store.limiter(|s| s as &mut dyn ResourceLimiter);
        store
    }
}

pub struct WasmSession {
    wasi_ctx: WasiP1Ctx,
    limits: StoreLimits,
    pub stdout_buf: Arc<RwLock<Vec<u8>>>,
    pub stderr_buf: Arc<RwLock<Vec<u8>>>,
}

struct WriteWrapper(Arc<RwLock<Vec<u8>>>);

impl Write for WriteWrapper {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}


impl WasmSession {
    pub fn new(memory_limit_bytes: Option<usize>, code: &str) -> Self {
        // Fallback: Use inherit_stdio for now as custom pipe types are mismatched
        // between wasi-common and wasmtime-wasi in this version setup.
        // let stdin = ReadPipe::from(code.as_bytes().to_vec());
        
        let stdout_buf = Arc::new(RwLock::new(Vec::new()));
        let stderr_buf = Arc::new(RwLock::new(Vec::new()));
        
        // let stdout = WritePipe::new(WriteWrapper(stdout_buf.clone()));
        // let stderr = WritePipe::new(WriteWrapper(stderr_buf.clone()));

        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stdio();
        // builder.stdin(stdin).stdout(stdout).stderr(stderr);
            
        let wasi_ctx = builder.build_p1();

        let mut limits_builder = StoreLimitsBuilder::new();
        if let Some(mem) = memory_limit_bytes {
            limits_builder = limits_builder.memory_size(mem);
        }
        
        let limits = limits_builder.build();

        Self { wasi_ctx, limits, stdout_buf, stderr_buf }
    }
}

// Implement ResourceLimiter to delegate to StoreLimits
impl ResourceLimiter for WasmSession {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> Result<bool> {
        self.limits.memory_growing(current, desired, maximum)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> Result<bool> {
        self.limits.table_growing(current, desired, maximum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_creation() {
        let _session = WasmSession::new(Some(1024), "print('hello')");
    }
}
