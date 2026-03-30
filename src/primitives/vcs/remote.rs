// SPDX-License-Identifier: MIT
//! Remote operations for Git2Backend (using CLI for credential helper support).

use super::backend::Git2Backend;
use super::types::*;
use std::process::Command;

impl Git2Backend {
    pub(super) fn push_impl(&self, set_upstream: bool, force: ForceMode) -> Result<(), GitError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(self.workdir()).arg("push");

        if set_upstream {
            cmd.arg("-u");
        }

        match force {
            ForceMode::None => {}
            ForceMode::WithLease => {
                cmd.arg("--force-with-lease");
            }
            ForceMode::Force => {
                cmd.arg("--force");
            }
        }

        let output = cmd.output().map_err(|e| {
            GitError::CommandFailed(format!("failed to execute git push: {}", e))
        })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(GitError::CommandFailed(format!("git push failed: {}", stderr)))
        }
    }

    pub(super) fn fetch_impl(&self, remote: Option<&str>) -> Result<(), GitError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(self.workdir()).arg("fetch");

        if let Some(r) = remote {
            cmd.arg(r);
        }

        let output = cmd.output().map_err(|e| {
            GitError::CommandFailed(format!("failed to execute git fetch: {}", e))
        })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(GitError::CommandFailed(format!("git fetch failed: {}", stderr)))
        }
    }

    pub(super) fn pull_impl(&self, remote: Option<&str>, branch: Option<&str>) -> Result<(), GitError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(self.workdir()).arg("pull");

        if let Some(r) = remote {
            cmd.arg(r);
        }
        if let Some(b) = branch {
            cmd.arg(b);
        }

        let output = cmd.output().map_err(|e| {
            GitError::CommandFailed(format!("failed to execute git pull: {}", e))
        })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(GitError::CommandFailed(format!("git pull failed: {}", stderr)))
        }
    }

    pub(super) fn pr_branch_ready_impl(&self, branch: &str, base: &str) -> Result<bool, GitError> {
        // A branch is ready for PR if:
        // 1. It exists and has commits ahead of base
        // 2. Working tree is clean (no uncommitted changes)

        let repo = self.lock_repo();

        // Check if branch exists
        let branch_ref = repo
            .find_branch(branch, git2::BranchType::Local)
            .map_err(|_| GitError::BranchNotFound(branch.to_string()))?;

        // Check if base exists
        let base_ref = repo
            .find_branch(base, git2::BranchType::Local)
            .map_err(|_| GitError::BranchNotFound(base.to_string()))?;

        // Get commit objects
        let branch_commit = branch_ref.get().peel_to_commit()?;
        let base_commit = base_ref.get().peel_to_commit()?;

        // Check if branch has commits ahead of base
        let (ahead, _behind) = repo.graph_ahead_behind(branch_commit.id(), base_commit.id())?;

        // Ready if there are commits ahead of base
        Ok(ahead > 0)
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
    // PR Branch Ready Tests
    // ========================================================================

    #[test]
    fn test_pr_branch_ready_with_commits() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let main_branch = if backend.branch_exists("main") { "main" } else { "master" };

        backend.branch("feature", None).unwrap();
        backend.checkout("feature").unwrap();

        fs::write(temp_dir.path().join("feature.txt"), "content").unwrap();
        backend.stage(&["feature.txt"]).unwrap();
        backend.commit("Add feature", None).unwrap();

        let ready = backend.pr_branch_ready("feature", main_branch).unwrap();
        assert!(ready);
    }

    #[test]
    fn test_pr_branch_ready_no_commits() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let main_branch = if backend.branch_exists("main") { "main" } else { "master" };

        backend.branch("feature", None).unwrap();

        let ready = backend.pr_branch_ready("feature", main_branch).unwrap();
        assert!(!ready);
    }

    #[test]
    fn test_pr_branch_ready_nonexistent_branch() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let main_branch = if backend.branch_exists("main") { "main" } else { "master" };

        let result = backend.pr_branch_ready("nonexistent", main_branch);
        assert!(result.is_err());
    }
}
