// SPDX-License-Identifier: MIT
//! Commit operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;
use std::path::Path;

impl Git2Backend {
    // ========================================================================
    // Commit Operations
    // ========================================================================

    pub(super) fn commit_impl(&self, message: &str, paths: Option<&[&str]>) -> Result<CommitResult, GitError> {
        // Stage specific paths if provided (needs separate lock scope)
        if let Some(paths) = paths {
            let repo = self.lock_repo();
            let mut index = repo.index()?;

            for path in paths {
                let full_path = self.workdir().join(path);
                if full_path.exists() {
                    index.add_path(Path::new(path))?;
                } else {
                    index.remove_path(Path::new(path))?;
                }
            }
            index.write()?;
            drop(repo); // Release lock before continuing
        }

        // Collect staged content for security scanning in a scoped block
        // so all borrows are released before the security check
        let staged_content = {
            let repo = self.lock_repo();
            let index = repo.index()?;
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            let diff = repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), None)?;

            let mut content = String::new();
            diff.foreach(
                &mut |_, _| true,
                None,
                None,
                Some(&mut |_delta, _hunk, line| {
                    if line.origin() == '+' || line.origin() == ' ' {
                        if let Ok(text) = std::str::from_utf8(line.content()) {
                            content.push_str(text);
                        }
                    }
                    true
                }),
            )?;
            content
            // repo, index, head_tree, diff all dropped here
        };

        // Security check at gate point (D15)
        let scan_result = self.secure().secret_detection(&staged_content)
            .map_err(|e| GitError::SecurityCheckFailed(e.to_string()))?;

        if scan_result.has_blockers() {
            let finding_msgs: Vec<String> = scan_result
                .findings
                .iter()
                .filter(|f| f.is_blocker())
                .map(|f| format!("{}: {}", f.location, f.description))
                .collect();
            return Err(GitError::SecurityCheckFailed(format!(
                "Secret detection found blocking issues:\n{}",
                finding_msgs.join("\n")
            )));
        }

        // Re-acquire lock for commit
        let repo = self.lock_repo();
        let mut index = repo.index()?;
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        // Re-calculate diff stats
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        let diff = repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), None)?;
        let stats = diff.stats()?;
        let files_changed = stats.files_changed();
        let insertions = stats.insertions();
        let deletions = stats.deletions();

        // Get parent commit(s)
        let parents: Vec<git2::Commit> = if let Ok(head) = repo.head() {
            vec![head.peel_to_commit()?]
        } else {
            vec![] // Initial commit
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        // Create the commit
        let sig = repo.signature()?;
        let commit_oid = repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            message,
            &tree,
            &parent_refs,
        )?;

        let commit = repo.find_commit(commit_oid)?;
        let timestamp = chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
            .unwrap_or_else(chrono::Utc::now);

        Ok(CommitResult {
            commit_hash: commit_oid.to_string(),
            short_hash: commit_oid.to_string()[..7].to_string(),
            message: message.to_string(),
            author: sig.name().unwrap_or("Unknown").to_string(),
            timestamp,
            files_changed,
            insertions,
            deletions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::GitBackend;
    use super::*;
    use crate::primitives::secure::{SecureBackend, ScanResult, ScanType, Finding, Severity, FindingType, Location};
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

    /// Mock SecureBackend that fails with a blocker
    struct MockSecureBackendWithBlocker;

    impl SecureBackend for MockSecureBackendWithBlocker {
        fn secret_detection(&self, _content: &str) -> Result<ScanResult, SecurityError> {
            Ok(ScanResult {
                passed: false,
                findings: vec![Finding {
                    severity: Severity::High,
                    finding_type: FindingType::Secret,
                    location: Location {
                        file: "test.txt".to_string(),
                        line: Some(1),
                        column: None,
                        snippet: Some("SECRET=abc123".to_string()),
                    },
                    description: "Hardcoded secret detected".to_string(),
                    remediation: "Remove the secret".to_string(),
                    rule_id: "test-rule".to_string(),
                    cve_id: None,
                    content_hash: None,
                }],
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
    // Commit Operation Tests
    // ========================================================================

    #[test]
    fn test_commit_basic() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("new.txt"), "content").unwrap();
        backend.stage(&["new.txt"]).unwrap();

        let result = backend.commit("Add new file", None).unwrap();
        assert_eq!(result.message, "Add new file");
        assert_eq!(result.author, "Test User");
        assert_eq!(result.files_changed, 1);
        assert!(result.commit_hash.len() == 40);
        assert_eq!(result.short_hash.len(), 7);
    }

    #[test]
    fn test_commit_with_paths() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();

        // Commit only file1.txt
        let result = backend.commit("Add file1", Some(&["file1.txt"])).unwrap();
        assert_eq!(result.files_changed, 1);

        let status = backend.status().unwrap();
        assert!(status.untracked.contains(&"file2.txt".to_string()));
    }

    #[test]
    fn test_commit_blocked_by_security() {
        // Need to create initial commit with passing backend first
        let (temp_dir, initial_backend) = create_test_repo();
        commit_initial(&initial_backend, &temp_dir);

        // Now recreate backend with blocking security
        drop(initial_backend);
        let secure = Arc::new(MockSecureBackendWithBlocker);
        let backend = Git2Backend::open(temp_dir.path(), secure).unwrap();

        fs::write(temp_dir.path().join("secret.txt"), "SECRET=abc123").unwrap();
        backend.stage(&["secret.txt"]).unwrap();

        let result = backend.commit("Add secret", None);
        assert!(result.is_err());
        match result {
            Err(GitError::SecurityCheckFailed(_)) => {},
            _ => panic!("Expected SecurityCheckFailed error"),
        }
    }
}
