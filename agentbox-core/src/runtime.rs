use crate::vfs::VirtualFS;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};
use tempfile::TempDir;
use wasmtime::{Config, Engine, Linker, ResourceLimiter, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

#[derive(Clone)]
pub struct WasmRuntime {
    engine: Engine,
}

impl WasmRuntime {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true); // Enable fuel consumption for timeouts
        config.async_support(false);

        // Optimize for speed
        // config.cranelift_opt_level(wasmtime::OptLevel::Speed);

        let engine = Engine::new(&config).context("Failed to create Wasmtime Engine")?;
        Ok(Self { engine })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn create_linker(&self) -> Result<Linker<WasmSession>> {
        let mut linker = Linker::new(&self.engine);
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
    pub stdout_pipe: MemoryOutputPipe,
    pub stderr_pipe: MemoryOutputPipe,
    _sandbox_root: TempDir,
}

impl WasmSession {
    pub fn new(memory_limit_bytes: Option<usize>, code: &str, vfs: &VirtualFS) -> Result<Self> {
        let sandbox_root = materialize_virtual_fs(vfs, code)?;
        let stdout_pipe = MemoryOutputPipe::new(1024 * 1024);
        let stderr_pipe = MemoryOutputPipe::new(1024 * 1024);

        let mut builder = WasiCtxBuilder::new();
        builder
            .stdin(MemoryInputPipe::new(code.as_bytes().to_vec()))
            .stdout(stdout_pipe.clone())
            .stderr(stderr_pipe.clone())
            .preopened_dir(
                sandbox_root.path(),
                "/sandbox",
                DirPerms::all(),
                FilePerms::all(),
            )
            .context("Failed to preopen sandbox VirtualFS")?;
        let wasi_ctx = builder.build_p1();

        let mut limits_builder = StoreLimitsBuilder::new();
        if let Some(mem) = memory_limit_bytes {
            limits_builder = limits_builder.memory_size(mem);
        }
        // Explicit table limits
        limits_builder = limits_builder.table_elements(10000); // 10k elements

        let limits = limits_builder.build();

        Ok(Self {
            wasi_ctx,
            limits,
            stdout_pipe,
            stderr_pipe,
            _sandbox_root: sandbox_root,
        })
    }
}

fn materialize_virtual_fs(vfs: &VirtualFS, code: &str) -> Result<TempDir> {
    let sandbox_root = tempfile::tempdir().context("Failed to create sandbox temp directory")?;

    for (relative_path, content) in vfs.entries() {
        write_relative_file(sandbox_root.path(), &relative_path, &content)?;
    }

    write_relative_file(sandbox_root.path(), "code.py", code.as_bytes())?;
    Ok(sandbox_root)
}

fn write_relative_file(root: &Path, relative_path: &str, content: &[u8]) -> Result<()> {
    if relative_path.is_empty() {
        bail!("Relative path must not be empty");
    }

    let mut target = PathBuf::from(root);
    for component in Path::new(relative_path).components() {
        match component {
            Component::Normal(part) => target.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("Path escapes sandbox root: {}", relative_path)
            }
        }
    }

    if target == root {
        bail!("Relative path must reference a file");
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).context("Failed to create parent directories in sandbox")?;
    }

    fs::write(&target, content).context("Failed to write file into sandbox")?;
    Ok(())
}

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
        let vfs = VirtualFS::new();
        let _session = WasmSession::new(Some(1024), "print('hello')", &vfs).unwrap();
    }

    #[test]
    fn test_materialize_virtual_fs_includes_user_files_and_code() {
        let vfs = VirtualFS::new();
        vfs.write_file("input/data.txt", b"payload").unwrap();

        let session = WasmSession::new(None, "print('hello')", &vfs).unwrap();
        let root = session._sandbox_root.path();

        assert_eq!(fs::read(root.join("input/data.txt")).unwrap(), b"payload");
        assert_eq!(
            fs::read_to_string(root.join("code.py")).unwrap(),
            "print('hello')"
        );
    }
}
