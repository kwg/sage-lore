// SPDX-License-Identifier: MIT
//! Tag operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;

impl Git2Backend {
    // ========================================================================
    // Tag Operations
    // ========================================================================

    pub(super) fn tag_impl(&self, name: &str, message: Option<&str>) -> Result<(), GitError> {
        let repo = self.lock_repo();

        // Get HEAD commit to tag
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;

        if let Some(msg) = message {
            // Annotated tag
            let sig = repo.signature()?;
            repo.tag(name, commit.as_object(), &sig, msg, false)?;
        } else {
            // Lightweight tag
            repo.tag_lightweight(name, commit.as_object(), false)?;
        }

        Ok(())
    }

    pub(super) fn list_tags_impl(&self) -> Result<Vec<String>, GitError> {
        let repo = self.lock_repo();

        let mut tags = Vec::new();
        repo.tag_foreach(|_oid, name| {
            if let Ok(name_str) = std::str::from_utf8(name) {
                // Remove refs/tags/ prefix
                let tag_name = name_str
                    .strip_prefix("refs/tags/")
                    .unwrap_or(name_str)
                    .to_string();
                tags.push(tag_name);
            }
            true // continue iterating
        })?;

        Ok(tags)
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
    // Tag Operation Tests
    // ========================================================================

    #[test]
    fn test_tag_lightweight() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        backend.tag("v1.0.0", None).unwrap();

        let tags = backend.list_tags().unwrap();
        assert!(tags.contains(&"v1.0.0".to_string()));
    }

    #[test]
    fn test_tag_annotated() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        backend.tag("v1.0.0", Some("Release version 1.0.0")).unwrap();

        let tags = backend.list_tags().unwrap();
        assert!(tags.contains(&"v1.0.0".to_string()));
    }

    #[test]
    fn test_list_tags_empty() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let tags = backend.list_tags().unwrap();
        assert!(tags.is_empty());
    }
}
