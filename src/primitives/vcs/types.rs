// SPDX-License-Identifier: MIT
//! Git types, errors, and common data structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Result of a commit operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResult {
    /// Full 40-character commit hash.
    pub commit_hash: String,
    /// 7-character abbreviated hash.
    pub short_hash: String,
    /// Commit message.
    pub message: String,
    /// Author name.
    pub author: String,
    /// Commit timestamp.
    pub timestamp: DateTime<Utc>,
    /// Number of files changed.
    pub files_changed: usize,
    /// Number of insertions.
    pub insertions: usize,
    /// Number of deletions.
    pub deletions: usize,
}

/// Result of branch operations like `ensure_branch` or `branch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchResult {
    /// Branch name.
    pub name: String,
    /// True if newly created, false if already existed.
    pub created: bool,
    /// Commit the branch points to.
    pub base_commit: String,
    /// Remote tracking branch if set.
    pub tracking: Option<String>,
}

/// Result of a merge operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    /// Merge commit hash, None for fast-forward merges.
    pub merge_commit: Option<String>,
    /// True if this was a fast-forward merge.
    pub fast_forward: bool,
    /// Conflicting file paths (empty if clean merge).
    pub conflicts: Vec<String>,
    /// Number of files changed.
    pub files_changed: usize,
}

/// Repository status information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    /// Current branch name.
    pub branch: String,
    /// Commits ahead of upstream.
    pub ahead: usize,
    /// Commits behind upstream.
    pub behind: usize,
    /// Staged file changes.
    pub staged: Vec<FileStatus>,
    /// Unstaged file changes.
    pub unstaged: Vec<FileStatus>,
    /// Untracked file paths.
    pub untracked: Vec<String>,
    /// Conflicted file paths.
    pub conflicted: Vec<String>,
    /// True if working tree is clean (no changes at all).
    pub clean: bool,
}

/// Status of a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    /// File path relative to repository root.
    pub path: String,
    /// Type of change.
    pub status: FileStatusType,
}

/// Type of file status change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatusType {
    /// File was added.
    Added,
    /// File was modified.
    Modified,
    /// File was deleted.
    Deleted,
    /// File was renamed.
    Renamed,
    /// File was copied.
    Copied,
    /// File type changed (e.g., file to symlink).
    TypeChanged,
}

/// Scope for diff operations.
#[derive(Debug, Clone)]
pub enum DiffScope {
    /// Staged changes only (index vs HEAD).
    Staged,
    /// Unstaged changes (working tree vs index).
    Unstaged,
    /// All changes vs HEAD (working tree vs HEAD).
    Head,
    /// Changes between two commits.
    Commits { from: String, to: String },
}

/// Result of a diff operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    /// Per-file diff information.
    pub files: Vec<FileDiff>,
    /// Total insertions across all files.
    pub total_insertions: usize,
    /// Total deletions across all files.
    pub total_deletions: usize,
}

/// Diff information for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    /// File path.
    pub path: String,
    /// Old path for renames.
    pub old_path: Option<String>,
    /// Type of change.
    pub status: FileStatusType,
    /// Number of insertions.
    pub insertions: usize,
    /// Number of deletions.
    pub deletions: usize,
    /// Diff hunks.
    pub hunks: Vec<DiffHunk>,
}

/// A single diff hunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    /// Starting line in old file.
    pub old_start: usize,
    /// Number of lines in old file.
    pub old_lines: usize,
    /// Starting line in new file.
    pub new_start: usize,
    /// Number of lines in new file.
    pub new_lines: usize,
    /// Hunk content (unified diff format).
    pub content: String,
}

/// A single commit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Full 40-character commit hash.
    pub commit_hash: String,
    /// 7-character abbreviated hash.
    pub short_hash: String,
    /// Commit message.
    pub message: String,
    /// Author name.
    pub author: String,
    /// Author email.
    pub author_email: String,
    /// Commit timestamp.
    pub timestamp: DateTime<Utc>,
    /// Parent commit hashes.
    pub parents: Vec<String>,
}

/// Reference to a stash entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashRef {
    /// Stash index (stash@{index}).
    pub index: usize,
    /// Stash message.
    pub message: String,
    /// Commit hash of the stash.
    pub commit: String,
}

/// A stash list entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashEntry {
    /// Stash index.
    pub index: usize,
    /// Stash message.
    pub message: String,
    /// Branch the stash was created on.
    pub branch: String,
    /// When the stash was created.
    pub timestamp: DateTime<Utc>,
}

/// Result of a pull request creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResult {
    /// PR number.
    pub number: i64,
    /// Web URL for the PR.
    pub url: String,
    /// PR title.
    pub title: String,
    /// PR state: "open", "closed", or "merged".
    pub state: String,
    /// Head (source) branch.
    pub head_branch: String,
    /// Base (target) branch.
    pub base_branch: String,
    /// Whether the PR can be merged (None if not yet determined).
    pub mergeable: Option<bool>,
}

/// Force push mode.
///
/// Force push is dangerous. SAGE makes it explicit with this enum.
/// Raw `--force` is not available by default and requires explicit
/// policy opt-in via `.sage-lore/security/policy.yaml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForceMode {
    /// Default - fails if remote has new commits.
    #[default]
    None,
    /// Safe force - only if remote matches expected (--force-with-lease).
    WithLease,
    /// Dangerous raw force - requires explicit policy opt-in.
    Force,
}

/// Submodule dirty policy.
///
/// Configurable behavior when parent repo has dirty submodules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubmodulePolicy {
    /// Only commit parent, leave submodules dirty.
    Ignore,
    /// Warn but proceed (default).
    #[default]
    Warn,
    /// Refuse commit if submodules are dirty.
    Block,
    /// Commit submodules first, then parent.
    AutoCommit,
}

/// Git operation errors.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// Branch not found.
    #[error("Branch not found: {0}")]
    BranchNotFound(String),

    /// Commit not found.
    #[error("Commit not found: {0}")]
    CommitNotFound(String),

    /// Merge conflict occurred.
    #[error("Merge conflict in files: {0:?}")]
    MergeConflict(Vec<String>),

    /// Working tree has uncommitted changes.
    #[error("Working tree is dirty")]
    DirtyWorkingTree,

    /// Not inside a git repository.
    #[error("Not a git repository")]
    NotARepository,

    /// Push was rejected by remote.
    #[error("Push rejected: {0}")]
    PushRejected(String),

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Remote not found.
    #[error("Remote not found: {0}")]
    RemoteNotFound(String),

    /// Security check failed (secret detection, CVE scan, etc.).
    #[error("Security check failed: {0}")]
    SecurityCheckFailed(String),

    /// Force push not allowed by policy.
    #[error("Force push not allowed: {0}")]
    ForcePushNotAllowed(String),

    /// Submodules are dirty and policy forbids it.
    #[error("Dirty submodules: {0:?}")]
    DirtySubmodules(Vec<PathBuf>),

    /// Error from the git2 crate.
    #[error("Git2 error: {0}")]
    Git2(#[from] git2::Error),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Command execution error.
    #[error("Command failed: {0}")]
    CommandFailed(String),

    /// Invalid reference.
    #[error("Invalid reference: {0}")]
    InvalidRef(String),

    /// Stash not found.
    #[error("Stash not found: {0}")]
    StashNotFound(usize),

    /// Tag already exists.
    #[error("Tag already exists: {0}")]
    TagExists(String),
}

/// Record of a git operation call (for mock backend).
#[derive(Debug, Clone)]
pub enum GitCall {
    EnsureBranch { name: String },
    Branch { name: String, start_point: Option<String> },
    Checkout { ref_name: String },
    CurrentBranch,
    BranchExists { name: String },
    DeleteBranch { name: String, force: bool },
    Stage { paths: Vec<String> },
    StageAll,
    Unstage { paths: Vec<String> },
    Commit { message: String, paths: Option<Vec<String>> },
    Merge { branch: String },
    Squash { branch: String },
    AbortMerge,
    Push { set_upstream: bool, force: ForceMode },
    Fetch { remote: Option<String> },
    Pull { remote: Option<String>, branch: Option<String> },
    PrBranchReady { branch: String, base: String },
    Status,
    Diff { scope: String },
    Log { count: usize },
    StashPush { message: String },
    StashPop { index: Option<usize> },
    StashList,
    ResetHard { commit: String },
    ResetSoft { commit: String },
    Head,
    HeadShort,
    ResolveRef { ref_name: String },
    Tag { name: String, message: Option<String> },
    ListTags,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_force_mode_default() {
        assert_eq!(ForceMode::default(), ForceMode::None);
    }

    #[test]
    fn test_submodule_policy_default() {
        assert_eq!(SubmodulePolicy::default(), SubmodulePolicy::Warn);
    }

    #[test]
    fn test_file_status_type_equality() {
        assert_eq!(FileStatusType::Added, FileStatusType::Added);
        assert_ne!(FileStatusType::Added, FileStatusType::Modified);
    }

    #[test]
    fn test_git_error_display() {
        let err = GitError::BranchNotFound("feature/test".to_string());
        assert_eq!(err.to_string(), "Branch not found: feature/test");

        let err = GitError::MergeConflict(vec!["file1.rs".to_string(), "file2.rs".to_string()]);
        assert!(err.to_string().contains("file1.rs"));
    }

    #[test]
    fn test_diff_scope_variants() {
        let _staged = DiffScope::Staged;
        let _unstaged = DiffScope::Unstaged;
        let _head = DiffScope::Head;
        let _commits = DiffScope::Commits {
            from: "abc123".to_string(),
            to: "def456".to_string(),
        };
    }

    #[test]
    fn test_commit_result_serialization() {
        let result = CommitResult {
            commit_hash: "a".repeat(40),
            short_hash: "a".repeat(7),
            message: "Test commit".to_string(),
            author: "Test Author".to_string(),
            timestamp: Utc::now(),
            files_changed: 3,
            insertions: 10,
            deletions: 5,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: CommitResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.message, "Test commit");
        assert_eq!(deserialized.files_changed, 3);
    }

    #[test]
    fn test_status_serialization() {
        let status = Status {
            branch: "main".to_string(),
            ahead: 2,
            behind: 0,
            staged: vec![FileStatus {
                path: "src/lib.rs".to_string(),
                status: FileStatusType::Modified,
            }],
            unstaged: vec![],
            untracked: vec!["new_file.txt".to_string()],
            conflicted: vec![],
            clean: false,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("main"));
        assert!(json.contains("src/lib.rs"));
    }

    #[test]
    fn test_branch_result_serialization() {
        let result = BranchResult {
            name: "feature/test".to_string(),
            created: true,
            base_commit: "abc123def456".to_string(),
            tracking: Some("origin/feature/test".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: BranchResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.created);
        assert_eq!(deserialized.tracking, Some("origin/feature/test".to_string()));
    }

    #[test]
    fn test_submodule_policy_serialization() {
        let policy = SubmodulePolicy::AutoCommit;
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, "\"auto-commit\"");

        let deserialized: SubmodulePolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SubmodulePolicy::AutoCommit);
    }
}
