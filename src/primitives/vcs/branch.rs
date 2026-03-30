// SPDX-License-Identifier: MIT
//! Branch operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;

impl Git2Backend {
    pub(super) fn ensure_branch_impl(&self, name: &str) -> Result<BranchResult, GitError> {
        let repo = self.lock_repo();

        // Check if branch already exists
        if let Ok(branch) = repo.find_branch(name, git2::BranchType::Local) {
            let commit = branch.get().peel_to_commit()?;
            let tracking = Self::get_tracking_branch(&repo, name);
            return Ok(BranchResult {
                name: name.to_string(),
                created: false,
                base_commit: commit.id().to_string(),
                tracking,
            });
        }

        // Create new branch from HEAD
        let head_commit = repo.head()?.peel_to_commit()?;
        let branch = repo.branch(name, &head_commit, false)?;
        let commit_id = branch.get().peel_to_commit()?.id().to_string();

        Ok(BranchResult {
            name: name.to_string(),
            created: true,
            base_commit: commit_id,
            tracking: None,
        })
    }

    pub(super) fn branch_impl(&self, name: &str, start_point: Option<&str>) -> Result<BranchResult, GitError> {
        let repo = self.lock_repo();

        let commit = match start_point {
            Some(ref_name) => {
                // Resolve the start point to a commit
                let obj = repo
                    .revparse_single(ref_name)
                    .map_err(|_| GitError::CommitNotFound(ref_name.to_string()))?;
                obj.peel_to_commit()?
            }
            None => repo.head()?.peel_to_commit()?,
        };

        let branch = repo.branch(name, &commit, false)?;
        let commit_id = branch.get().peel_to_commit()?.id().to_string();

        Ok(BranchResult {
            name: name.to_string(),
            created: true,
            base_commit: commit_id,
            tracking: None,
        })
    }

    pub(super) fn checkout_impl(&self, ref_name: &str) -> Result<(), GitError> {
        let repo = self.lock_repo();

        // Try to find as a branch first
        let obj = repo
            .revparse_single(ref_name)
            .map_err(|_| GitError::BranchNotFound(ref_name.to_string()))?;

        // Set HEAD
        if let Ok(branch) = repo.find_branch(ref_name, git2::BranchType::Local) {
            // Checkout branch - set HEAD to branch ref
            let refname = branch
                .get()
                .name()
                .ok_or_else(|| GitError::InvalidRef(ref_name.to_string()))?;
            repo.set_head(refname)?;
        } else {
            // Detached HEAD checkout
            repo.set_head_detached(obj.id())?;
        }

        // Checkout the tree
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new()
                .safe()
                .force(),
        ))?;

        Ok(())
    }

    pub(super) fn current_branch_impl(&self) -> Result<String, GitError> {
        let repo = self.lock_repo();
        let head = repo.head()?;
        if head.is_branch() {
            head.shorthand()
                .map(|s| s.to_string())
                .ok_or_else(|| GitError::InvalidRef("HEAD".to_string()))
        } else {
            Err(GitError::InvalidRef("HEAD is detached".to_string()))
        }
    }

    pub(super) fn branch_exists_impl(&self, name: &str) -> bool {
        let repo = self.lock_repo();
        let exists = repo.find_branch(name, git2::BranchType::Local).is_ok();
        exists
    }

    pub(super) fn delete_branch_impl(&self, name: &str, force: bool) -> Result<(), GitError> {
        let repo = self.lock_repo();

        let mut branch = repo
            .find_branch(name, git2::BranchType::Local)
            .map_err(|_| GitError::BranchNotFound(name.to_string()))?;

        // Check if branch is fully merged unless force is set
        if !force {
            // Get HEAD commit
            let head = repo.head()?.peel_to_commit()?;
            let branch_commit = branch.get().peel_to_commit()?;

            // Check if branch commit is an ancestor of HEAD
            if !repo.graph_descendant_of(head.id(), branch_commit.id())? {
                return Err(GitError::BranchNotFound(format!(
                    "Branch '{}' is not fully merged. Use force=true to delete anyway.",
                    name
                )));
            }
        }

        branch.delete()?;
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

    #[test]
    fn test_ensure_branch_creates_new() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let result = backend.ensure_branch("feature/test").unwrap();
        assert_eq!(result.name, "feature/test");
        assert!(result.created);
        assert!(backend.branch_exists("feature/test"));
    }

    #[test]
    fn test_ensure_branch_idempotent() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let result1 = backend.ensure_branch("feature/test").unwrap();
        assert!(result1.created);

        let result2 = backend.ensure_branch("feature/test").unwrap();
        assert!(!result2.created);
        assert_eq!(result1.base_commit, result2.base_commit);
    }

    #[test]
    fn test_branch_create_from_head() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let result = backend.branch("dev", None).unwrap();
        assert_eq!(result.name, "dev");
        assert!(result.created);
        assert!(backend.branch_exists("dev"));
    }

    #[test]
    fn test_branch_create_from_ref() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let head_hash = backend.head().unwrap();
        let result = backend.branch("feature", Some(&head_hash)).unwrap();
        assert_eq!(result.name, "feature");
        assert_eq!(result.base_commit, head_hash);
    }

    #[test]
    fn test_branch_exists() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        assert!(backend.branch_exists("main") || backend.branch_exists("master"));
        assert!(!backend.branch_exists("nonexistent"));

        backend.branch("test-branch", None).unwrap();
        assert!(backend.branch_exists("test-branch"));
    }

    #[test]
    fn test_checkout_branch() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        backend.branch("feature", None).unwrap();
        backend.checkout("feature").unwrap();

        let current = backend.current_branch().unwrap();
        assert_eq!(current, "feature");
    }

    #[test]
    fn test_checkout_nonexistent_fails() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let result = backend.checkout("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_current_branch() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let branch = backend.current_branch().unwrap();
        assert!(branch == "main" || branch == "master");
    }

    #[test]
    fn test_delete_branch() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        backend.branch("to-delete", None).unwrap();
        assert!(backend.branch_exists("to-delete"));

        backend.delete_branch("to-delete", true).unwrap();
        assert!(!backend.branch_exists("to-delete"));
    }

    #[test]
    fn test_delete_nonexistent_branch_fails() {
        let (_temp_dir, backend) = create_test_repo();

        let result = backend.delete_branch("nonexistent", false);
        assert!(result.is_err());
    }
}
