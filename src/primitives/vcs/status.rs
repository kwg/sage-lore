// SPDX-License-Identifier: MIT
//! Status and log operations for Git2Backend.

use super::backend::Git2Backend;
use super::types::*;

impl Git2Backend {
    // ========================================================================
    // Status Operations
    // ========================================================================

    pub(super) fn status_impl(&self) -> Result<Status, GitError> {
        let repo = self.lock_repo();

        let statuses = repo.statuses(Some(
            git2::StatusOptions::new()
                .include_untracked(true)
                .recurse_untracked_dirs(true)
                .include_ignored(false),
        ))?;

        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();
        let mut conflicted = Vec::new();

        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("").to_string();
            let status = entry.status();

            // Check for conflicts
            if status.is_conflicted() {
                conflicted.push(path.clone());
                continue;
            }

            // Staged changes (index)
            if status.is_index_new() {
                staged.push(FileStatus {
                    path: path.clone(),
                    status: FileStatusType::Added,
                });
            } else if status.is_index_modified() {
                staged.push(FileStatus {
                    path: path.clone(),
                    status: FileStatusType::Modified,
                });
            } else if status.is_index_deleted() {
                staged.push(FileStatus {
                    path: path.clone(),
                    status: FileStatusType::Deleted,
                });
            } else if status.is_index_renamed() {
                staged.push(FileStatus {
                    path: path.clone(),
                    status: FileStatusType::Renamed,
                });
            } else if status.is_index_typechange() {
                staged.push(FileStatus {
                    path: path.clone(),
                    status: FileStatusType::TypeChanged,
                });
            }

            // Unstaged changes (working tree)
            if status.is_wt_modified() {
                unstaged.push(FileStatus {
                    path: path.clone(),
                    status: FileStatusType::Modified,
                });
            } else if status.is_wt_deleted() {
                unstaged.push(FileStatus {
                    path: path.clone(),
                    status: FileStatusType::Deleted,
                });
            } else if status.is_wt_renamed() {
                unstaged.push(FileStatus {
                    path: path.clone(),
                    status: FileStatusType::Renamed,
                });
            } else if status.is_wt_typechange() {
                unstaged.push(FileStatus {
                    path: path.clone(),
                    status: FileStatusType::TypeChanged,
                });
            }

            // Untracked files
            if status.is_wt_new() {
                untracked.push(path);
            }
        }

        // Get current branch
        let branch = repo
            .head()
            .ok()
            .and_then(|h| {
                if h.is_branch() {
                    h.shorthand().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "HEAD".to_string());

        // Get ahead/behind counts
        let (ahead, behind) = if let Ok(head) = repo.head() {
            if let Ok(local_commit) = head.peel_to_commit() {
                if let Some(tracking) = Self::get_tracking_branch(&repo, &branch) {
                    if let Ok(upstream) = repo.find_branch(&tracking, git2::BranchType::Remote) {
                        if let Ok(upstream_commit) = upstream.get().peel_to_commit() {
                            Self::ahead_behind(&repo, local_commit.id(), upstream_commit.id())
                        } else {
                            (0, 0)
                        }
                    } else {
                        (0, 0)
                    }
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        let clean = staged.is_empty()
            && unstaged.is_empty()
            && untracked.is_empty()
            && conflicted.is_empty();

        Ok(Status {
            branch,
            ahead,
            behind,
            staged,
            unstaged,
            untracked,
            conflicted,
            clean,
        })
    }

    pub(super) fn log_impl(&self, count: usize) -> Result<Vec<LogEntry>, GitError> {
        let repo = self.lock_repo();

        let mut revwalk = repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)?;

        let mut entries = Vec::new();

        for (idx, oid) in revwalk.enumerate() {
            if idx >= count {
                break;
            }

            let oid = oid?;
            let commit = repo.find_commit(oid)?;

            let author = commit.author();
            let timestamp = chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
                .unwrap_or_else(chrono::Utc::now);

            let parents: Vec<String> = commit.parent_ids().map(|id| id.to_string()).collect();

            entries.push(LogEntry {
                commit_hash: oid.to_string(),
                short_hash: oid.to_string()[..7].to_string(),
                message: commit.message().unwrap_or("").to_string(),
                author: author.name().unwrap_or("Unknown").to_string(),
                author_email: author.email().unwrap_or("").to_string(),
                timestamp,
                parents,
            });
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
    // Status Operation Tests
    // ========================================================================

    #[test]
    fn test_status_clean() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        let status = backend.status().unwrap();
        assert!(status.clean);
        assert!(status.staged.is_empty());
        assert!(status.unstaged.is_empty());
        assert!(status.untracked.is_empty());
    }

    #[test]
    fn test_status_untracked() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("untracked.txt"), "content").unwrap();

        let status = backend.status().unwrap();
        assert!(!status.clean);
        assert!(status.untracked.contains(&"untracked.txt".to_string()));
    }

    #[test]
    fn test_status_modified() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("README.md"), "# Modified\n").unwrap();

        let status = backend.status().unwrap();
        assert!(!status.clean);
        assert!(status.unstaged.iter().any(|f| f.path == "README.md" && f.status == FileStatusType::Modified));
    }

    #[test]
    fn test_log() {
        let (temp_dir, backend) = create_test_repo();
        commit_initial(&backend, &temp_dir);

        fs::write(temp_dir.path().join("file.txt"), "content").unwrap();
        backend.stage(&["file.txt"]).unwrap();
        backend.commit("Second commit", None).unwrap();

        let log = backend.log(10).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].message, "Second commit");
        assert_eq!(log[1].message, "Initial commit");
    }
}
