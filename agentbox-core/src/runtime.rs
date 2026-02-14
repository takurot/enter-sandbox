use crate::vfs::VirtualFS;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};
use tempfile::TempDir;
use wasmtime::{Config, Engine, Linker, ResourceLimiter, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::preview1::{self, WasiP1Ctx};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const INTERNAL_CODE_PATH: &str = "__agentbox_internal__/code.py";
const INTERNAL_PATH_PREFIX: &str = "__agentbox_internal__/";

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
}
