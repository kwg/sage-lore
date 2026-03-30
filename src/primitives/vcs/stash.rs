// SPDX-License-Identifier: MIT
//! Stash operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;
use std::process::Command;

impl Git2Backend {
    // ========================================================================
    // Stash Operations
    // ========================================================================

    pub(super) fn stash_push_impl(&self, message: &str) -> Result<StashRef, GitError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(self.workdir()).args(["stash", "push"]);

        if !message.is_empty() {
            cmd.args(["-m", message]);
        }

        let output = cmd.output().map_err(|e| {
            GitError::CommandFailed(format!("failed to execute git stash push: {}", e))
        })?;

        if output.status.success() {
            // Get the stash commit hash
            let stash_output = Command::new("git")
                .current_dir(self.workdir())
                .args(["stash", "list", "-1", "--format=%H"])
                .output()
                .map_err(|e| GitError::CommandFailed(format!("failed to get stash info: {}", e)))?;

            let commit = String::from_utf8_lossy(&stash_output.stdout)
                .trim()
                .to_string();

            Ok(StashRef {
                index: 0,
                message: message.to_string(),
                commit,
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(GitError::CommandFailed(format!("git stash push failed: {}", stderr)))
        }
    }

    pub(super) fn stash_pop_impl(&self, index: Option<usize>) -> Result<(), GitError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(self.workdir()).args(["stash", "pop"]);

        if let Some(i) = index {
            cmd.arg(format!("stash@{{{}}}", i));
        }

        let output = cmd.output().map_err(|e| {
            GitError::CommandFailed(format!("failed to execute git stash pop: {}", e))
        })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(GitError::CommandFailed(format!("git stash pop failed: {}", stderr)))
        }
    }

    pub(super) fn stash_list_impl(&self) -> Result<Vec<StashEntry>, GitError> {
        let output = Command::new("git")
            .current_dir(self.workdir())
            .args(["stash", "list", "--format=%H|%s|%gd|%ct"])
            .output()
            .map_err(|e| GitError::CommandFailed(format!("failed to execute git stash list: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitError::CommandFailed(format!("git stash list failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut entries = Vec::new();

        for (index, line) in stdout.lines().enumerate() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 4 {
                let timestamp = parts[3]
                    .parse::<i64>()
                    .ok()
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                    .unwrap_or_else(chrono::Utc::now);

                // Extract branch from message like "WIP on branch: message"
                let message = parts[1];
                let branch = if message.starts_with("WIP on ") {
                    message
                        .strip_prefix("WIP on ")
                        .and_then(|s| s.split(':').next())
                        .unwrap_or("unknown")
                        .to_string()
                } else {
                    "unknown".to_string()
                };

                entries.push(StashEntry {
                    index,
                    message: message.to_string(),
                    branch,
                    timestamp,
                });
            }
        }

        Ok(entries)
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
    // Stash Operation Tests
    // ========================================================================

    #[test]
    fn test_stash_push() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        // Modify tracked file (stash requires tracked changes)
        fs::write(temp_dir.path().join("README.md"), "# Modified\nwork in progress").unwrap();

        let result = backend.stash_push("WIP").unwrap();
        assert_eq!(result.index, 0);
        assert_eq!(result.message, "WIP");

        // After stash, working tree should be clean
        let status = backend.status().unwrap();
        assert!(status.clean || status.unstaged.is_empty());
    }

    #[test]
    fn test_stash_list() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        // Modify tracked file
        fs::write(temp_dir.path().join("README.md"), "# Modified\nwork").unwrap();
        backend.stash_push("First stash").unwrap();

        let list = backend.stash_list().unwrap();
        assert!(!list.is_empty());
    }

    #[test]
    fn test_stash_pop() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        // Modify tracked file
        fs::write(temp_dir.path().join("README.md"), "# Modified\nwork in progress").unwrap();
        backend.stash_push("WIP").unwrap();

        let status1 = backend.status().unwrap();
        assert!(status1.clean || status1.unstaged.is_empty());

        backend.stash_pop(None).unwrap();

        let status2 = backend.status().unwrap();
        assert!(!status2.clean);
        // Verify the modification is back
        let content = fs::read_to_string(temp_dir.path().join("README.md")).unwrap();
        assert!(content.contains("work in progress"));
    }
}
