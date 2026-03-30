// SPDX-License-Identifier: MIT
//! Core types for filesystem operations.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::primitives::Finding;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during filesystem operations.
#[derive(Debug, thiserror::Error)]
pub enum FsError {
    /// Path escapes the sandbox (project root).
    #[error("Path escapes sandbox: {path} resolved to {resolved}")]
    SandboxEscape {
        /// The original path provided
        path: String,
        /// The resolved absolute path
        resolved: String,
    },

    /// Path is protected and cannot be written/deleted.
    #[error("Path is protected: {0}")]
    ProtectedPath(String),

    /// File extension is not in the allowed list for writes.
    #[error("Extension not allowed: {0}")]
    DisallowedExtension(String),

    /// Secrets were detected in the content being written.
    #[error("Secrets detected in content for {path}: {findings:?}")]
    SecretsInContent {
        /// The path being written to
        path: String,
        /// The security findings detected
        findings: Vec<Finding>,
    },

    /// Path is read-protected (opt-in protection, D29).
    #[error("Path is read-protected: {0}")]
    ReadProtected(String),

    /// Delete operation not allowed (based on delete policy).
    #[error("Delete not allowed: {0}")]
    DeleteNotAllowed(String),

    /// Invalid or malformed path.
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// Underlying I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Glob pattern error.
    #[error("Glob pattern error: {0}")]
    GlobPattern(#[from] glob::PatternError),
}

// ============================================================================
// Data Types
// ============================================================================

/// Metadata about a file or directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    /// Absolute path to the file
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Whether this is a regular file
    pub is_file: bool,
    /// Whether this is a directory
    pub is_dir: bool,
    /// Whether this is a symbolic link
    pub is_symlink: bool,
    /// Last modification time
    pub modified: Option<SystemTime>,
    /// Creation time (may not be available on all platforms)
    pub created: Option<SystemTime>,
}

impl FileMeta {
    /// Create metadata from a path by reading filesystem attributes.
    pub fn from_path(path: &Path) -> Result<Self, FsError> {
        let metadata = std::fs::symlink_metadata(path)?;

        Ok(Self {
            path: path.to_path_buf(),
            size: metadata.len(),
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            is_symlink: metadata.file_type().is_symlink(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
        })
    }
}

/// Delete policy configuration (D30).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeletePolicy {
    /// Apply same rules as write operations.
    #[default]
    SameAsWrite,
    /// Only allow deletion of files created by the current scroll.
    ScrollCreatedOnly,
    /// Disable all delete operations.
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delete_policy_default() {
        assert_eq!(DeletePolicy::default(), DeletePolicy::SameAsWrite);
    }

    #[test]
    fn test_file_meta_from_tempfile() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        let meta = FileMeta::from_path(&file_path).unwrap();
        assert!(meta.is_file);
        assert!(!meta.is_dir);
        assert!(!meta.is_symlink);
        assert_eq!(meta.size, 12); // "test content" = 12 bytes
    }

    #[test]
    fn test_file_meta_from_directory() {
        let temp_dir = tempfile::tempdir().unwrap();

        let meta = FileMeta::from_path(temp_dir.path()).unwrap();
        assert!(!meta.is_file);
        assert!(meta.is_dir);
        assert!(!meta.is_symlink);
    }

    #[test]
    fn test_fs_error_display() {
        let err = FsError::SandboxEscape {
            path: "../etc/passwd".to_string(),
            resolved: "/etc/passwd".to_string(),
        };
        assert!(err.to_string().contains("sandbox"));

        let err = FsError::ProtectedPath(".git/config".to_string());
        assert!(err.to_string().contains("protected"));

        let err = FsError::DisallowedExtension(".exe".to_string());
        assert!(err.to_string().contains("not allowed"));
    }
}
