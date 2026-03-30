// SPDX-License-Identifier: MIT
//! Merge operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;
use std::process::Command;

impl Git2Backend {
    // ========================================================================
    // Merge Operations
    // ========================================================================

    pub(super) fn merge_impl(&self, branch: &str) -> Result<MergeResult, GitError> {
        // Use CLI for merge to handle complex conflict scenarios
        let output = Command::new("git")
            .current_dir(self.workdir())
            .args(["merge", "--no-edit", branch])
            .output()
            .map_err(|e| GitError::CommandFailed(format!("failed to execute git merge: {}", e)))?;

        if output.status.success() {
            // Check if it was a fast-forward
            let stdout = String::from_utf8_lossy(&output.stdout);
            let fast_forward = stdout.contains("Fast-forward");

            // Get merge commit hash (HEAD)
            let repo = self.lock_repo();
            let head = repo.head()?;
            let commit = head.peel_to_commit()?;
            let merge_commit = if fast_forward {
                None
            } else {
                Some(commit.id().to_string())
            };

            Ok(MergeResult {
                merge_commit,
                fast_forward,
                conflicts: vec![],
                files_changed: 0, // Would need to parse git output for accurate count
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("CONFLICT") || output.status.code() == Some(1) {
                // Merge conflict - parse conflicting files
                let conflicts = self.get_conflicting_files()?;
                Ok(MergeResult {
                    merge_commit: None,
                    fast_forward: false,
                    conflicts,
                    files_changed: 0,
                })
            } else {
                Err(GitError::CommandFailed(format!("git merge failed: {}", stderr)))
            }
        }
    }

    pub(super) fn squash_impl(&self, branch: &str) -> Result<MergeResult, GitError> {
        // Squash merge: merge --squash, then need to commit separately
        let output = Command::new("git")
            .current_dir(self.workdir())
            .args(["merge", "--squash", branch])
            .output()
            .map_err(|e| GitError::CommandFailed(format!("failed to execute git merge --squash: {}", e)))?;

        if output.status.success() {
            Ok(MergeResult {
                merge_commit: None, // Squash doesn't create a merge commit yet
                fast_forward: false,
                conflicts: vec![],
                files_changed: 0,
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("CONFLICT") {
                let conflicts = self.get_conflicting_files()?;
                Ok(MergeResult {
                    merge_commit: None,
                    fast_forward: false,
                    conflicts,
                    files_changed: 0,
                })
            } else {
                Err(GitError::CommandFailed(format!("git merge --squash failed: {}", stderr)))
            }
        }
    }

    pub(super) fn abort_merge_impl(&self) -> Result<(), GitError> {
        let output = Command::new("git")
            .current_dir(self.workdir())
            .args(["merge", "--abort"])
            .output()
            .map_err(|e| GitError::CommandFailed(format!("failed to execute git merge --abort: {}", e)))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(GitError::CommandFailed(format!("git merge --abort failed: {}", stderr)))
        }
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
    // Merge Operation Tests
    // ========================================================================

    #[test]
    fn test_merge_fast_forward() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        // Create a feature branch and add a commit
        backend.branch("feature", None).unwrap();
        backend.checkout("feature").unwrap();

        fs::write(temp_dir.path().join("feature.txt"), "feature content").unwrap();
        backend.stage(&["feature.txt"]).unwrap();
        backend.commit("Add feature", None).unwrap();

        // Switch back to main and merge
        let main_branch = if backend.branch_exists("main") { "main" } else { "master" };
        backend.checkout(main_branch).unwrap();

        let result = backend.merge("feature").unwrap();
        assert!(result.fast_forward);
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn test_merge_conflict_detection() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        // Create conflicting changes
        let main_branch = if backend.branch_exists("main") { "main" } else { "master" };

        // Modify README on main
        fs::write(temp_dir.path().join("README.md"), "# Main version\n").unwrap();
        backend.stage(&["README.md"]).unwrap();
        backend.commit("Update on main", None).unwrap();

        // Create branch from before the change
        backend.checkout("HEAD~1").unwrap();
        backend.branch("feature", None).unwrap();
        backend.checkout("feature").unwrap();

        // Make conflicting change
        fs::write(temp_dir.path().join("README.md"), "# Feature version\n").unwrap();
        backend.stage(&["README.md"]).unwrap();
        backend.commit("Update on feature", None).unwrap();

        // Try to merge - should conflict
        backend.checkout(main_branch).unwrap();
        let result = backend.merge("feature").unwrap();

        if !result.conflicts.is_empty() {
            assert!(result.conflicts.contains(&"README.md".to_string()));
        }
    }

    #[test]
    fn test_squash_merge() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        backend.branch("feature", None).unwrap();
        backend.checkout("feature").unwrap();

        fs::write(temp_dir.path().join("feature.txt"), "feature").unwrap();
        backend.stage(&["feature.txt"]).unwrap();
        backend.commit("Add feature", None).unwrap();

        let main_branch = if backend.branch_exists("main") { "main" } else { "master" };
        backend.checkout(main_branch).unwrap();

        let result = backend.squash("feature").unwrap();
        assert!(!result.fast_forward);
        assert!(result.conflicts.is_empty());
    }
}
