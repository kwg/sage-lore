// SPDX-License-Identifier: MIT
//! Mock filesystem backend for testing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use super::backend::FsBackend;
use super::policy::glob_match;
use super::types::{FileMeta, FsError};

// ============================================================================
// Call Recording Types
// ============================================================================

/// Record of a filesystem operation call for testing verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsCall {
    /// Read operation
    Read { path: PathBuf },
    /// Write operation
    Write { path: PathBuf, content: String },
    /// Append operation
    Append { path: PathBuf, content: String },
    /// Delete operation
    Delete { path: PathBuf },
    /// Exists check
    Exists { path: PathBuf },
    /// List directory
    List {
        path: PathBuf,
        pattern: Option<String>,
    },
    /// Create directory
    Mkdir { path: PathBuf },
    /// Copy file
    Copy { src: PathBuf, dest: PathBuf },
    /// Rename/move file
    Rename { src: PathBuf, dest: PathBuf },
    /// Stat file
    Stat { path: PathBuf },
}

// ============================================================================
// MockFsBackend
// ============================================================================

/// Mock filesystem backend for testing.
///
/// Provides an in-memory filesystem implementation with call recording
/// for verifying that scrolls make the expected filesystem calls.
///
/// # Features
///
/// - In-memory file storage (no actual disk I/O)
/// - Call recording for test assertions
/// - Directory support via path conventions
/// - Configurable initial filesystem state
///
/// # Thread Safety
///
/// This mock uses `RwLock` for interior mutability, making it safe for
/// use across threads. All operations acquire appropriate locks.
///
/// # Example
///
/// ```
/// use sage_lore::primitives::fs::{MockFsBackend, FsBackend, FsCall};
/// use std::path::Path;
///
/// let mock = MockFsBackend::new();
/// mock.set_file("test.txt", "hello world");
///
/// let content = mock.read(Path::new("test.txt")).unwrap();
/// assert_eq!(content, "hello world");
///
/// let calls = mock.calls();
/// assert_eq!(calls.len(), 1);
/// assert!(matches!(&calls[0], FsCall::Read { path } if path.to_string_lossy() == "test.txt"));
/// ```
pub struct MockFsBackend {
    /// In-memory file storage: path -> content
    files: RwLock<HashMap<PathBuf, String>>,
    /// Record of all filesystem calls made
    calls: RwLock<Vec<FsCall>>,
    /// Directories that exist (for exists/list operations)
    directories: RwLock<std::collections::HashSet<PathBuf>>,
}

impl MockFsBackend {
    /// Create a new empty mock filesystem.
    pub fn new() -> Self {
        Self {
            files: RwLock::new(HashMap::new()),
            calls: RwLock::new(Vec::new()),
            directories: RwLock::new(std::collections::HashSet::new()),
        }
    }

    /// Create a mock filesystem with initial files.
    ///
    /// # Arguments
    ///
    /// * `files` - Iterator of (path, content) pairs
    ///
    /// # Example
    ///
    /// ```
    /// use sage_lore::primitives::fs::MockFsBackend;
    ///
    /// let mock = MockFsBackend::with_files([
    ///     ("src/main.rs", "fn main() {}"),
    ///     ("Cargo.toml", "[package]\nname = \"test\""),
    /// ]);
    /// ```
    pub fn with_files<I, P, S>(files: I) -> Self
    where
        I: IntoIterator<Item = (P, S)>,
        P: AsRef<Path>,
        S: AsRef<str>,
    {
        let mock = Self::new();
        for (path, content) in files {
            mock.set_file(path.as_ref(), content.as_ref());
        }
        mock
    }

    /// Set a file's content in the mock filesystem.
    ///
    /// Creates parent directories as needed.
    pub fn set_file<P: AsRef<Path>>(&self, path: P, content: &str) {
        let path = path.as_ref().to_path_buf();

        // Create parent directories
        let mut current = PathBuf::new();
        for component in path.parent().into_iter().flat_map(|p| p.components()) {
            current.push(component);
            self.directories.write().unwrap().insert(current.clone());
        }

        self.files.write().unwrap().insert(path, content.to_string());
    }

    /// Get a file's content from the mock filesystem.
    pub fn get_file<P: AsRef<Path>>(&self, path: P) -> Option<String> {
        self.files.read().unwrap().get(path.as_ref()).cloned()
    }

    /// Check if a file exists in the mock filesystem.
    pub fn has_file<P: AsRef<Path>>(&self, path: P) -> bool {
        self.files.read().unwrap().contains_key(path.as_ref())
    }

    /// Remove a file from the mock filesystem.
    pub fn remove_file<P: AsRef<Path>>(&self, path: P) -> Option<String> {
        self.files.write().unwrap().remove(path.as_ref())
    }

    /// Add a directory to the mock filesystem.
    pub fn add_directory<P: AsRef<Path>>(&self, path: P) {
        let path = path.as_ref();

        // Add all parent directories too
        let mut current = PathBuf::new();
        for component in path.components() {
            current.push(component);
            self.directories.write().unwrap().insert(current.clone());
        }
    }

    /// Check if a directory exists in the mock filesystem.
    pub fn has_directory<P: AsRef<Path>>(&self, path: P) -> bool {
        self.directories.read().unwrap().contains(path.as_ref())
    }

    /// Get all recorded calls.
    pub fn calls(&self) -> Vec<FsCall> {
        self.calls.read().unwrap().clone()
    }

    /// Clear all recorded calls.
    pub fn clear_calls(&self) {
        self.calls.write().unwrap().clear();
    }

    /// Get the number of recorded calls.
    pub fn call_count(&self) -> usize {
        self.calls.read().unwrap().len()
    }

    /// Get all file paths in the mock filesystem.
    pub fn file_paths(&self) -> Vec<PathBuf> {
        self.files.read().unwrap().keys().cloned().collect()
    }

    /// Get all files as a hashmap (for inspection).
    pub fn files(&self) -> HashMap<PathBuf, String> {
        self.files.read().unwrap().clone()
    }

    /// Record a call.
    fn record(&self, call: FsCall) {
        self.calls.write().unwrap().push(call);
    }
}

impl Default for MockFsBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FsBackend for MockFsBackend {
    fn read(&self, path: &Path) -> Result<String, FsError> {
        self.record(FsCall::Read {
            path: path.to_path_buf(),
        });

        self.files
            .read()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found: {}", path.display()),
            )))
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), FsError> {
        self.record(FsCall::Write {
            path: path.to_path_buf(),
            content: content.to_string(),
        });

        // Create parent directories
        if let Some(parent) = path.parent() {
            self.add_directory(parent);
        }

        self.files
            .write()
            .unwrap()
            .insert(path.to_path_buf(), content.to_string());
        Ok(())
    }

    fn append(&self, path: &Path, content: &str) -> Result<(), FsError> {
        self.record(FsCall::Append {
            path: path.to_path_buf(),
            content: content.to_string(),
        });

        // Create parent directories
        if let Some(parent) = path.parent() {
            self.add_directory(parent);
        }

        let mut files = self.files.write().unwrap();
        let entry = files.entry(path.to_path_buf()).or_default();
        entry.push_str(content);
        Ok(())
    }

    fn delete(&self, path: &Path) -> Result<(), FsError> {
        self.record(FsCall::Delete {
            path: path.to_path_buf(),
        });

        if self.files.write().unwrap().remove(path).is_some() {
            Ok(())
        } else {
            Err(FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found: {}", path.display()),
            )))
        }
    }

    fn exists(&self, path: &Path) -> bool {
        self.record(FsCall::Exists {
            path: path.to_path_buf(),
        });

        self.files.read().unwrap().contains_key(path)
            || self.directories.read().unwrap().contains(path)
    }

    fn list(&self, path: &Path, pattern: Option<&str>) -> Result<Vec<PathBuf>, FsError> {
        self.record(FsCall::List {
            path: path.to_path_buf(),
            pattern: pattern.map(String::from),
        });

        let files = self.files.read().unwrap();
        let directories = self.directories.read().unwrap();

        // Check if the directory exists
        if !path.as_os_str().is_empty() && !directories.contains(path) {
            return Err(FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Directory not found: {}", path.display()),
            )));
        }

        let mut results = Vec::new();

        // List files in the directory
        for file_path in files.keys() {
            if let Ok(relative) = file_path.strip_prefix(path) {
                // Only include direct children (no nested paths)
                if relative.parent().is_some_and(|p| p.as_os_str().is_empty())
                    || relative.parent().is_none()
                {
                    // Apply pattern filter if provided
                    if let Some(pattern) = pattern {
                        let filename = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if glob_match(pattern, filename) {
                            results.push(file_path.clone());
                        }
                    } else {
                        results.push(file_path.clone());
                    }
                }
            }
        }

        // List subdirectories
        for dir_path in directories.iter() {
            if let Ok(relative) = dir_path.strip_prefix(path) {
                // Only include direct children
                if relative.components().count() == 1
                    && pattern.is_none() {
                        results.push(dir_path.clone());
                }
            }
        }

        Ok(results)
    }

    fn mkdir(&self, path: &Path) -> Result<(), FsError> {
        self.record(FsCall::Mkdir {
            path: path.to_path_buf(),
        });

        self.add_directory(path);
        Ok(())
    }

    fn copy(&self, src: &Path, dest: &Path) -> Result<(), FsError> {
        self.record(FsCall::Copy {
            src: src.to_path_buf(),
            dest: dest.to_path_buf(),
        });

        let content = self
            .files
            .read()
            .unwrap()
            .get(src)
            .cloned()
            .ok_or_else(|| FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Source file not found: {}", src.display()),
            )))?;

        // Create parent directories for destination
        if let Some(parent) = dest.parent() {
            self.add_directory(parent);
        }

        self.files.write().unwrap().insert(dest.to_path_buf(), content);
        Ok(())
    }

    fn rename(&self, src: &Path, dest: &Path) -> Result<(), FsError> {
        self.record(FsCall::Rename {
            src: src.to_path_buf(),
            dest: dest.to_path_buf(),
        });

        let content = self
            .files
            .write()
            .unwrap()
            .remove(src)
            .ok_or_else(|| FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Source file not found: {}", src.display()),
            )))?;

        // Create parent directories for destination
        if let Some(parent) = dest.parent() {
            self.add_directory(parent);
        }

        self.files.write().unwrap().insert(dest.to_path_buf(), content);
        Ok(())
    }

    fn stat(&self, path: &Path) -> Result<FileMeta, FsError> {
        self.record(FsCall::Stat {
            path: path.to_path_buf(),
        });

        let files = self.files.read().unwrap();
        let directories = self.directories.read().unwrap();

        if let Some(content) = files.get(path) {
            Ok(FileMeta {
                path: path.to_path_buf(),
                size: content.len() as u64,
                is_file: true,
                is_dir: false,
                is_symlink: false,
                modified: None,
                created: None,
            })
        } else if directories.contains(path) {
            Ok(FileMeta {
                path: path.to_path_buf(),
                size: 0,
                is_file: false,
                is_dir: true,
                is_symlink: false,
                modified: None,
                created: None,
            })
        } else {
            Err(FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Path not found: {}", path.display()),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_fs_new_empty() {
        let mock = MockFsBackend::new();
        assert_eq!(mock.call_count(), 0);
        assert!(mock.file_paths().is_empty());
    }

    #[test]
    fn test_mock_fs_with_files() {
        let mock = MockFsBackend::with_files([
            ("src/main.rs", "fn main() {}"),
            ("Cargo.toml", "[package]"),
        ]);

        assert!(mock.has_file("src/main.rs"));
        assert!(mock.has_file("Cargo.toml"));
        assert_eq!(mock.get_file("src/main.rs"), Some("fn main() {}".to_string()));
    }

    #[test]
    fn test_mock_fs_read() {
        let mock = MockFsBackend::new();
        mock.set_file("test.txt", "hello world");

        let result = mock.read(Path::new("test.txt"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello world");

        let calls = mock.calls();
        assert_eq!(calls.len(), 1);
        assert!(matches!(&calls[0], FsCall::Read { path } if path == Path::new("test.txt")));
    }

    #[test]
    fn test_mock_fs_read_not_found() {
        let mock = MockFsBackend::new();

        let result = mock.read(Path::new("nonexistent.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_fs_write() {
        let mock = MockFsBackend::new();

        let result = mock.write(Path::new("output.txt"), "new content");
        assert!(result.is_ok());

        assert_eq!(mock.get_file("output.txt"), Some("new content".to_string()));

        let calls = mock.calls();
        assert_eq!(calls.len(), 1);
        assert!(matches!(
            &calls[0],
            FsCall::Write { path, content }
            if path == Path::new("output.txt") && content == "new content"
        ));
    }

    #[test]
    fn test_mock_fs_write_creates_parent_directories() {
        let mock = MockFsBackend::new();

        mock.write(Path::new("a/b/c/file.txt"), "content").unwrap();

        assert!(mock.has_directory("a"));
        assert!(mock.has_directory("a/b"));
        assert!(mock.has_directory("a/b/c"));
    }

    #[test]
    fn test_mock_fs_append() {
        let mock = MockFsBackend::new();
        mock.set_file("log.txt", "line1\n");

        mock.append(Path::new("log.txt"), "line2\n").unwrap();

        assert_eq!(mock.get_file("log.txt"), Some("line1\nline2\n".to_string()));
    }

    #[test]
    fn test_mock_fs_append_new_file() {
        let mock = MockFsBackend::new();

        mock.append(Path::new("new.txt"), "first line").unwrap();

        assert_eq!(mock.get_file("new.txt"), Some("first line".to_string()));
    }

    #[test]
    fn test_mock_fs_delete() {
        let mock = MockFsBackend::new();
        mock.set_file("to_delete.txt", "content");

        let result = mock.delete(Path::new("to_delete.txt"));
        assert!(result.is_ok());
        assert!(!mock.has_file("to_delete.txt"));
    }

    #[test]
    fn test_mock_fs_delete_not_found() {
        let mock = MockFsBackend::new();

        let result = mock.delete(Path::new("nonexistent.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_fs_exists() {
        let mock = MockFsBackend::new();
        mock.set_file("exists.txt", "content");
        mock.add_directory("mydir");

        assert!(mock.exists(Path::new("exists.txt")));
        assert!(mock.exists(Path::new("mydir")));
        assert!(!mock.exists(Path::new("nope.txt")));
    }

    #[test]
    fn test_mock_fs_mkdir() {
        let mock = MockFsBackend::new();

        mock.mkdir(Path::new("new/nested/dir")).unwrap();

        assert!(mock.has_directory("new"));
        assert!(mock.has_directory("new/nested"));
        assert!(mock.has_directory("new/nested/dir"));
    }

    #[test]
    fn test_mock_fs_copy() {
        let mock = MockFsBackend::new();
        mock.set_file("source.txt", "original content");

        mock.copy(Path::new("source.txt"), Path::new("dest.txt")).unwrap();

        assert_eq!(mock.get_file("source.txt"), Some("original content".to_string()));
        assert_eq!(mock.get_file("dest.txt"), Some("original content".to_string()));
    }

    #[test]
    fn test_mock_fs_copy_not_found() {
        let mock = MockFsBackend::new();

        let result = mock.copy(Path::new("nope.txt"), Path::new("dest.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_fs_rename() {
        let mock = MockFsBackend::new();
        mock.set_file("old.txt", "content");

        mock.rename(Path::new("old.txt"), Path::new("new.txt")).unwrap();

        assert!(!mock.has_file("old.txt"));
        assert_eq!(mock.get_file("new.txt"), Some("content".to_string()));
    }

    #[test]
    fn test_mock_fs_stat_file() {
        let mock = MockFsBackend::new();
        mock.set_file("file.txt", "12345");

        let meta = mock.stat(Path::new("file.txt")).unwrap();

        assert!(meta.is_file);
        assert!(!meta.is_dir);
        assert_eq!(meta.size, 5);
    }

    #[test]
    fn test_mock_fs_stat_directory() {
        let mock = MockFsBackend::new();
        mock.add_directory("mydir");

        let meta = mock.stat(Path::new("mydir")).unwrap();

        assert!(!meta.is_file);
        assert!(meta.is_dir);
    }

    #[test]
    fn test_mock_fs_list() {
        let mock = MockFsBackend::new();
        mock.set_file("src/a.rs", "");
        mock.set_file("src/b.rs", "");
        mock.set_file("src/nested/c.rs", "");

        let entries = mock.list(Path::new("src"), None).unwrap();

        // Should only list direct children
        assert!(entries.iter().any(|p| p == Path::new("src/a.rs")));
        assert!(entries.iter().any(|p| p == Path::new("src/b.rs")));
        // Subdirectory should be included
        assert!(entries.iter().any(|p| p == Path::new("src/nested")));
    }

    #[test]
    fn test_mock_fs_list_with_pattern() {
        let mock = MockFsBackend::new();
        mock.set_file("src/main.rs", "");
        mock.set_file("src/lib.rs", "");
        mock.set_file("src/config.toml", "");

        let entries = mock.list(Path::new("src"), Some("*.rs")).unwrap();

        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|p| p.extension().is_some_and(|e| e == "rs")));
    }

    #[test]
    fn test_mock_fs_call_recording() {
        let mock = MockFsBackend::new();
        mock.set_file("test.txt", "content");

        mock.read(Path::new("test.txt")).unwrap();
        mock.write(Path::new("out.txt"), "data").unwrap();
        mock.exists(Path::new("test.txt"));

        let calls = mock.calls();
        assert_eq!(calls.len(), 3);
        assert!(matches!(&calls[0], FsCall::Read { .. }));
        assert!(matches!(&calls[1], FsCall::Write { .. }));
        assert!(matches!(&calls[2], FsCall::Exists { .. }));
    }

    #[test]
    fn test_mock_fs_clear_calls() {
        let mock = MockFsBackend::new();
        mock.set_file("test.txt", "content");

        mock.read(Path::new("test.txt")).unwrap();
        assert_eq!(mock.call_count(), 1);

        mock.clear_calls();
        assert_eq!(mock.call_count(), 0);
    }
}
