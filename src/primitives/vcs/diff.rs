// SPDX-License-Identifier: MIT
//! Diff operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;

impl Git2Backend {
    // ========================================================================
    // Diff Operations
    // ========================================================================

    pub(super) fn diff_impl(&self, scope: DiffScope) -> Result<DiffResult, GitError> {
        let repo = self.lock_repo();

        let diff = match scope {
            DiffScope::Staged => {
                let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
                let index = repo.index()?;
                repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), None)?
            }
            DiffScope::Unstaged => {
                let index = repo.index()?;
                repo.diff_index_to_workdir(Some(&index), None)?
            }
            DiffScope::Head => {
                let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
                repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), None)?
            }
            DiffScope::Commits { ref from, ref to } => {
                let from_obj = repo
                    .revparse_single(from)
                    .map_err(|_| GitError::CommitNotFound(from.clone()))?;
                let to_obj = repo
                    .revparse_single(to)
                    .map_err(|_| GitError::CommitNotFound(to.clone()))?;

                let from_tree = from_obj.peel_to_tree()?;
                let to_tree = to_obj.peel_to_tree()?;

                repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)?
            }
        };

        let stats = diff.stats()?;
        let mut files: Vec<FileDiff> = Vec::new();

        // Process each delta (file change)
        for (delta_idx, delta) in diff.deltas().enumerate() {
            let new_file = delta.new_file();
            let old_file = delta.old_file();

            let path = new_file
                .path()
                .or_else(|| old_file.path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let old_path = if delta.status() == git2::Delta::Renamed {
                old_file.path().map(|p| p.to_string_lossy().to_string())
            } else {
                None
            };

            let status = Self::delta_to_status(delta.status());

            // Collect hunks for this file
            let mut hunks: Vec<DiffHunk> = Vec::new();
            let mut file_insertions = 0;
            let mut file_deletions = 0;

            if let Ok(Some(patch)) = git2::Patch::from_diff(&diff, delta_idx) {
                {
                    for hunk_idx in 0..patch.num_hunks() {
                        if let Ok((hunk, _)) = patch.hunk(hunk_idx) {
                            let mut content = String::new();

                            for line_idx in 0..patch.num_lines_in_hunk(hunk_idx).unwrap_or(0) {
                                if let Ok(line) = patch.line_in_hunk(hunk_idx, line_idx) {
                                    let origin = line.origin();
                                    if origin == '+' || origin == '-' || origin == ' ' {
                                        content.push(origin);
                                        if let Ok(text) = std::str::from_utf8(line.content()) {
                                            content.push_str(text);
                                        }
                                    }

                                    match origin {
                                        '+' => file_insertions += 1,
                                        '-' => file_deletions += 1,
                                        _ => {}
                                    }
                                }
                            }

                            hunks.push(DiffHunk {
                                old_start: hunk.old_start() as usize,
                                old_lines: hunk.old_lines() as usize,
                                new_start: hunk.new_start() as usize,
                                new_lines: hunk.new_lines() as usize,
                                content,
                            });
                        }
                    }
                }
            }

            files.push(FileDiff {
                path,
                old_path,
                status,
                insertions: file_insertions,
                deletions: file_deletions,
                hunks,
            });
        }

        Ok(DiffResult {
            files,
            total_insertions: stats.insertions(),
            total_deletions: stats.deletions(),
        })
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
    // Diff Operation Tests
    // ========================================================================

    #[test]
    fn test_diff_staged() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("new.txt"), "content\n").unwrap();
        backend.stage(&["new.txt"]).unwrap();

        let diff = backend.diff(DiffScope::Staged).unwrap();
        assert_eq!(diff.files.len(), 1);
        assert_eq!(diff.files[0].path, "new.txt");
        assert_eq!(diff.files[0].status, FileStatusType::Added);
    }

    #[test]
    fn test_diff_unstaged() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("README.md"), "# Modified\n").unwrap();

        let diff = backend.diff(DiffScope::Unstaged).unwrap();
        assert_eq!(diff.files.len(), 1);
        assert_eq!(diff.files[0].path, "README.md");
        assert_eq!(diff.files[0].status, FileStatusType::Modified);
    }
}
