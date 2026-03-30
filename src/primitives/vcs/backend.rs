// SPDX-License-Identifier: MIT
//! Git2Backend implementation structure and utilities.

use super::types::*;
use crate::primitives::secure::SecureBackend;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Production git backend using git2 for local operations and CLI for remote operations.
///
/// This hybrid approach provides:
/// - Fast local operations (commit, branch, status, diff, log, reset) via git2 crate
/// - Credential helper support for remote operations (push, fetch, pull) via CLI
///
/// Security checks are integrated at gate points (commit, merge, PR) via the secure backend.
///
/// The git2::Repository is wrapped in a Mutex to satisfy the Sync requirement of GitBackend.
/// While git2::Repository operations are generally not thread-safe, the Mutex ensures
/// exclusive access during each operation.
pub struct Git2Backend {
    /// The git2 repository handle, wrapped in Mutex for thread safety.
    repo: Mutex<git2::Repository>,
    /// Security backend for gate point checks.
    secure: Arc<dyn SecureBackend>,
    /// Repository working directory path.
    workdir: PathBuf,
}

impl Git2Backend {
    /// Open an existing repository at the given path.
    ///
    /// # Arguments
    /// * `path` - Path to the repository (can be workdir or .git dir)
    /// * `secure` - Security backend for gate point checks
    ///
    /// # Errors
    /// Returns `GitError::NotARepository` if the path is not a git repository.
    pub fn open(path: &Path, secure: Arc<dyn SecureBackend>) -> Result<Self, GitError> {
        let repo = git2::Repository::open(path)?;
        let workdir = repo
            .workdir()
            .ok_or(GitError::NotARepository)?
            .to_path_buf();
        Ok(Self {
            repo: Mutex::new(repo),
            secure,
            workdir,
        })
    }

    /// Open the repository containing the current directory.
    ///
    /// Searches upward from the current directory to find a git repository.
    pub fn open_from_cwd(secure: Arc<dyn SecureBackend>) -> Result<Self, GitError> {
        let repo = git2::Repository::open_from_env()?;
        let workdir = repo
            .workdir()
            .ok_or(GitError::NotARepository)?
            .to_path_buf();
        Ok(Self {
            repo: Mutex::new(repo),
            secure,
            workdir,
        })
    }

    /// Get the repository working directory.
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// Convert git2 delta to our FileStatusType.
    pub(super) fn delta_to_status(delta: git2::Delta) -> FileStatusType {
        match delta {
            git2::Delta::Added | git2::Delta::Untracked => FileStatusType::Added,
            git2::Delta::Deleted => FileStatusType::Deleted,
            git2::Delta::Modified => FileStatusType::Modified,
            git2::Delta::Renamed => FileStatusType::Renamed,
            git2::Delta::Copied => FileStatusType::Copied,
            git2::Delta::Typechange => FileStatusType::TypeChanged,
            _ => FileStatusType::Modified,
        }
    }

    /// Get the tracking branch info for a local branch.
    pub(super) fn get_tracking_branch(repo: &git2::Repository, branch_name: &str) -> Option<String> {
        let branch = repo.find_branch(branch_name, git2::BranchType::Local).ok()?;
        let upstream = branch.upstream().ok()?;
        upstream.name().ok().flatten().map(|s| s.to_string())
    }

    /// Count commits ahead/behind upstream.
    pub(super) fn ahead_behind(repo: &git2::Repository, local: git2::Oid, upstream: git2::Oid) -> (usize, usize) {
        repo.graph_ahead_behind(local, upstream)
            .unwrap_or((0, 0))
    }

    /// Lock the repository and return a guard. Panics if the mutex is poisoned.
    pub(super) fn lock_repo(&self) -> std::sync::MutexGuard<'_, git2::Repository> {
        self.repo.lock().expect("Git2Backend mutex poisoned")
    }

    /// Get list of conflicting files from the index.
    pub(super) fn get_conflicting_files(&self) -> Result<Vec<String>, GitError> {
        let repo = self.lock_repo();
        let index = repo.index()?;
        let mut conflicts = Vec::new();

        for entry in index.conflicts()? {
            let entry = entry?;
            // Any of ancestor, ours, theirs being present indicates a conflict
            if let Some(ours) = entry.our {
                let path = String::from_utf8_lossy(&ours.path).to_string();
                if !conflicts.contains(&path) {
                    conflicts.push(path);
                }
            } else if let Some(theirs) = entry.their {
                let path = String::from_utf8_lossy(&theirs.path).to_string();
                if !conflicts.contains(&path) {
                    conflicts.push(path);
                }
            }
        }

        Ok(conflicts)
    }

    /// Get the secure backend reference.
    pub(super) fn secure(&self) -> &Arc<dyn SecureBackend> {
        &self.secure
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::secure::{SecureBackend, ScanResult, ScanType};
    use crate::config::SecurityError;
    use std::sync::Arc;

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
            unimplemented!("audit not needed for git tests")
        }

        fn dependency_scan(&self, _manifest: &Path) -> Result<crate::primitives::secure::CveReport, SecurityError> {
            unimplemented!("dependency_scan not needed for git tests")
        }

        fn static_analysis(&self, _path: &Path) -> Result<crate::primitives::secure::SastReport, SecurityError> {
            unimplemented!("static_analysis not needed for git tests")
        }

        fn available_tools(&self) -> Vec<crate::primitives::secure::ToolStatus> {
            vec![]
        }
    }

    #[test]
    fn test_delta_to_status() {
        assert_eq!(
            Git2Backend::delta_to_status(git2::Delta::Added),
            FileStatusType::Added
        );
        assert_eq!(
            Git2Backend::delta_to_status(git2::Delta::Modified),
            FileStatusType::Modified
        );
        assert_eq!(
            Git2Backend::delta_to_status(git2::Delta::Deleted),
            FileStatusType::Deleted
        );
        assert_eq!(
            Git2Backend::delta_to_status(git2::Delta::Renamed),
            FileStatusType::Renamed
        );
        assert_eq!(
            Git2Backend::delta_to_status(git2::Delta::Copied),
            FileStatusType::Copied
        );
        assert_eq!(
            Git2Backend::delta_to_status(git2::Delta::Typechange),
            FileStatusType::TypeChanged
        );
        assert_eq!(
            Git2Backend::delta_to_status(git2::Delta::Untracked),
            FileStatusType::Added
        );
    }

    #[test]
    fn test_open_nonexistent_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let secure = Arc::new(MockSecureBackend);

        let result = Git2Backend::open(temp_dir.path(), secure);
        assert!(result.is_err());
    }

    #[test]
    fn test_open_valid_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp_dir.path()).unwrap();
        drop(repo);

        let secure = Arc::new(MockSecureBackend);
        let backend = Git2Backend::open(temp_dir.path(), secure).unwrap();

        assert_eq!(backend.workdir(), temp_dir.path());
    }

    #[test]
    fn test_workdir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(temp_dir.path()).unwrap();
        drop(repo);

        let secure = Arc::new(MockSecureBackend);
        let backend = Git2Backend::open(temp_dir.path(), secure).unwrap();

        assert_eq!(backend.workdir(), temp_dir.path());
    }
}
