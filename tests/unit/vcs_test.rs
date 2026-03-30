//! Unit tests for the VCS primitive.
//!
//! Tests VCS operation dispatching through the git interface.

use chrono::Utc;
use sage_lore::primitives::vcs::{
    BranchResult, CommitResult, DiffResult, DiffScope, ForceMode, GitBackend, GitError,
    LogEntry, MergeResult, StashEntry, StashRef, Status,
};
use sage_lore::scroll::error::ExecutionError;
use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::interfaces::vcs::VcsInterface;
use sage_lore::scroll::interfaces::InterfaceDispatch;
use sage_lore::scroll::schema::{OnFail, Step, VcsOperation, VcsParams, VcsStep};
use std::sync::Arc;

// ============================================================================
// Mock GitBackend for Testing
// ============================================================================

/// Mock git backend that returns predefined results.
struct MockGitBackend {
    current_branch_name: String,
    should_fail: bool,
}

impl MockGitBackend {
    fn new() -> Self {
        Self {
            current_branch_name: "main".to_string(),
            should_fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            current_branch_name: "main".to_string(),
            should_fail: true,
        }
    }

    fn on_branch(branch: &str) -> Self {
        Self {
            current_branch_name: branch.to_string(),
            should_fail: false,
        }
    }
}

impl GitBackend for MockGitBackend {
    fn ensure_branch(&self, name: &str) -> Result<BranchResult, GitError> {
        if self.should_fail {
            return Err(GitError::BranchNotFound(name.to_string()));
        }
        Ok(BranchResult {
            name: name.to_string(),
            created: true,
            base_commit: "abc1234567890".to_string(),
            tracking: None,
        })
    }

    fn branch(&self, name: &str, _start_point: Option<&str>) -> Result<BranchResult, GitError> {
        if self.should_fail {
            return Err(GitError::BranchNotFound(name.to_string()));
        }
        Ok(BranchResult {
            name: name.to_string(),
            created: true,
            base_commit: "abc1234567890".to_string(),
            tracking: None,
        })
    }

    fn checkout(&self, ref_name: &str) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::BranchNotFound(ref_name.to_string()));
        }
        Ok(())
    }

    fn current_branch(&self) -> Result<String, GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(self.current_branch_name.clone())
    }

    fn branch_exists(&self, _name: &str) -> bool {
        !self.should_fail
    }

    fn delete_branch(&self, name: &str, _force: bool) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::BranchNotFound(name.to_string()));
        }
        Ok(())
    }

    fn stage(&self, _paths: &[&str]) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(())
    }

    fn stage_all(&self) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(())
    }

    fn unstage(&self, _paths: &[&str]) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(())
    }

    fn commit(&self, message: &str, _paths: Option<&[&str]>) -> Result<CommitResult, GitError> {
        if self.should_fail {
            return Err(GitError::DirtyWorkingTree);
        }
        Ok(CommitResult {
            commit_hash: "abc1234567890def1234567890abc123456789de".to_string(),
            short_hash: "abc1234".to_string(),
            message: message.to_string(),
            author: "Test Author".to_string(),
            timestamp: Utc::now(),
            files_changed: 1,
            insertions: 10,
            deletions: 5,
        })
    }

    fn merge(&self, branch: &str) -> Result<MergeResult, GitError> {
        if self.should_fail {
            return Err(GitError::MergeConflict(vec!["conflict.txt".to_string()]));
        }
        Ok(MergeResult {
            merge_commit: Some("def456".to_string()),
            fast_forward: false,
            conflicts: vec![],
            files_changed: 3,
        })
    }

    fn squash(&self, _branch: &str) -> Result<MergeResult, GitError> {
        if self.should_fail {
            return Err(GitError::MergeConflict(vec!["conflict.txt".to_string()]));
        }
        Ok(MergeResult {
            merge_commit: None,
            fast_forward: false,
            conflicts: vec![],
            files_changed: 5,
        })
    }

    fn abort_merge(&self) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(())
    }

    fn push(&self, _set_upstream: bool, _force: ForceMode) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::PushRejected("Remote has new commits".to_string()));
        }
        Ok(())
    }

    fn fetch(&self, _remote: Option<&str>) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::RemoteNotFound("origin".to_string()));
        }
        Ok(())
    }

    fn pull(&self, _remote: Option<&str>, _branch: Option<&str>) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::MergeConflict(vec!["conflict.txt".to_string()]));
        }
        Ok(())
    }

    fn pr_branch_ready(&self, _branch: &str, _base: &str) -> Result<bool, GitError> {
        Ok(!self.should_fail)
    }

    fn status(&self) -> Result<Status, GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(Status {
            branch: self.current_branch_name.clone(),
            ahead: 0,
            behind: 0,
            staged: vec![],
            unstaged: vec![],
            untracked: vec![],
            conflicted: vec![],
            clean: true,
        })
    }

    fn diff(&self, _scope: DiffScope) -> Result<DiffResult, GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(DiffResult {
            files: vec![],
            total_insertions: 10,
            total_deletions: 5,
        })
    }

    fn log(&self, count: usize) -> Result<Vec<LogEntry>, GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        let mut entries = vec![];
        for i in 0..count.min(3) {
            entries.push(LogEntry {
                commit_hash: format!("abc123{:034}", i),
                short_hash: format!("abc{}", i),
                message: format!("Commit message {}", i),
                author: "Test Author".to_string(),
                author_email: "test@example.com".to_string(),
                timestamp: Utc::now(),
                parents: vec![],
            });
        }
        Ok(entries)
    }

    fn stash_push(&self, message: &str) -> Result<StashRef, GitError> {
        if self.should_fail {
            return Err(GitError::DirtyWorkingTree);
        }
        Ok(StashRef {
            index: 0,
            message: message.to_string(),
            commit: "stash123".to_string(),
        })
    }

    fn stash_pop(&self, _index: Option<usize>) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::MergeConflict(vec!["conflict.txt".to_string()]));
        }
        Ok(())
    }

    fn stash_list(&self) -> Result<Vec<StashEntry>, GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(vec![StashEntry {
            index: 0,
            message: "WIP: test stash".to_string(),
            branch: "main".to_string(),
            timestamp: Utc::now(),
        }])
    }

    fn reset_hard(&self, _commit: &str) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::CommitNotFound("invalid".to_string()));
        }
        Ok(())
    }

    fn reset_soft(&self, _commit: &str) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::CommitNotFound("invalid".to_string()));
        }
        Ok(())
    }

    fn head(&self) -> Result<String, GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok("abc1234567890def1234567890abc123456789de".to_string())
    }

    fn head_short(&self) -> Result<String, GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok("abc1234".to_string())
    }

    fn resolve_ref(&self, ref_name: &str) -> Result<String, GitError> {
        if self.should_fail {
            return Err(GitError::CommitNotFound(ref_name.to_string()));
        }
        Ok("abc1234567890def1234567890abc123456789de".to_string())
    }

    fn tag(&self, _name: &str, _message: Option<&str>) -> Result<(), GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(())
    }

    fn list_tags(&self) -> Result<Vec<String>, GitError> {
        if self.should_fail {
            return Err(GitError::NotARepository);
        }
        Ok(vec!["v1.0.0".to_string(), "v1.1.0".to_string()])
    }
}

// ============================================================================
// VcsInterface Dispatch Tests
// ============================================================================

#[tokio::test]
async fn test_interface_status() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("status", &None).await;
    assert!(result.is_ok(), "status dispatch failed: {:?}", result.err());

    let status: Status = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(status.branch, "main");
    assert!(status.clean);
}

#[tokio::test]
async fn test_interface_commit() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"message: "Test commit""#).unwrap();
    let result = interface.dispatch("commit", &Some(params)).await;
    assert!(result.is_ok(), "commit dispatch failed: {:?}", result.err());

    let commit: CommitResult = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(commit.message, "Test commit");
    assert_eq!(commit.short_hash, "abc1234");
}

#[tokio::test]
async fn test_interface_commit_requires_message() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("commit", &None).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ExecutionError::MissingParameter(_)));
}

#[tokio::test]
async fn test_interface_diff() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("diff", &None).await;
    assert!(result.is_ok(), "diff dispatch failed: {:?}", result.err());

    let diff: DiffResult = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(diff.total_insertions, 10);
    assert_eq!(diff.total_deletions, 5);
}

#[tokio::test]
async fn test_interface_diff_with_scope() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"scope: "staged""#).unwrap();
    let result = interface.dispatch("diff", &Some(params)).await;
    assert!(result.is_ok(), "diff with scope failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_current_branch() {
    let backend = Arc::new(MockGitBackend::on_branch("feature/test"));
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("current_branch", &None).await;
    assert!(result.is_ok(), "current_branch failed: {:?}", result.err());

    let branch: String = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(branch, "feature/test");
}

#[tokio::test]
async fn test_interface_ensure_branch() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"name: "feature/new""#).unwrap();
    let result = interface.dispatch("ensure_branch", &Some(params)).await;
    assert!(result.is_ok(), "ensure_branch failed: {:?}", result.err());

    let branch: BranchResult = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(branch.name, "feature/new");
    assert!(branch.created);
}

#[tokio::test]
async fn test_interface_checkout() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"branch: "develop""#).unwrap();
    let result = interface.dispatch("checkout", &Some(params)).await;
    assert!(result.is_ok(), "checkout failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_checkout_requires_branch() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("checkout", &None).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ExecutionError::MissingParameter(_)));
}

#[tokio::test]
async fn test_interface_stage_all() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("stage_all", &None).await;
    assert!(result.is_ok(), "stage_all failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_stage_files() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"files: ["src/main.rs", "Cargo.toml"]"#).unwrap();
    let result = interface.dispatch("stage", &Some(params)).await;
    assert!(result.is_ok(), "stage failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_unstage() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"files: ["src/main.rs"]"#).unwrap();
    let result = interface.dispatch("unstage", &Some(params)).await;
    assert!(result.is_ok(), "unstage failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_log() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"count: 5"#).unwrap();
    let result = interface.dispatch("log", &Some(params)).await;
    assert!(result.is_ok(), "log failed: {:?}", result.err());

    let entries: Vec<LogEntry> = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(entries.len(), 3); // Mock returns max 3
}

#[tokio::test]
async fn test_interface_push() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("push", &None).await;
    assert!(result.is_ok(), "push failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_push_with_upstream() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"set_upstream: true"#).unwrap();
    let result = interface.dispatch("push", &Some(params)).await;
    assert!(result.is_ok(), "push with upstream failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_fetch() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("fetch", &None).await;
    assert!(result.is_ok(), "fetch failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_pull() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("pull", &None).await;
    assert!(result.is_ok(), "pull failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_merge() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"branch: "feature/merge-me""#).unwrap();
    let result = interface.dispatch("merge", &Some(params)).await;
    assert!(result.is_ok(), "merge failed: {:?}", result.err());

    let merge: MergeResult = serde_json::from_value(result.unwrap()).unwrap();
    assert!(merge.conflicts.is_empty());
}

#[tokio::test]
async fn test_interface_squash() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"branch: "feature/squash-me""#).unwrap();
    let result = interface.dispatch("squash", &Some(params)).await;
    assert!(result.is_ok(), "squash failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_abort_merge() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("abort_merge", &None).await;
    assert!(result.is_ok(), "abort_merge failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_stash_push() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"message: "WIP: saving work""#).unwrap();
    let result = interface.dispatch("stash_push", &Some(params)).await;
    assert!(result.is_ok(), "stash_push failed: {:?}", result.err());

    let stash: StashRef = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(stash.message, "WIP: saving work");
}

#[tokio::test]
async fn test_interface_stash_pop() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("stash_pop", &None).await;
    assert!(result.is_ok(), "stash_pop failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_stash_list() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("stash_list", &None).await;
    assert!(result.is_ok(), "stash_list failed: {:?}", result.err());

    let stashes: Vec<StashEntry> = serde_json::from_value(result.unwrap()).unwrap();
    assert!(!stashes.is_empty());
}

#[tokio::test]
async fn test_interface_reset_hard() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"target: "HEAD~1""#).unwrap();
    let result = interface.dispatch("reset_hard", &Some(params)).await;
    assert!(result.is_ok(), "reset_hard failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_reset_soft() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"target: "HEAD~1""#).unwrap();
    let result = interface.dispatch("reset_soft", &Some(params)).await;
    assert!(result.is_ok(), "reset_soft failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_head() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("head", &None).await;
    assert!(result.is_ok(), "head failed: {:?}", result.err());

    let sha: String = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(sha.len(), 40); // Full SHA
}

#[tokio::test]
async fn test_interface_head_short() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("head_short", &None).await;
    assert!(result.is_ok(), "head_short failed: {:?}", result.err());

    let sha: String = serde_json::from_value(result.unwrap()).unwrap();
    assert_eq!(sha.len(), 7); // Short SHA
}

#[tokio::test]
async fn test_interface_resolve_ref() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"ref: "HEAD""#).unwrap();
    let result = interface.dispatch("resolve_ref", &Some(params)).await;
    assert!(result.is_ok(), "resolve_ref failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_tag() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"name: "v1.2.0""#).unwrap();
    let result = interface.dispatch("tag", &Some(params)).await;
    assert!(result.is_ok(), "tag failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_tag_with_message() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"
name: "v1.2.0"
message: "Release version 1.2.0"
"#).unwrap();
    let result = interface.dispatch("tag", &Some(params)).await;
    assert!(result.is_ok(), "tag with message failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_list_tags() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("list_tags", &None).await;
    assert!(result.is_ok(), "list_tags failed: {:?}", result.err());

    let tags: Vec<String> = serde_json::from_value(result.unwrap()).unwrap();
    assert!(tags.contains(&"v1.0.0".to_string()));
}

#[tokio::test]
async fn test_interface_branch_exists() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"name: "main""#).unwrap();
    let result = interface.dispatch("branch_exists", &Some(params)).await;
    assert!(result.is_ok(), "branch_exists failed: {:?}", result.err());

    let exists: bool = serde_json::from_value(result.unwrap()).unwrap();
    assert!(exists);
}

#[tokio::test]
async fn test_interface_delete_branch() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"name: "feature/old""#).unwrap();
    let result = interface.dispatch("delete_branch", &Some(params)).await;
    assert!(result.is_ok(), "delete_branch failed: {:?}", result.err());
}

#[tokio::test]
async fn test_interface_pr_branch_ready() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let params = serde_yaml::from_str(r#"
branch: "feature/ready"
base: "main"
"#).unwrap();
    let result = interface.dispatch("pr_branch_ready", &Some(params)).await;
    assert!(result.is_ok(), "pr_branch_ready failed: {:?}", result.err());

    let ready: bool = serde_json::from_value(result.unwrap()).unwrap();
    assert!(ready);
}

#[tokio::test]
async fn test_interface_no_backend() {
    let interface = VcsInterface::new(); // No backend

    let result = interface.dispatch("status", &None).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ExecutionError::NotImplemented(_)));
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_interface_failing_backend() {
    let backend = Arc::new(MockGitBackend::failing());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("status", &None).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ExecutionError::InvocationError(_)));
}

#[tokio::test]
async fn test_interface_unknown_method() {
    let backend = Arc::new(MockGitBackend::new());
    let interface = VcsInterface::with_backend(backend);

    let result = interface.dispatch("nonexistent_method", &None).await;
    assert!(result.is_err());
}

// ============================================================================
// Step Dispatch Tests (Full Executor Path)
// ============================================================================

#[tokio::test]
async fn test_step_vcs_commit_requires_message() {
    let mut executor = Executor::for_testing();

    let step = Step::Vcs(VcsStep {
        vcs: VcsParams {
            operation: VcsOperation::Commit,
            message: None,
            files: None,
            branch: None,
            name: None,
            set_upstream: None,
            scope: None,
            remote: None,
            target: None,
        },
        output: Some("commit_result".to_string()),
        on_fail: OnFail::Halt,
    });

    let result = executor.execute_step(&step).await;
    assert!(result.is_err());

    if let Err(ExecutionError::MissingParameter(param)) = result {
        assert_eq!(param, "message required for commit operation");
    } else {
        panic!("Expected MissingParameter error, got: {:?}", result);
    }
}

#[tokio::test]
async fn test_step_vcs_checkout_requires_branch() {
    let mut executor = Executor::for_testing();

    let step = Step::Vcs(VcsStep {
        vcs: VcsParams {
            operation: VcsOperation::Checkout,
            message: None,
            files: None,
            branch: None,
            name: None,
            set_upstream: None,
            scope: None,
            remote: None,
            target: None,
        },
        output: Some("checkout_result".to_string()),
        on_fail: OnFail::Halt,
    });

    let result = executor.execute_step(&step).await;
    assert!(result.is_err());

    if let Err(ExecutionError::MissingParameter(param)) = result {
        assert_eq!(param, "branch required for checkout operation");
    } else {
        panic!("Expected MissingParameter error, got: {:?}", result);
    }
}

#[tokio::test]
async fn test_step_vcs_on_fail_continue() {
    let mut executor = Executor::for_testing();

    let step = Step::Vcs(VcsStep {
        vcs: VcsParams {
            operation: VcsOperation::Status,
            message: None,
            files: None,
            branch: None,
            name: None,
            set_upstream: None,
            scope: None,
            remote: None,
            target: None,
        },
        output: Some("status_result".to_string()),
        on_fail: OnFail::Continue,
    });

    let result = executor.execute_step(&step).await;

    // With OnFail::Continue, the error should be caught and null returned
    assert!(result.is_ok());

    // The output variable should be set to null
    let output = executor.context().get_variable("status_result");
    assert!(output.is_some());
    assert_eq!(output.unwrap(), &serde_json::Value::Null);
}
