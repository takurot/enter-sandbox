use crate::vfs::VirtualFS;
use anyhow::{bail, Context, Result};
use once_cell::sync::OnceCell;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use wasmtime::{
    Config, Engine, Linker, Module, ResourceLimiter, Store, StoreLimits, StoreLimitsBuilder, Trap,
};
use wasmtime_wasi::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const INTERNAL_CODE_PATH: &str = "__agentbox_internal__/code.py";
const INTERNAL_PATH_PREFIX: &str = "__agentbox_internal__/";
const EPOCH_TICK_INTERVAL_MS: u64 = 1;
const NO_TIMEOUT_EPOCH_DEADLINE_TICKS: u64 = u64::MAX / 2;
const RUNNER_WASM_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/runner-wasm.wasm"));

pub struct WasmRuntime {
    shared: Arc<SharedRuntimeState>,
}

struct SharedRuntimeState {
    engine: Engine,
    module: Module,
}

impl WasmRuntime {
    pub fn new() -> Result<Self> {
        static SHARED_RUNTIME_STATE: OnceCell<Arc<SharedRuntimeState>> = OnceCell::new();
        let shared = SHARED_RUNTIME_STATE
            .get_or_try_init(init_shared_runtime_state)?
            .clone();
        Ok(Self { shared })
    }

    pub fn engine(&self) -> &Engine {
        &self.shared.engine
    }

    pub fn module(&self) -> &Module {
        &self.shared.module
    }

    pub fn create_linker(&self) -> Result<Linker<WasmSession>> {
        let mut linker = Linker::new(self.engine());
        preview1::add_to_linker_sync(&mut linker, |s: &mut WasmSession| &mut s.wasi_ctx)
            .context("Failed to link WASI preview1")?;
        Ok(linker)
    }

    pub fn create_store(&self, session: WasmSession) -> Store<WasmSession> {
        let mut store = Store::new(self.engine(), session);
        store.limiter(|s| s as &mut dyn ResourceLimiter);
        store
    }

    pub fn arm_epoch_timeout<T>(&self, store: &mut Store<T>, timeout_ms: Option<u64>) {
        let Some(timeout_ms) = timeout_ms else {
            // Explicitly disable timeout for this run. With epoch interruption enabled,
            // the default deadline is immediate and must be overridden.
            store.set_epoch_deadline(NO_TIMEOUT_EPOCH_DEADLINE_TICKS);
            return;
        };

        let ticks = timeout_ms.div_ceil(EPOCH_TICK_INTERVAL_MS).max(1);
        store.set_epoch_deadline(ticks);
    }

    pub fn is_epoch_timeout_error(error: &wasmtime::Error) -> bool {
        if let Some(trap) = error.downcast_ref::<Trap>() {
            if matches!(trap, Trap::Interrupt) {
                return true;
            }
        }

        error.chain().any(|cause| {
            let lower = cause.to_string().to_ascii_lowercase();
            lower.contains("interrupt") || lower.contains("epoch deadline")
        })
    }
}

fn init_shared_runtime_state() -> Result<Arc<SharedRuntimeState>> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    config.async_support(false);

    let engine = Engine::new(&config).context("Failed to create Wasmtime Engine")?;
    spawn_epoch_ticker(engine.clone())?;
    let module =
        Module::new(&engine, RUNNER_WASM_BYTES).context("Failed to compile runner wasm module")?;

    Ok(Arc::new(SharedRuntimeState { engine, module }))
}

fn spawn_epoch_ticker(engine: Engine) -> Result<()> {
    thread::Builder::new()
        .name("agentbox-epoch-ticker".to_string())
        .spawn(move || loop {
            thread::sleep(Duration::from_millis(EPOCH_TICK_INTERVAL_MS));
            engine.increment_epoch();
        })
        .context("Failed to spawn epoch ticker thread")?;
    Ok(())
}

impl WasmRuntime {
    #[cfg(test)]
    fn shared_ptr(&self) -> *const SharedRuntimeState {
        Arc::as_ptr(&self.shared)
    }
}

pub struct WasmSession {
    wasi_ctx: WasiP1Ctx,
    limits: StoreLimits,
    pub stdout_pipe: MemoryOutputPipe,
    pub stderr_pipe: MemoryOutputPipe,
    output_limit_bytes: usize,
    sandbox_root: TempDir,
}

impl WasmSession {
    pub fn new(
        memory_limit_bytes: Option<usize>,
        max_output_bytes: Option<usize>,
        code: &str,
        vfs: &VirtualFS,
    ) -> Result<Self> {
        let sandbox_root = materialize_virtual_fs(vfs, code)?;
        let output_limit_bytes = resolve_output_limit(max_output_bytes)?;
        let stdout_pipe = MemoryOutputPipe::new(output_limit_bytes);
        let stderr_pipe = MemoryOutputPipe::new(output_limit_bytes);

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
            output_limit_bytes,
            sandbox_root,
        })
    }

    pub fn sync_back_to_vfs(&self, vfs: &VirtualFS) -> Result<()> {
        let mut entries = Vec::new();
        collect_files_in_dir(
            self.sandbox_root.path(),
            self.sandbox_root.path(),
            &mut entries,
        )?;
        entries.retain(|(path, _)| !is_internal_path(path));
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        vfs.replace_entries(entries)
            .context("Failed to synchronize VirtualFS from sandbox")
    }

    pub fn output_limit_exceeded(&self) -> bool {
        self.stdout_pipe.contents().len() >= self.output_limit_bytes
            || self.stderr_pipe.contents().len() >= self.output_limit_bytes
    }

    pub fn output_limit_bytes(&self) -> usize {
        self.output_limit_bytes
    }
}

fn materialize_virtual_fs(vfs: &VirtualFS, code: &str) -> Result<TempDir> {
    let sandbox_root = tempfile::tempdir().context("Failed to create sandbox temp directory")?;

    for (relative_path, content) in vfs.entries() {
        if is_internal_path(&relative_path) {
            continue;
        }
        write_relative_file(sandbox_root.path(), &relative_path, &content)?;
    }

    write_relative_file(sandbox_root.path(), INTERNAL_CODE_PATH, code.as_bytes())?;
    Ok(sandbox_root)
}

fn resolve_output_limit(max_output_bytes: Option<usize>) -> Result<usize> {
    let output_limit = max_output_bytes.unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);
    if output_limit == 0 {
        bail!("max_output_bytes must be greater than 0");
    }
    Ok(output_limit)
}

fn is_internal_path(path: &str) -> bool {
    path == "__agentbox_internal__" || path.starts_with(INTERNAL_PATH_PREFIX)
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

fn collect_files_in_dir(
    root: &Path,
    current_dir: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<()> {
    let mut entries = fs::read_dir(current_dir)
        .context("Failed to list sandbox directory")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to read sandbox directory entry")?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .context("Failed to read sandbox entry type")?;

        if file_type.is_dir() {
            collect_files_in_dir(root, &path, files)?;
            continue;
        }

        if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .context("Sandbox entry escaped root")?;
            let relative_path = relative
                .components()
                .filter_map(|component| match component {
                    Component::Normal(part) => Some(part.to_string_lossy().to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");

            if relative_path.is_empty() {
                continue;
            }

            let content = fs::read(&path).context("Failed to read sandbox file content")?;
            files.push((relative_path, content));
        }
    }

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
        let _session = WasmSession::new(Some(1024), None, "print('hello')", &vfs).unwrap();
    }

    #[test]
    fn test_wasm_runtime_reuses_engine_across_instances() {
        let first = WasmRuntime::new().unwrap();
        let second = WasmRuntime::new().unwrap();

        assert_eq!(first.shared_ptr(), second.shared_ptr());
        assert!(
            std::ptr::eq(first.engine(), second.engine()),
            "WasmRuntime should reuse a shared Engine across instances"
        );
        assert!(
            std::ptr::eq(first.module(), second.module()),
            "WasmRuntime should reuse a shared compiled runner module across instances"
        );
    }

    #[test]
    fn test_materialize_virtual_fs_includes_user_files_and_code() {
        let vfs = VirtualFS::new();
        vfs.write_file("input/data.txt", b"payload").unwrap();

        let session = WasmSession::new(None, None, "print('hello')", &vfs).unwrap();
        let root = session.sandbox_root.path();

        assert_eq!(fs::read(root.join("input/data.txt")).unwrap(), b"payload");
        assert_eq!(
            fs::read_to_string(root.join(INTERNAL_CODE_PATH)).unwrap(),
            "print('hello')"
        );
    }

    #[test]
    fn test_sync_back_to_vfs_reflects_creates_and_deletes() {
        let vfs = VirtualFS::new();
        vfs.write_file("old.txt", b"old").unwrap();

        let session = WasmSession::new(None, None, "print('hello')", &vfs).unwrap();
        let root = session.sandbox_root.path();
        fs::remove_file(root.join("old.txt")).unwrap();
        write_relative_file(root, "new/data.txt", b"new").unwrap();

        session.sync_back_to_vfs(&vfs).unwrap();

        assert!(!vfs.exists("old.txt"));
        assert_eq!(vfs.read_file("new/data.txt").unwrap(), b"new");
        assert!(!vfs.exists(INTERNAL_CODE_PATH));
    }

    #[test]
    fn test_arm_epoch_timeout_interrupts_long_running_module() {
        let runtime = WasmRuntime::new().unwrap();
        let module = wasmtime::Module::new(
            runtime.engine(),
            r#"(module
                (func (export "_start")
                    (loop $loop
                        br $loop
                    )
                )
            )"#,
        )
        .unwrap();
        let mut store = Store::new(runtime.engine(), ());
        runtime.arm_epoch_timeout(&mut store, Some(20));
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .unwrap();

        let err = start
            .call(&mut store, ())
            .expect_err("infinite module should be interrupted by epoch timeout");
        assert!(
            WasmRuntime::is_epoch_timeout_error(&err),
            "unexpected timeout error: {err}"
        );
    }

    #[test]
    fn test_arm_epoch_timeout_none_keeps_execution_successful() {
        let runtime = WasmRuntime::new().unwrap();
        let module = wasmtime::Module::new(
            runtime.engine(),
            r#"(module
                (func (export "_start"))
            )"#,
        )
        .unwrap();
        let mut store = Store::new(runtime.engine(), ());
        runtime.arm_epoch_timeout(&mut store, None);
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
        let start = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .unwrap();

        start.call(&mut store, ()).unwrap();
    }
}
