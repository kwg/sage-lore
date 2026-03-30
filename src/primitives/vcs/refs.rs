// SPDX-License-Identifier: MIT
//! Reference operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;

impl Git2Backend {
    // ========================================================================
    // Reference Queries
    // ========================================================================

    pub(super) fn head_impl(&self) -> Result<String, GitError> {
        let repo = self.lock_repo();
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;
        Ok(commit.id().to_string())
    }

    pub(super) fn head_short_impl(&self) -> Result<String, GitError> {
        let repo = self.lock_repo();
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;
        let id = commit.id().to_string();
        Ok(id[..7].to_string())
    }

    pub(super) fn resolve_ref_impl(&self, ref_name: &str) -> Result<String, GitError> {
        let repo = self.lock_repo();
        let obj = repo
            .revparse_single(ref_name)
            .map_err(|_| GitError::InvalidRef(ref_name.to_string()))?;
        let commit = obj.peel_to_commit()?;
        Ok(commit.id().to_string())
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
    // Reference Operation Tests
    // ========================================================================

    #[test]
    fn test_head() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let hash = backend.head().unwrap();
        assert_eq!(hash.len(), 40);
    }

    #[test]
    fn test_head_short() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let short = backend.head_short().unwrap();
        assert_eq!(short.len(), 7);
    }

    #[test]
    fn test_resolve_ref() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let head = backend.head().unwrap();
        let resolved = backend.resolve_ref("HEAD").unwrap();
        assert_eq!(head, resolved);
    }

    #[test]
    fn test_resolve_invalid_ref() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let result = backend.resolve_ref("invalid-ref");
        assert!(result.is_err());
    }
}
