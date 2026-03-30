// SPDX-License-Identifier: MIT
//! Staging operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;
use std::path::Path;

impl Git2Backend {
    // ========================================================================
    // Staging Operations
    // ========================================================================

    pub(super) fn stage_impl(&self, paths: &[&str]) -> Result<(), GitError> {
        let repo = self.lock_repo();
        let mut index = repo.index()?;

        for path in paths {
            // Check if file exists - if not, it might be a deletion
            let full_path = self.workdir().join(path);
            if full_path.exists() {
                index.add_path(Path::new(path))?;
            } else {
                // File was deleted - remove from index
                index.remove_path(Path::new(path))?;
            }
        }

        index.write()?;
        Ok(())
    }

    pub(super) fn stage_all_impl(&self) -> Result<(), GitError> {
        let repo = self.lock_repo();
        let mut index = repo.index()?;

        // Add all changes including untracked files
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;

        // Also handle deletions
        index.update_all(["*"].iter(), None)?;

        index.write()?;
        Ok(())
    }

    pub(super) fn unstage_impl(&self, paths: &[&str]) -> Result<(), GitError> {
        let repo = self.lock_repo();
        let head = repo.head()?.peel_to_commit()?;
        let head_tree = head.tree()?;

        let mut index = repo.index()?;

        for path in paths {
            // Reset the file in the index to the HEAD state
            if let Ok(entry) = head_tree.get_path(Path::new(path)) {
                // File exists in HEAD - reset to that state
                let new_entry = git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: entry.filemode() as u32,
                    uid: 0,
                    gid: 0,
                    file_size: 0,
                    id: entry.id(),
                    flags: 0,
                    flags_extended: 0,
                    path: path.as_bytes().to_vec(),
                };
                index.add(&new_entry)?;
            } else {
                // File doesn't exist in HEAD - remove from index
                index.remove_path(Path::new(path))?;
            }
        }

        index.write()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::GitBackend;
    use super::*;
    use crate::primitives::secure::{SecureBackend, ScanResult, ScanType};
    use crate::config::SecurityError;
    use std::path::Path;
    use std::sync::Arc;
    use std::fs;

    /// Mock SecureBackend that always passes scans
    struct MockSecureBackend;

    impl SecureBackend for MockSecureBackend {
        fn secret_detection(&self, _content: &str) -> Result<ScanResult, SecurityError> {
            Ok(ScanResult {
                passed: true,
                findings: vec![],
                tool_used: "mock".to_string(),
                scan_type: ScanType::SecretDetection,
                duration_ms: 0,
            })
        }

        fn audit(&self, _root: &Path) -> Result<crate::primitives::secure::AuditReport, SecurityError> {
            unimplemented!()
        }

        fn dependency_scan(&self, _manifest: &Path) -> Result<crate::primitives::secure::CveReport, SecurityError> {
            unimplemented!()
        }

        fn static_analysis(&self, _path: &Path) -> Result<crate::primitives::secure::SastReport, SecurityError> {
            unimplemented!()
        }

        fn available_tools(&self) -> Vec<crate::primitives::secure::ToolStatus> {
            vec![]
        }
    }

    fn create_test_repo() -> (tempfile::TempDir, Git2Backend) {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp_dir.path()).unwrap();

        // Configure user for commits
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        drop(config);
        drop(repo);

        let secure = Arc::new(MockSecureBackend);
        let backend = Git2Backend::open(temp_dir.path(), secure).unwrap();

        (temp_dir, backend)
    }

    fn commit_initial(backend: &Git2Backend, temp_dir: &tempfile::TempDir) {
        // Create initial commit
        let test_file = temp_dir.path().join("README.md");
        fs::write(&test_file, "# Test Repository\n").unwrap();
        backend.stage(&["README.md"]).unwrap();
        backend.commit("Initial commit", None).unwrap();
    }

    // ========================================================================
    // Staging Operation Tests
    // ========================================================================

    #[test]
    fn test_stage_single_file() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let test_file = temp_dir.path().join("new_file.txt");
        fs::write(&test_file, "content").unwrap();

        backend.stage(&["new_file.txt"]).unwrap();

        let status = backend.status().unwrap();
        assert!(!status.staged.is_empty());
        assert!(status.staged.iter().any(|f| f.path == "new_file.txt"));
    }

    #[test]
    fn test_stage_multiple_files() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();

        backend.stage(&["file1.txt", "file2.txt"]).unwrap();

        let status = backend.status().unwrap();
        assert_eq!(status.staged.len(), 2);
    }

    #[test]
    fn test_stage_all() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();
        fs::write(temp_dir.path().join("file3.txt"), "content3").unwrap();

        backend.stage_all().unwrap();

        let status = backend.status().unwrap();
        assert_eq!(status.staged.len(), 3);
    }

    #[test]
    fn test_unstage() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("file.txt"), "content").unwrap();
        backend.stage(&["file.txt"]).unwrap();

        let status1 = backend.status().unwrap();
        assert!(!status1.staged.is_empty());

        backend.unstage(&["file.txt"]).unwrap();

        let status2 = backend.status().unwrap();
        assert!(status2.staged.is_empty());
    }

    #[test]
    fn test_stage_deleted_file() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let file_path = temp_dir.path().join("to_delete.txt");
        fs::write(&file_path, "content").unwrap();
        backend.stage(&["to_delete.txt"]).unwrap();
        backend.commit("Add file", None).unwrap();

        // Delete the file
        fs::remove_file(&file_path).unwrap();
        backend.stage(&["to_delete.txt"]).unwrap();

        let status = backend.status().unwrap();
        assert!(status.staged.iter().any(|f| f.path == "to_delete.txt" && f.status == FileStatusType::Deleted));
    }
}
