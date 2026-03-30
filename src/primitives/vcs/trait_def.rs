// SPDX-License-Identifier: MIT
//! Git backend trait definition.

use super::types::*;

/// Git operations backend trait.
pub trait GitBackend: Send + Sync {
    fn ensure_branch(&self, name: &str) -> Result<BranchResult, GitError>;
    fn branch(&self, name: &str, start_point: Option<&str>) -> Result<BranchResult, GitError>;
    fn checkout(&self, ref_name: &str) -> Result<(), GitError>;
    fn current_branch(&self) -> Result<String, GitError>;
    fn branch_exists(&self, name: &str) -> bool;
    fn delete_branch(&self, name: &str, force: bool) -> Result<(), GitError>;
    
    fn stage(&self, paths: &[&str]) -> Result<(), GitError>;
    fn stage_all(&self) -> Result<(), GitError>;
    fn unstage(&self, paths: &[&str]) -> Result<(), GitError>;
    
    fn commit(&self, message: &str, paths: Option<&[&str]>) -> Result<CommitResult, GitError>;
    
    fn merge(&self, branch: &str) -> Result<MergeResult, GitError>;
    fn squash(&self, branch: &str) -> Result<MergeResult, GitError>;
    fn abort_merge(&self) -> Result<(), GitError>;
    
    fn push(&self, set_upstream: bool, force: ForceMode) -> Result<(), GitError>;
    fn fetch(&self, remote: Option<&str>) -> Result<(), GitError>;
    fn pull(&self, remote: Option<&str>, branch: Option<&str>) -> Result<(), GitError>;
    
    fn pr_branch_ready(&self, branch: &str, base: &str) -> Result<bool, GitError>;
    
    fn status(&self) -> Result<Status, GitError>;
    fn diff(&self, scope: DiffScope) -> Result<DiffResult, GitError>;
    fn log(&self, count: usize) -> Result<Vec<LogEntry>, GitError>;
    
    fn stash_push(&self, message: &str) -> Result<StashRef, GitError>;
    fn stash_pop(&self, index: Option<usize>) -> Result<(), GitError>;
    fn stash_list(&self) -> Result<Vec<StashEntry>, GitError>;
    
    fn reset_hard(&self, commit: &str) -> Result<(), GitError>;
    fn reset_soft(&self, commit: &str) -> Result<(), GitError>;
    
    fn head(&self) -> Result<String, GitError>;
    fn head_short(&self) -> Result<String, GitError>;
    fn resolve_ref(&self, ref_name: &str) -> Result<String, GitError>;
    
    fn tag(&self, name: &str, message: Option<&str>) -> Result<(), GitError>;
    fn list_tags(&self) -> Result<Vec<String>, GitError>;
}
