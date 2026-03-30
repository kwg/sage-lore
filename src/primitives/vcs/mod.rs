// SPDX-License-Identifier: MIT
//! Git primitive for SAGE scrolls.
//!
//! This module provides a safe, consistent interface for git operations
//! including branch management, commits, merges, and recovery operations.
//! Security checks are integrated at gate points (commit, merge, PR).
//!
//! ## Architecture
//!
//! The git primitive uses a hybrid backend approach:
//! - **Local operations** (commit, branch, status, diff, log, reset) use the `git2` crate
//!   for speed and no PATH dependency
//! - **Remote operations** (push, fetch, pull, clone) use CLI for credential helper support
//!
//! ## Security Integration
//!
//! Git operations integrate with the `secure` primitive at gate points:
//! - Pre-commit: Secret detection on staged content
//! - Pre-merge: Full security suite (secrets + CVE scan + SAST per policy)

// Public modules
pub mod types;
pub mod trait_def;
pub mod backend;

// Private implementation modules
mod branch;
mod stage;
mod commit;
mod remote;
mod merge;
mod stash;
mod reset;
mod tag;
mod diff;
mod status;
mod refs;
mod impl_backend;

// Re-export public types and traits
pub use backend::Git2Backend;
pub use trait_def::GitBackend;
pub use types::*;
