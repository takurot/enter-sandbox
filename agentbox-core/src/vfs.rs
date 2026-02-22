use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Component, Path};
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct VirtualFS {
    files: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl Default for VirtualFS {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl VirtualFS {
    pub fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let normalized = normalize_path(path)?;
        let mut files = self.files.write().unwrap();
        files.insert(normalized, content.to_vec());
        Ok(())
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let normalized = normalize_path(path)?;
        let files = self.files.read().unwrap();
        if let Some(content) = files.get(&normalized) {
            Ok(content.clone())
        } else {
            bail!("File not found: {}", normalized)
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        let Ok(normalized) = normalize_path(path) else {
            return false;
        };
        let files = self.files.read().unwrap();
        files.contains_key(&normalized)
    }

    pub fn remove_file(&self, path: &str) -> Result<()> {
        let normalized = normalize_path(path)?;
        let mut files = self.files.write().unwrap();
        if files.remove(&normalized).is_some() {
            Ok(())
        } else {
            bail!("File not found: {}", normalized)
        }
    }

    pub fn entries(&self) -> Vec<(String, Vec<u8>)> {
        let files = self.files.read().unwrap();
        let mut entries: Vec<(String, Vec<u8>)> = files
            .iter()
            .map(|(path, content)| (path.clone(), content.clone()))
            .collect();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    }

    pub fn replace_entries(&self, entries: Vec<(String, Vec<u8>)>) -> Result<()> {
        let mut next = HashMap::new();
        for (path, content) in entries {
            let normalized = normalize_path(&path)?;
            next.insert(normalized, content);
        }

        let mut files = self.files.write().unwrap();
        *files = next;
        Ok(())
    }
}

fn normalize_path(path: &str) -> Result<String> {
    if path.is_empty() {
        bail!("Path must not be empty");
    }

    let mut normalized_parts = Vec::new();
    for component in Path::new(path.trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => normalized_parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => bail!("Parent directory traversal is not allowed: {}", path),
            Component::Prefix(_) | Component::RootDir => bail!("Absolute paths are not allowed"),
        }
    }

    if normalized_parts.is_empty() {
        bail!("Path must reference a file");
    }

    Ok(normalized_parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_ops() {
        let fs = VirtualFS::new();
        fs.write_file("test.txt", b"hello").unwrap();
        assert_eq!(fs.read_file("test.txt").unwrap(), b"hello");
        assert!(fs.exists("test.txt"));
        fs.remove_file("test.txt").unwrap();
        assert!(!fs.exists("test.txt"));
    }

    #[test]
    fn test_vfs_normalizes_leading_slash_and_nested_paths() {
        let fs = VirtualFS::new();
        fs.write_file("/tmp/example.txt", b"hello").unwrap();
        assert!(fs.exists("tmp/example.txt"));
        assert_eq!(fs.read_file("/tmp/example.txt").unwrap(), b"hello");
    }

    #[test]
    fn test_vfs_blocks_parent_traversal() {
        let fs = VirtualFS::new();
        assert!(fs.write_file("../secret.txt", b"x").is_err());
        assert!(!fs.exists("../secret.txt"));
    }

    #[test]
    fn test_vfs_replace_entries_replaces_entire_state() {
        let fs = VirtualFS::new();
        fs.write_file("old.txt", b"old").unwrap();
        fs.replace_entries(vec![("new.txt".to_string(), b"new".to_vec())])
            .unwrap();

        assert!(!fs.exists("old.txt"));
        assert_eq!(fs.read_file("new.txt").unwrap(), b"new");
    }
}
