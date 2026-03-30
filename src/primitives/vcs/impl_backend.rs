// SPDX-License-Identifier: MIT
//! GitBackend trait implementation - thin delegation layer.
//!
//! This module provides the GitBackend trait implementation for Git2Backend by delegating
//! to specialized implementation modules (branch, stage, commit, remote, merge, stash,
//! reset, tag, diff, status, refs).

use super::backend::Git2Backend;
use super::trait_def::GitBackend;
use super::types::*;

impl GitBackend for Git2Backend {
    // Branch operations (from branch.rs)
    fn ensure_branch(&self, name: &str) -> Result<BranchResult, GitError> {
        self.ensure_branch_impl(name)
    }

    fn branch(&self, name: &str, start_point: Option<&str>) -> Result<BranchResult, GitError> {
        self.branch_impl(name, start_point)
    }

    fn checkout(&self, ref_name: &str) -> Result<(), GitError> {
        self.checkout_impl(ref_name)
    }

    fn current_branch(&self) -> Result<String, GitError> {
        self.current_branch_impl()
    }

    fn branch_exists(&self, name: &str) -> bool {
        self.branch_exists_impl(name)
    }

    fn delete_branch(&self, name: &str, force: bool) -> Result<(), GitError> {
        self.delete_branch_impl(name, force)
    }

    // Staging operations (from stage.rs)
    fn stage(&self, paths: &[&str]) -> Result<(), GitError> {
        self.stage_impl(paths)
    }

    fn stage_all(&self) -> Result<(), GitError> {
        self.stage_all_impl()
    }

    fn unstage(&self, paths: &[&str]) -> Result<(), GitError> {
        self.unstage_impl(paths)
    }

    // Commit operations (from commit.rs)
    fn commit(&self, message: &str, paths: Option<&[&str]>) -> Result<CommitResult, GitError> {
        self.commit_impl(message, paths)
    }

    // Merge operations (from merge.rs)
    fn merge(&self, branch: &str) -> Result<MergeResult, GitError> {
        self.merge_impl(branch)
    }

    fn squash(&self, branch: &str) -> Result<MergeResult, GitError> {
        self.squash_impl(branch)
    }

    fn abort_merge(&self) -> Result<(), GitError> {
        self.abort_merge_impl()
    }

    // Remote operations (from remote.rs)
    fn push(&self, set_upstream: bool, force: ForceMode) -> Result<(), GitError> {
        self.push_impl(set_upstream, force)
    }

    fn fetch(&self, remote: Option<&str>) -> Result<(), GitError> {
        self.fetch_impl(remote)
    }

    fn pull(&self, remote: Option<&str>, branch: Option<&str>) -> Result<(), GitError> {
        self.pull_impl(remote, branch)
    }

    fn pr_branch_ready(&self, branch: &str, base: &str) -> Result<bool, GitError> {
        self.pr_branch_ready_impl(branch, base)
    }

    // Status operations (from status.rs)
    fn status(&self) -> Result<Status, GitError> {
        self.status_impl()
    }

    fn log(&self, count: usize) -> Result<Vec<LogEntry>, GitError> {
        self.log_impl(count)
    }

    // Diff operations (from diff.rs)
    fn diff(&self, scope: DiffScope) -> Result<DiffResult, GitError> {
        self.diff_impl(scope)
    }

    // Stash operations (from stash.rs)
    fn stash_push(&self, message: &str) -> Result<StashRef, GitError> {
        self.stash_push_impl(message)
    }

    fn stash_pop(&self, index: Option<usize>) -> Result<(), GitError> {
        self.stash_pop_impl(index)
    }

    fn stash_list(&self) -> Result<Vec<StashEntry>, GitError> {
        self.stash_list_impl()
    }

    // Reset operations (from reset.rs)
    fn reset_hard(&self, commit: &str) -> Result<(), GitError> {
        self.reset_hard_impl(commit)
    }

    fn reset_soft(&self, commit: &str) -> Result<(), GitError> {
        self.reset_soft_impl(commit)
    }

    // Reference queries (from refs.rs)
    fn head(&self) -> Result<String, GitError> {
        self.head_impl()
    }

    fn head_short(&self) -> Result<String, GitError> {
        self.head_short_impl()
    }

    fn resolve_ref(&self, ref_name: &str) -> Result<String, GitError> {
        self.resolve_ref_impl(ref_name)
    }

    // Tag operations (from tag.rs)
    fn tag(&self, name: &str, message: Option<&str>) -> Result<(), GitError> {
        self.tag_impl(name, message)
    }

    fn list_tags(&self) -> Result<Vec<String>, GitError> {
        self.list_tags_impl()
    }
}
