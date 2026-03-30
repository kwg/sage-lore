// SPDX-License-Identifier: MIT
//! Filesystem backend trait and implementation.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::primitives::secure::SecureBackend;

use super::policy::FsPolicy;
use super::types::{DeletePolicy, FileMeta, FsError};

// ============================================================================
// Backend Trait
// ============================================================================

/// Backend trait for filesystem operations.
///
/// This trait defines the interface for all filesystem backends.
/// Implementations handle the actual file I/O while the security layer
/// validates paths and content before delegation.
///
/// # Security Model
///
/// The `SecureFsBackend` implementation wraps this trait with 5 security layers:
/// 1. Sandbox bounds checking
/// 2. Protected path validation
/// 3. Extension allow-listing
/// 4. Content scanning for secrets
/// 5. Symlink resolution
pub trait FsBackend: Send + Sync {
    /// Read file contents as text.
    ///
    /// # Errors
    ///
    /// Returns `FsError::Io` if the file cannot be read.
    fn read(&self, path: &Path) -> Result<String, FsError>;

    /// Write content to a file.
    ///
    /// Creates the file if it doesn't exist, truncates if it does.
    /// Parent directories are created as needed.
    ///
    /// # Errors
    ///
    /// Returns `FsError::Io` if the file cannot be written.
    fn write(&self, path: &Path, content: &str) -> Result<(), FsError>;

    /// Append content to a file.
    ///
    /// Creates the file if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns `FsError::Io` if the file cannot be written.
    fn append(&self, path: &Path, content: &str) -> Result<(), FsError>;

    /// Delete a file.
    ///
    /// # Errors
    ///
    /// Returns `FsError::Io` if the file cannot be deleted.
    fn delete(&self, path: &Path) -> Result<(), FsError>;

    /// Check if a path exists.
    fn exists(&self, path: &Path) -> bool;

    /// List directory contents with optional glob pattern.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory to list
    /// * `pattern` - Optional glob pattern to filter results (e.g., "*.rs")
    ///
    /// # Errors
    ///
    /// Returns `FsError::Io` if the directory cannot be read.
    fn list(&self, path: &Path, pattern: Option<&str>) -> Result<Vec<PathBuf>, FsError>;

    /// Create a directory and all parent directories.
    ///
    /// # Errors
    ///
    /// Returns `FsError::Io` if the directory cannot be created.
    fn mkdir(&self, path: &Path) -> Result<(), FsError>;

    /// Copy a file from source to destination.
    ///
    /// # Errors
    ///
    /// Returns `FsError::Io` if the file cannot be copied.
    fn copy(&self, src: &Path, dest: &Path) -> Result<(), FsError>;

    /// Move/rename a file.
    ///
    /// # Errors
    ///
    /// Returns `FsError::Io` if the file cannot be moved.
    fn rename(&self, src: &Path, dest: &Path) -> Result<(), FsError>;

    /// Get file metadata.
    ///
    /// # Errors
    ///
    /// Returns `FsError::Io` if metadata cannot be read.
    fn stat(&self, path: &Path) -> Result<FileMeta, FsError>;
}

// ============================================================================
// SecureFsBackend Implementation
// ============================================================================

/// Production filesystem backend with security checks.
///
/// This implementation wraps all filesystem operations with the 5-layer
/// security model:
///
/// 1. **Sandbox Bounds**: All paths validated to be within project root
/// 2. **Protected Paths**: Deny list that cannot be written/deleted
/// 3. **Allowed Extensions**: Allow list for write operations
/// 4. **Content Scanning**: All writes scanned for secrets via `SecureBackend`
/// 5. **Symlink Handling**: Symlinks resolved to real paths before validation to prevent escapes (D31)
///
/// # Thread Safety
///
/// This backend is `Send + Sync` and can be shared across threads.
/// The `SecureBackend` is held behind an `Arc` for shared access.
pub struct SecureFsBackend {
    /// Security policy for filesystem operations
    policy: FsPolicy,
    /// Secure backend for content scanning
    secure: Arc<dyn SecureBackend>,
    /// Project root directory (all operations bounded here)
    project_root: PathBuf,
}

impl SecureFsBackend {
    /// Create a new secure filesystem backend.
    ///
    /// # Arguments
    ///
    /// * `policy` - Filesystem security policy
    /// * `secure` - Backend for content scanning (secret detection)
    /// * `project_root` - Root directory for sandbox bounds
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::sync::Arc;
    /// use sage_lore::primitives::fs::{FsPolicy, SecureFsBackend};
    /// use sage_lore::primitives::secure::PolicyDrivenBackend;
    ///
    /// let policy = FsPolicy::new("/project".into());
    /// let secure = Arc::new(PolicyDrivenBackend::from_project("/project".as_ref())?);
    /// let backend = SecureFsBackend::new(policy, secure, "/project".into());
    /// ```
    pub fn new(policy: FsPolicy, secure: Arc<dyn SecureBackend>, project_root: PathBuf) -> Self {
        // Canonicalize project_root so sandbox checks work with absolute paths
        // from read_dir(). Without this, starts_with(".") vs absolute paths
        // silently drops all results.
        let project_root = project_root.canonicalize().unwrap_or(project_root);
        Self {
            policy,
            secure,
            project_root,
        }
    }

    /// Get a reference to the security policy.
    pub fn policy(&self) -> &FsPolicy {
        &self.policy
    }

    /// Get the project root directory.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Validate a path and return its resolved absolute path.
    ///
    /// This is a convenience wrapper around `policy.validate_path()`.
    fn validate_path(&self, path: &str) -> Result<PathBuf, FsError> {
        self.policy.validate_path(path)
    }

    /// Check if a path is protected for write/delete operations.
    fn check_write_protected(&self, path: &Path) -> Result<(), FsError> {
        if self.policy.is_protected(path) {
            return Err(FsError::ProtectedPath(path.display().to_string()));
        }
        Ok(())
    }

    /// Check if a path is read-protected — opt-in list that blocks reads on sensitive files (D29).
    fn check_read_protected(&self, path: &Path) -> Result<(), FsError> {
        if self.policy.is_read_protected(path) {
            return Err(FsError::ReadProtected(path.display().to_string()));
        }
        Ok(())
    }

    /// Check if a file extension is allowed for writing.
    fn check_extension(&self, path: &Path) -> Result<(), FsError> {
        if !self.policy.is_allowed_extension(path) {
            return Err(FsError::DisallowedExtension(path.display().to_string()));
        }
        Ok(())
    }

    /// Scan content for secrets before writing.
    ///
    /// Uses the `SecureBackend` for content scanning (Layer 4).
    fn scan_content(&self, path: &Path, content: &str) -> Result<(), FsError> {
        if !self.policy.scan_content {
            return Ok(());
        }

        // Use the secure backend to scan content
        match self.secure.secret_detection(content) {
            Ok(scan_result) => {
                if scan_result.has_findings() {
                    return Err(FsError::SecretsInContent {
                        path: path.display().to_string(),
                        findings: scan_result.findings,
                    });
                }
                Ok(())
            }
            Err(e) => {
                // Treat scan failures as protective - don't allow write if we can't scan
                Err(FsError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Content scan failed: {}", e),
                )))
            }
        }
    }

    /// Check delete policy and return error if delete is not allowed.
    fn check_delete_allowed(&self, path: &Path) -> Result<(), FsError> {
        match self.policy.delete_policy {
            DeletePolicy::Disabled => {
                return Err(FsError::DeleteNotAllowed(
                    "Delete operations are disabled".to_string(),
                ));
            }
            DeletePolicy::ScrollCreatedOnly => {
                // For now, we can't track which files were created by scrolls,
                // so this falls back to denying deletes unless we have tracking.
                // A full implementation would need a registry of created files.
                return Err(FsError::DeleteNotAllowed(
                    "Only scroll-created files can be deleted".to_string(),
                ));
            }
            DeletePolicy::SameAsWrite => {
                // Same rules as write - check protected paths
                self.check_write_protected(path)?;
            }
        }
        Ok(())
    }

    /// Ensure parent directories exist for a path.
    fn ensure_parent_dirs(&self, path: &Path) -> Result<(), FsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

impl FsBackend for SecureFsBackend {
    /// Read file contents as text.
    ///
    /// Security checks:
    /// - Layer 1: Sandbox bounds (path validation)
    /// - Layer 5: Symlink resolution (via canonicalize)
    /// - Read protection check — opt-in list blocking reads on sensitive files (D29)
    fn read(&self, path: &Path) -> Result<String, FsError> {
        // Convert to string path for validation
        let path_str = path.to_string_lossy();
        let resolved = self.validate_path(&path_str)?;

        // Check read protection (opt-in)
        self.check_read_protected(&resolved)?;

        // Read the file
        std::fs::read_to_string(&resolved).map_err(FsError::from)
    }

    /// Write content to a file.
    ///
    /// Security checks (all 5 layers):
    /// - Layer 1: Sandbox bounds (path validation)
    /// - Layer 2: Protected path check
    /// - Layer 3: Extension allow list
    /// - Layer 4: Content scanning for secrets
    /// - Layer 5: Symlink resolution (via canonicalize)
    fn write(&self, path: &Path, content: &str) -> Result<(), FsError> {
        let path_str = path.to_string_lossy();
        let resolved = self.validate_path(&path_str)?;

        // Layer 2: Protected path check
        self.check_write_protected(&resolved)?;

        // Layer 3: Extension check
        self.check_extension(&resolved)?;

        // Layer 4: Content scanning
        self.scan_content(&resolved, content)?;

        // Create parent directories if needed
        self.ensure_parent_dirs(&resolved)?;

        // Write the file
        std::fs::write(&resolved, content).map_err(FsError::from)
    }

    /// Append content to a file.
    ///
    /// Security checks: same as `write()`.
    fn append(&self, path: &Path, content: &str) -> Result<(), FsError> {
        let path_str = path.to_string_lossy();
        let resolved = self.validate_path(&path_str)?;

        // Layer 2: Protected path check
        self.check_write_protected(&resolved)?;

        // Layer 3: Extension check
        self.check_extension(&resolved)?;

        // Layer 4: Content scanning
        self.scan_content(&resolved, content)?;

        // Create parent directories if needed
        self.ensure_parent_dirs(&resolved)?;

        // Append to the file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&resolved)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    /// Delete a file.
    ///
    /// Security checks:
    /// - Layer 1: Sandbox bounds (path validation)
    /// - Layer 2: Protected path check
    /// - Layer 5: Symlink resolution
    /// - Delete policy check (D30)
    fn delete(&self, path: &Path) -> Result<(), FsError> {
        let path_str = path.to_string_lossy();
        let resolved = self.validate_path(&path_str)?;

        // Check delete policy (includes protected path check for SameAsWrite)
        self.check_delete_allowed(&resolved)?;

        // Delete the file
        std::fs::remove_file(&resolved).map_err(FsError::from)
    }

    /// Check if a path exists.
    ///
    /// Security checks:
    /// - Layer 1: Sandbox bounds (path validation)
    /// - Layer 5: Symlink resolution
    fn exists(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        match self.validate_path(&path_str) {
            Ok(resolved) => resolved.exists(),
            Err(_) => false, // Invalid paths don't exist
        }
    }

    /// List directory contents with optional glob pattern.
    ///
    /// Security checks:
    /// - Layer 1: Sandbox bounds (path validation)
    /// - Layer 5: Symlink resolution
    ///
    /// Returns paths relative to the directory being listed.
    fn list(&self, path: &Path, pattern: Option<&str>) -> Result<Vec<PathBuf>, FsError> {
        let path_str = path.to_string_lossy();
        let resolved = self.validate_path(&path_str)?;

        if !resolved.is_dir() {
            return Err(FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                format!("Not a directory: {}", resolved.display()),
            )));
        }

        let mut results = Vec::new();

        // Use glob if pattern provided, otherwise list directory
        if let Some(pattern) = pattern {
            // Create glob pattern
            let glob_pattern = resolved.join(pattern);
            let glob_str = glob_pattern.to_string_lossy();

            for entry in glob::glob(&glob_str)? {
                match entry {
                    Ok(entry_path) => {
                        // Validate each result is within sandbox
                        if entry_path.starts_with(&self.project_root) {
                            results.push(entry_path);
                        }
                    }
                    Err(_) => continue, // Skip unreadable entries
                }
            }
        } else {
            // List all entries in directory
            for entry in std::fs::read_dir(&resolved)? {
                let entry = entry?;
                let entry_path = entry.path();

                // Validate each result is within sandbox (should always be true)
                if entry_path.starts_with(&self.project_root) {
                    results.push(entry_path);
                }
            }
        }

        Ok(results)
    }

    /// Create a directory and all parent directories.
    ///
    /// Security checks:
    /// - Layer 1: Sandbox bounds (path validation)
    /// - Layer 2: Protected path check
    /// - Layer 5: Symlink resolution
    fn mkdir(&self, path: &Path) -> Result<(), FsError> {
        let path_str = path.to_string_lossy();
        let resolved = self.validate_path(&path_str)?;

        // Check protected paths
        self.check_write_protected(&resolved)?;

        // Create directory and parents
        std::fs::create_dir_all(&resolved).map_err(FsError::from)
    }

    /// Copy a file from source to destination.
    ///
    /// Security checks on both source and destination:
    /// - Layer 1: Sandbox bounds (path validation)
    /// - Layer 2: Protected path check (destination only)
    /// - Layer 3: Extension check (destination only)
    /// - Layer 4: Content scanning (reads source, scans before write)
    /// - Layer 5: Symlink resolution
    fn copy(&self, src: &Path, dest: &Path) -> Result<(), FsError> {
        // Validate source
        let src_str = src.to_string_lossy();
        let resolved_src = self.validate_path(&src_str)?;
        self.check_read_protected(&resolved_src)?;

        // Validate destination
        let dest_str = dest.to_string_lossy();
        let resolved_dest = self.validate_path(&dest_str)?;
        self.check_write_protected(&resolved_dest)?;
        self.check_extension(&resolved_dest)?;

        // Read source content for scanning
        let content = std::fs::read_to_string(&resolved_src)?;

        // Scan content before writing
        self.scan_content(&resolved_dest, &content)?;

        // Ensure parent directories exist
        self.ensure_parent_dirs(&resolved_dest)?;

        // Perform the copy
        std::fs::copy(&resolved_src, &resolved_dest)?;
        Ok(())
    }

    /// Move/rename a file.
    ///
    /// Security checks on both source and destination:
    /// - Layer 1: Sandbox bounds (path validation)
    /// - Layer 2: Protected path check (both source and destination)
    /// - Layer 3: Extension check (destination only)
    /// - Layer 5: Symlink resolution
    ///
    /// Note: Content is not re-scanned for move operations as the
    /// content itself doesn't change.
    fn rename(&self, src: &Path, dest: &Path) -> Result<(), FsError> {
        // Validate source
        let src_str = src.to_string_lossy();
        let resolved_src = self.validate_path(&src_str)?;

        // Source must not be protected (we're removing it)
        self.check_write_protected(&resolved_src)?;

        // Validate destination
        let dest_str = dest.to_string_lossy();
        let resolved_dest = self.validate_path(&dest_str)?;
        self.check_write_protected(&resolved_dest)?;
        self.check_extension(&resolved_dest)?;

        // Ensure parent directories exist
        self.ensure_parent_dirs(&resolved_dest)?;

        // Perform the rename
        std::fs::rename(&resolved_src, &resolved_dest).map_err(FsError::from)
    }

    /// Get file metadata.
    ///
    /// Security checks:
    /// - Layer 1: Sandbox bounds (path validation)
    /// - Layer 5: Symlink resolution
    fn stat(&self, path: &Path) -> Result<FileMeta, FsError> {
        let path_str = path.to_string_lossy();
        let resolved = self.validate_path(&path_str)?;

        FileMeta::from_path(&resolved)
    }
}

#[cfg(test)]
mod tests {
    // Ensure SecureFsBackend is Send + Sync
    // This is automatically satisfied since:
    // - FsPolicy contains only Send + Sync types
    // - Arc<dyn SecureBackend> is Send + Sync (trait has Send + Sync bounds)
    // - PathBuf is Send + Sync
    static_assertions::assert_impl_all!(super::SecureFsBackend: Send, Sync);
}
