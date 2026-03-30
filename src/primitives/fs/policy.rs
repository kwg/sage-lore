// SPDX-License-Identifier: MIT
//! Filesystem security policy configuration and validation.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::types::{DeletePolicy, FsError};

// ============================================================================
// Default Security Lists
// ============================================================================

/// Default protected paths (compiled into engine, cannot be overridden).
pub const DEFAULT_PROTECTED: &[&str] = &[
    // Git internals
    ".git/**",
    // Secrets and credentials
    ".env",
    ".env.*",
    "**/*.pem",
    "**/*.key",
    "**/*.p12",
    "**/credentials.json",
    "**/secrets.yaml",
    "**/secrets.json",
    "**/.secrets/**",
    // SAGE security config (prevent scroll from weakening its own policy)
    ".sage-lore/security/**",
    // Scroll infrastructure — prevent scrolls from modifying themselves (SAFETY.md Invariant 1)
    "**/*.scroll",
    "scrolls/**", "examples/scrolls/**",
    ".sage-lore/**",
    ".sage-project/**",
    // Build outputs (scrolls shouldn't write here directly)
    "node_modules/**",
    "target/**",
    "dist/**",
    "build/**",
    ".next/**",
    "__pycache__/**",
];

/// Default allowed extensions for write operations.
pub const DEFAULT_ALLOWED_EXTENSIONS: &[&str] = &[
    // Code
    ".rs", ".py", ".ts", ".js", ".tsx", ".jsx", ".go", ".java", ".rb", ".sh", ".bash", ".vue",
    ".svelte", ".c", ".cpp", ".h", ".hpp", // Config
    ".yaml", ".yml", ".toml", ".json", ".xml", ".ini", ".cfg", ".conf", // Docs
    ".md", ".txt", ".rst", ".adoc", // Scripting
    ".rhai", // Rhai scripts only - .scroll is infrastructure, not LLM-writable
    // Web
    ".html", ".css", ".scss", ".less", // Data (text-based)
    ".csv", ".sql",
];

// ============================================================================
// Glob Matching Helper
// ============================================================================

/// Match a glob pattern against a path.
///
/// Supports:
/// - `*` - matches any sequence of characters except path separators
/// - `**` - matches any sequence of characters including path separators
/// - `?` - matches any single character
///
/// # Arguments
///
/// * `pattern` - The glob pattern to match
/// * `path` - The path to match against (should be relative)
pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    // Normalize path separators for cross-platform support
    let path = path.replace('\\', "/");
    let pattern = pattern.replace('\\', "/");

    glob_match_recursive(&pattern, &path)
}

/// Recursive implementation of glob matching.
fn glob_match_recursive(pattern: &str, path: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut path_chars = path.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                // Check for **
                if pattern_chars.peek() == Some(&'*') {
                    pattern_chars.next(); // consume second *

                    // Skip any trailing slashes after **
                    while pattern_chars.peek() == Some(&'/') {
                        pattern_chars.next();
                    }

                    let remaining_pattern: String = pattern_chars.collect();

                    // ** matches everything if nothing follows
                    if remaining_pattern.is_empty() {
                        return true;
                    }

                    // Try matching remaining pattern at every position
                    let remaining_path: String = path_chars.collect();
                    for i in 0..=remaining_path.len() {
                        if glob_match_recursive(&remaining_pattern, &remaining_path[i..]) {
                            return true;
                        }
                    }
                    return false;
                } else {
                    // Single * - match any characters except /
                    // Skip trailing slash after *
                    let _next_pattern: Option<char> = pattern_chars.peek().copied();

                    // Collect remaining pattern
                    let remaining_pattern: String = pattern_chars.collect();

                    // Try matching at every position up to next /
                    let remaining_path: String = path_chars.collect();
                    for i in 0..=remaining_path.len() {
                        let before_match = &remaining_path[..i];
                        // Single * cannot match /
                        if before_match.contains('/') {
                            break;
                        }
                        if remaining_pattern.is_empty() && i == remaining_path.len() {
                            return true;
                        }
                        if glob_match_recursive(&remaining_pattern, &remaining_path[i..]) {
                            return true;
                        }
                    }
                    return false;
                }
            }
            '?' => {
                // Match any single character except /
                match path_chars.next() {
                    Some('/') | None => return false,
                    Some(_) => continue,
                }
            }
            c => {
                // Literal character match
                match path_chars.next() {
                    Some(pc) if pc == c => continue,
                    _ => return false,
                }
            }
        }
    }

    // Pattern exhausted - path should also be exhausted
    path_chars.next().is_none()
}

// ============================================================================
// Filesystem Policy
// ============================================================================

/// Filesystem security policy configuration.
///
/// Projects can extend (but never weaken) the default protections — the security floor only rises (D28).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsPolicy {
    /// Project root directory (all operations bounded here)
    pub project_root: PathBuf,

    /// Additional protected paths beyond defaults (glob patterns)
    #[serde(default)]
    pub additional_protected: Vec<String>,

    /// Additional allowed extensions beyond defaults
    #[serde(default)]
    pub additional_extensions: Vec<String>,

    /// Read-protected paths — opt-in list that blocks reads on sensitive files (D29)
    #[serde(default)]
    pub read_protected: Vec<String>,

    /// Delete policy
    #[serde(default)]
    pub delete_policy: DeletePolicy,

    /// Whether to scan content for secrets on write (default: true)
    #[serde(default = "default_true")]
    pub scan_content: bool,
}

fn default_true() -> bool {
    true
}

impl Default for FsPolicy {
    fn default() -> Self {
        Self {
            project_root: PathBuf::from("."),
            additional_protected: Vec::new(),
            additional_extensions: Vec::new(),
            read_protected: Vec::new(),
            delete_policy: DeletePolicy::default(),
            scan_content: true,
        }
    }
}

impl FsPolicy {
    /// Create a new policy with the given project root.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            ..Default::default()
        }
    }

    /// Validate a path is within the sandbox and return its resolved absolute path.
    ///
    /// This implements Layer 1 (Sandbox Bounds) and Layer 5 (Symlink Handling) of the
    /// security model. Symlinks are resolved to real paths before validation to prevent sandbox escapes (D31).
    ///
    /// # Arguments
    ///
    /// * `path` - The path to validate (relative to project root or absolute)
    ///
    /// # Errors
    ///
    /// * `FsError::InvalidPath` - If the path cannot be resolved
    /// * `FsError::SandboxEscape` - If the resolved path is outside project root
    pub fn validate_path(&self, path: &str) -> Result<PathBuf, FsError> {
        let joined = self.project_root.join(path);

        // canonicalize() resolves symlinks and returns the absolute path.
        // If the path doesn't exist yet, we need to handle that case:
        // - For existing paths: canonicalize directly
        // - For non-existing paths: canonicalize the parent, then append the filename
        //
        // NOTE: There is a TOCTOU window between exists() and canonicalize() where
        // a symlink could be swapped. This is inherent to the canonicalize approach
        // and acceptable for our threat model (same-process sandbox). A future
        // improvement could use O_NOFOLLOW or openat() for stricter enforcement.
        let resolved = if joined.exists() {
            joined
                .canonicalize()
                .map_err(|_| FsError::InvalidPath(path.to_string()))?
        } else {
            // For non-existing paths, canonicalize the deepest existing ancestor
            // and then append the remaining components
            self.resolve_nonexistent_path(&joined, path)?
        };

        // Get the canonical project root for comparison
        let canonical_root = self
            .project_root
            .canonicalize()
            .map_err(|_| FsError::InvalidPath("project root".to_string()))?;

        // Verify the resolved path is within the project root
        if !resolved.starts_with(&canonical_root) {
            return Err(FsError::SandboxEscape {
                path: path.to_string(),
                resolved: resolved.display().to_string(),
            });
        }

        Ok(resolved)
    }

    /// Resolve a non-existent path by canonicalizing the deepest existing ancestor.
    fn resolve_nonexistent_path(&self, joined: &Path, original: &str) -> Result<PathBuf, FsError> {
        let mut existing_ancestor = joined.to_path_buf();
        let mut remaining_components: Vec<std::ffi::OsString> = Vec::new();

        // Walk up the path tree to find the deepest existing ancestor
        while !existing_ancestor.exists() {
            if let Some(file_name) = existing_ancestor.file_name() {
                // Clone the file_name to avoid borrow issues
                remaining_components.push(file_name.to_os_string());
            }
            match existing_ancestor.parent() {
                Some(parent) => existing_ancestor = parent.to_path_buf(),
                None => return Err(FsError::InvalidPath(original.to_string())),
            }
        }

        // Canonicalize the existing ancestor
        let mut resolved = existing_ancestor
            .canonicalize()
            .map_err(|_| FsError::InvalidPath(original.to_string()))?;

        // Append the remaining components (in reverse order since we collected them bottom-up)
        for component in remaining_components.into_iter().rev() {
            // Validate each component to prevent path traversal
            let component_str = component.to_string_lossy();
            if component_str == ".." {
                // Even in non-existent paths, reject explicit parent traversal
                return Err(FsError::SandboxEscape {
                    path: original.to_string(),
                    resolved: resolved.display().to_string(),
                });
            }
            resolved.push(&component);
        }

        Ok(resolved)
    }

    /// Check if a path is protected and cannot be written/deleted.
    ///
    /// This implements Layer 2 (Protected Paths) of the security model.
    /// Protected paths are additive only — projects can add paths but never remove defaults, so the floor only rises (D28).
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check (relative to project root)
    pub fn is_protected(&self, path: &Path) -> bool {
        // Get the path relative to project root for pattern matching
        let relative = self.to_relative_path(path);

        // Check default protected patterns (cannot be overridden)
        for pattern in DEFAULT_PROTECTED {
            if glob_match(pattern, &relative) {
                return true;
            }
        }

        // Check project-specific protected paths — additive only, floor never lowers (D28)
        for pattern in &self.additional_protected {
            if glob_match(pattern, &relative) {
                return true;
            }
        }

        false
    }

    /// Check if a path is read-protected — opt-in list that blocks reads on sensitive files (D29).
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check (relative to project root)
    pub fn is_read_protected(&self, path: &Path) -> bool {
        let relative = self.to_relative_path(path);

        for pattern in &self.read_protected {
            if glob_match(pattern, &relative) {
                return true;
            }
        }

        false
    }

    /// Check if a file extension is allowed for write operations.
    ///
    /// This implements Layer 3 (Allowed Extensions) of the security model.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check
    pub fn is_allowed_extension(&self, path: &Path) -> bool {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e));

        match ext {
            Some(ext) => {
                // Check default allowed extensions
                if DEFAULT_ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
                    return true;
                }

                // Check project-specific extensions (additive)
                self.additional_extensions.contains(&ext)
            }
            None => false, // No extension = blocked
        }
    }

    /// Convert an absolute path to a path relative to the project root.
    fn to_relative_path(&self, path: &Path) -> String {
        // Try to get canonical roots for accurate comparison
        let canonical_root = self.project_root.canonicalize().ok();
        let canonical_path = path.canonicalize().ok();

        match (canonical_root, canonical_path) {
            (Some(root), Some(abs_path)) => abs_path
                .strip_prefix(&root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string()),
            _ => {
                // Fallback: try direct strip_prefix or return as-is
                path.strip_prefix(&self.project_root)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.to_string_lossy().to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Glob Matching Tests
    // ============================================================================

    #[test]
    fn test_glob_match_literal() {
        assert!(glob_match("foo.txt", "foo.txt"));
        assert!(!glob_match("foo.txt", "bar.txt"));
        assert!(!glob_match("foo.txt", "foo.rs"));
    }

    #[test]
    fn test_glob_match_single_star() {
        assert!(glob_match("*.txt", "foo.txt"));
        assert!(glob_match("*.txt", "bar.txt"));
        assert!(!glob_match("*.txt", "foo.rs"));
        assert!(glob_match("foo.*", "foo.txt"));
        assert!(glob_match("foo.*", "foo.rs"));
    }

    #[test]
    fn test_glob_match_single_star_does_not_match_slash() {
        assert!(!glob_match("*.txt", "dir/foo.txt"));
        assert!(!glob_match("src/*.rs", "src/sub/foo.rs"));
    }

    #[test]
    fn test_glob_match_double_star() {
        assert!(glob_match("**/*.txt", "foo.txt"));
        assert!(glob_match("**/*.txt", "dir/foo.txt"));
        assert!(glob_match("**/*.txt", "a/b/c/foo.txt"));
        assert!(glob_match("src/**/*.rs", "src/foo.rs"));
        assert!(glob_match("src/**/*.rs", "src/a/b/foo.rs"));
    }

    #[test]
    fn test_glob_match_double_star_suffix() {
        assert!(glob_match(".git/**", ".git/config"));
        assert!(glob_match(".git/**", ".git/objects/pack"));
        assert!(glob_match("node_modules/**", "node_modules/foo/bar.js"));
    }

    #[test]
    fn test_glob_match_question_mark() {
        assert!(glob_match("foo?.txt", "foo1.txt"));
        assert!(glob_match("foo?.txt", "fooa.txt"));
        assert!(!glob_match("foo?.txt", "foo.txt"));
        assert!(!glob_match("foo?.txt", "foo12.txt"));
    }

    #[test]
    fn test_glob_match_env_patterns() {
        assert!(glob_match(".env", ".env"));
        assert!(glob_match(".env.*", ".env.local"));
        assert!(glob_match(".env.*", ".env.production"));
        assert!(!glob_match(".env.*", ".env"));
    }

    #[test]
    fn test_glob_match_credential_patterns() {
        assert!(glob_match("**/*.pem", "certs/server.pem"));
        assert!(glob_match("**/*.key", "ssl/private.key"));
        assert!(glob_match("**/credentials.json", "config/credentials.json"));
        assert!(glob_match("**/secrets.yaml", "k8s/secrets.yaml"));
    }

    // ============================================================================
    // Policy Tests
    // ============================================================================

    #[test]
    fn test_fs_policy_default() {
        let policy = FsPolicy::default();
        assert!(policy.scan_content);
        assert!(policy.additional_protected.is_empty());
        assert!(policy.additional_extensions.is_empty());
        assert!(policy.read_protected.is_empty());
        assert_eq!(policy.delete_policy, DeletePolicy::SameAsWrite);
    }

    #[test]
    fn test_fs_policy_new() {
        let policy = FsPolicy::new(PathBuf::from("/project"));
        assert_eq!(policy.project_root, PathBuf::from("/project"));
        assert!(policy.scan_content);
    }

    #[test]
    fn test_default_protected_contains_git() {
        assert!(DEFAULT_PROTECTED.contains(&".git/**"));
    }

    #[test]
    fn test_default_protected_contains_env() {
        assert!(DEFAULT_PROTECTED.contains(&".env"));
    }

    #[test]
    fn test_default_extensions_contains_rust() {
        assert!(DEFAULT_ALLOWED_EXTENSIONS.contains(&".rs"));
    }

    #[test]
    fn test_default_extensions_contains_markdown() {
        assert!(DEFAULT_ALLOWED_EXTENSIONS.contains(&".md"));
    }

    // ============================================================================
    // Path Validation Tests
    // ============================================================================

    #[test]
    fn test_validate_path_simple() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        // Create a file to test with
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "content").unwrap();

        let result = policy.validate_path("test.txt");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), file_path.canonicalize().unwrap());
    }

    #[test]
    fn test_validate_path_subdirectory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        // Create a subdirectory and file
        let subdir = temp_dir.path().join("src");
        std::fs::create_dir(&subdir).unwrap();
        let file_path = subdir.join("main.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let result = policy.validate_path("src/main.rs");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), file_path.canonicalize().unwrap());
    }

    #[test]
    fn test_validate_path_sandbox_escape_dotdot() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        let result = policy.validate_path("../etc/passwd");
        assert!(matches!(result, Err(FsError::SandboxEscape { .. })));
    }

    #[test]
    fn test_validate_path_nonexistent_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        // Validating a non-existent path within sandbox should succeed
        let result = policy.validate_path("new_file.txt");
        assert!(result.is_ok());

        let resolved = result.unwrap();
        assert!(resolved.starts_with(temp_dir.path().canonicalize().unwrap()));
    }

    #[test]
    fn test_validate_path_nonexistent_nested() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        // Validating a deeply nested non-existent path should succeed
        let result = policy.validate_path("a/b/c/new_file.txt");
        assert!(result.is_ok());

        let resolved = result.unwrap();
        assert!(resolved.starts_with(temp_dir.path().canonicalize().unwrap()));
        assert!(resolved.ends_with("a/b/c/new_file.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn test_validate_path_symlink_inside_sandbox() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        // Create a real file
        let real_file = temp_dir.path().join("real.txt");
        std::fs::write(&real_file, "content").unwrap();

        // Create a symlink to it
        let link_path = temp_dir.path().join("link.txt");
        std::os::unix::fs::symlink(&real_file, &link_path).unwrap();

        let result = policy.validate_path("link.txt");
        assert!(result.is_ok());
        // Should resolve to the real file
        assert_eq!(result.unwrap(), real_file.canonicalize().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn test_validate_path_symlink_escape() {
        let temp_dir = tempfile::tempdir().unwrap();
        let outside_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        // Create a file outside the sandbox
        let outside_file = outside_dir.path().join("outside.txt");
        std::fs::write(&outside_file, "secret").unwrap();

        // Create a symlink inside sandbox pointing outside
        let link_path = temp_dir.path().join("escape_link.txt");
        std::os::unix::fs::symlink(&outside_file, &link_path).unwrap();

        let result = policy.validate_path("escape_link.txt");
        assert!(matches!(result, Err(FsError::SandboxEscape { .. })));
    }

    // ============================================================================
    // Protected Path Tests
    // ============================================================================

    #[test]
    fn test_is_protected_git() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        // Create .git directory for the test
        let git_dir = temp_dir.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();
        let config_file = git_dir.join("config");
        std::fs::write(&config_file, "[core]").unwrap();

        assert!(policy.is_protected(Path::new(".git/config")));
        assert!(policy.is_protected(Path::new(".git/objects/pack")));
    }

    #[test]
    fn test_is_protected_env() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        assert!(policy.is_protected(Path::new(".env")));
        assert!(policy.is_protected(Path::new(".env.local")));
        assert!(policy.is_protected(Path::new(".env.production")));
    }

    #[test]
    fn test_is_protected_credentials() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        assert!(policy.is_protected(Path::new("certs/server.pem")));
        assert!(policy.is_protected(Path::new("ssl/private.key")));
        assert!(policy.is_protected(Path::new("config/credentials.json")));
        assert!(policy.is_protected(Path::new("k8s/secrets.yaml")));
    }

    #[test]
    fn test_is_protected_sage_security() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        assert!(policy.is_protected(Path::new(".sage-lore/security/fs-policy.yaml")));
        assert!(policy.is_protected(Path::new(".sage-lore/security/rules.toml")));
    }

    #[test]
    fn test_is_protected_build_outputs() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        assert!(policy.is_protected(Path::new("node_modules/foo/index.js")));
        assert!(policy.is_protected(Path::new("target/debug/binary")));
        assert!(policy.is_protected(Path::new("dist/bundle.js")));
        assert!(policy.is_protected(Path::new("build/output.css")));
    }

    #[test]
    fn test_is_protected_normal_files_not_protected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = FsPolicy::new(temp_dir.path().to_path_buf());

        assert!(!policy.is_protected(Path::new("src/main.rs")));
        assert!(!policy.is_protected(Path::new("README.md")));
        assert!(!policy.is_protected(Path::new("Cargo.toml")));
    }

    #[test]
    fn test_is_protected_additional_patterns() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut policy = FsPolicy::new(temp_dir.path().to_path_buf());

        // Add project-specific protected paths
        policy.additional_protected = vec!["internal/**".to_string()];

        assert!(policy.is_protected(Path::new("internal/docs/secret.md")));
        assert!(!policy.is_protected(Path::new("public/docs/readme.md")));
    }

    // ============================================================================
    // Allowed Extension Tests
    // ============================================================================

    #[test]
    fn test_is_allowed_extension_code() {
        let policy = FsPolicy::default();

        assert!(policy.is_allowed_extension(Path::new("main.rs")));
        assert!(policy.is_allowed_extension(Path::new("app.py")));
        assert!(policy.is_allowed_extension(Path::new("index.ts")));
        assert!(policy.is_allowed_extension(Path::new("component.tsx")));
        assert!(policy.is_allowed_extension(Path::new("script.js")));
    }

    #[test]
    fn test_is_allowed_extension_config() {
        let policy = FsPolicy::default();

        assert!(policy.is_allowed_extension(Path::new("config.yaml")));
        assert!(policy.is_allowed_extension(Path::new("settings.yml")));
        assert!(policy.is_allowed_extension(Path::new("Cargo.toml")));
        assert!(policy.is_allowed_extension(Path::new("package.json")));
    }

    #[test]
    fn test_is_allowed_extension_docs() {
        let policy = FsPolicy::default();

        assert!(policy.is_allowed_extension(Path::new("README.md")));
        assert!(policy.is_allowed_extension(Path::new("notes.txt")));
        assert!(policy.is_allowed_extension(Path::new("docs.rst")));
    }

    #[test]
    fn test_is_allowed_extension_blocked() {
        let policy = FsPolicy::default();

        // Binaries and executables should be blocked
        assert!(!policy.is_allowed_extension(Path::new("program.exe")));
        assert!(!policy.is_allowed_extension(Path::new("library.dll")));
        assert!(!policy.is_allowed_extension(Path::new("binary.so")));
        assert!(!policy.is_allowed_extension(Path::new("image.png")));
        assert!(!policy.is_allowed_extension(Path::new("archive.zip")));
    }

    #[test]
    fn test_is_allowed_extension_no_extension() {
        let policy = FsPolicy::default();

        // Files without extension should be blocked
        assert!(!policy.is_allowed_extension(Path::new("Makefile")));
        assert!(!policy.is_allowed_extension(Path::new("Dockerfile")));
        assert!(!policy.is_allowed_extension(Path::new("README")));
    }

    #[test]
    fn test_is_allowed_extension_additional() {
        let mut policy = FsPolicy::default();

        // Add project-specific extensions
        policy.additional_extensions = vec![".proto".to_string(), ".graphql".to_string()];

        assert!(policy.is_allowed_extension(Path::new("schema.proto")));
        assert!(policy.is_allowed_extension(Path::new("query.graphql")));
        // Default extensions still work
        assert!(policy.is_allowed_extension(Path::new("main.rs")));
    }

    // ============================================================================
    // Read Protection Tests
    // ============================================================================

    #[test]
    fn test_is_read_protected_default_empty() {
        let policy = FsPolicy::default();

        // By default, nothing is read-protected — read protection is opt-in (D29)
        assert!(!policy.is_read_protected(Path::new(".env")));
        assert!(!policy.is_read_protected(Path::new("secrets.yaml")));
    }

    #[test]
    fn test_is_read_protected_with_patterns() {
        let mut policy = FsPolicy::default();
        policy.read_protected = vec![".env".to_string(), ".env.*".to_string()];

        assert!(policy.is_read_protected(Path::new(".env")));
        assert!(policy.is_read_protected(Path::new(".env.local")));
        assert!(!policy.is_read_protected(Path::new("config.yaml")));
    }
}
