// SPDX-License-Identifier: MIT
//! Reset operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;

impl Git2Backend {
    // ========================================================================
    // Reset Operations
    // ========================================================================

    pub(super) fn reset_hard_impl(&self, commit: &str) -> Result<(), GitError> {
        let repo = self.lock_repo();

        // Resolve the commit ref
        let obj = repo
            .revparse_single(commit)
            .map_err(|_| GitError::InvalidRef(commit.to_string()))?;
        let commit_obj = obj
            .peel_to_commit()
            .map_err(|_| GitError::InvalidRef(commit.to_string()))?;

        // Reset hard to the commit
        repo.reset(commit_obj.as_object(), git2::ResetType::Hard, None)?;

        Ok(())
    }

    pub(super) fn reset_soft_impl(&self, commit: &str) -> Result<(), GitError> {
        let repo = self.lock_repo();

        // Resolve the commit ref
        let obj = repo
            .revparse_single(commit)
            .map_err(|_| GitError::InvalidRef(commit.to_string()))?;
        let commit_obj = obj
            .peel_to_commit()
            .map_err(|_| GitError::InvalidRef(commit.to_string()))?;

        // Reset soft to the commit
        repo.reset(commit_obj.as_object(), git2::ResetType::Soft, None)?;

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
    // Reset Operation Tests
    // ========================================================================

    #[test]
    fn test_reset_soft() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("file.txt"), "content").unwrap();
        backend.stage(&["file.txt"]).unwrap();
        backend.commit("Second commit", None).unwrap();

        // Reset soft to previous commit
        backend.reset_soft("HEAD~1").unwrap();

        let status = backend.status().unwrap();
        // After soft reset, changes should be staged
        assert!(status.staged.iter().any(|f| f.path == "file.txt"));
    }

    #[test]
    fn test_reset_hard() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("file.txt"), "content").unwrap();
        backend.stage(&["file.txt"]).unwrap();
        backend.commit("Second commit", None).unwrap();

        backend.reset_hard("HEAD~1").unwrap();

        let status = backend.status().unwrap();
        assert!(status.clean);
        assert!(!temp_dir.path().join("file.txt").exists());
    }
}
