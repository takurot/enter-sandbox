use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use anyhow::{Result, bail};

#[derive(Clone, Debug)]
pub struct VirtualFS {
    files: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl VirtualFS {
    pub fn new() -> Self {
        Self {
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let mut files = self.files.write().unwrap();
        files.insert(path.to_string(), content.to_vec());
        Ok(())
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let files = self.files.read().unwrap();
        if let Some(content) = files.get(path) {
            Ok(content.clone())
        } else {
            bail!("File not found: {}", path)
        }
    }

    pub fn exists(&self, path: &str) -> bool {
        let files = self.files.read().unwrap();
        files.contains_key(path)
    }

    pub fn remove_file(&self, path: &str) -> Result<()> {
        let mut files = self.files.write().unwrap();
        if files.remove(path).is_some() {
            Ok(())
        } else {
            bail!("File not found: {}", path)
        }
    }
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
}
