// SPDX-License-Identifier: MIT
//! Path resolution for the three-tier config hierarchy (D4, D14, D25, D30, #178).
//!
//! Discovers global, user, and project roots based on platform.
//! Global: SAGE_LORE_HOME env → compile-time SAGE_LORE_DATADIR → exe-relative → None
//! User: XDG_CONFIG_HOME/sage-lore (respects dirs::config_dir)
//! Project: walk cwd up looking for .sage-lore/, stop at .git or device boundary

use std::path::{Path, PathBuf};

/// Platform detection for path resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    NixOS,
    Linux,
    Windows,
}

impl Platform {
    /// Detect the current platform.
    pub fn detect() -> Self {
        #[cfg(target_os = "windows")]
        {
            return Platform::Windows;
        }

        #[cfg(not(target_os = "windows"))]
        {
            if Path::new("/etc/NIXOS").exists() {
                Platform::NixOS
            } else {
                Platform::Linux
            }
        }
    }
}

/// Resolves paths for the three-tier config hierarchy.
#[derive(Debug, Clone)]
pub struct PathResolver {
    pub platform: Platform,
    pub global_root: Option<PathBuf>,
    pub user_root: PathBuf,
    pub project_root: Option<PathBuf>,
}

impl PathResolver {
    /// Create a PathResolver by discovering all tiers.
    ///
    /// `working_dir` is used as the starting point for project root walk-up.
    pub fn discover(working_dir: &Path) -> Self {
        let platform = Platform::detect();
        let global_root = Self::discover_global();
        let user_root = Self::discover_user();
        let project_root = Self::discover_project(working_dir);

        Self {
            platform,
            global_root,
            user_root,
            project_root,
        }
    }

    /// Create a PathResolver with explicit paths (for testing).
    pub fn with_paths(
        global_root: Option<PathBuf>,
        user_root: PathBuf,
        project_root: Option<PathBuf>,
    ) -> Self {
        Self {
            platform: Platform::detect(),
            global_root,
            user_root,
            project_root,
        }
    }

    /// Resolve a config file across tiers. Returns paths that exist, in merge order
    /// (global first, project last — caller merges left to right).
    pub fn resolve_config_files(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Global tier
        if let Some(ref global) = self.global_root {
            let p = global.join("config/config.yaml");
            if p.exists() {
                paths.push(p);
            }
        }

        // User tier
        let user_config = self.user_root.join("config.yaml");
        if user_config.exists() {
            paths.push(user_config);
        }

        // Project tier
        if let Some(ref project) = self.project_root {
            let p = project.join("config.yaml");
            if p.exists() {
                paths.push(p);
            }
        }

        paths
    }

    /// Resolve a scroll by name using the search path (D18, D33).
    ///
    /// Bare/relative names search: project → user → global, first match wins.
    /// Paths starting with `.` or `/` are returned directly (if they exist).
    pub fn resolve_scroll(&self, name: &str) -> Option<PathBuf> {
        // Direct resolution for absolute or explicit-relative paths
        if name.starts_with('/') || name.starts_with("./") || name.starts_with("..") {
            let p = PathBuf::from(name);
            return if p.exists() { Some(p) } else { None };
        }

        // Add .scroll extension if not present
        let scroll_name = if name.ends_with(".scroll") {
            name.to_string()
        } else {
            format!("{}.scroll", name)
        };

        // Search path: project → user → global
        let search_dirs: Vec<PathBuf> = [
            self.project_root.as_ref().map(|p| p.join("scrolls")),
            Some(self.user_root.join("scrolls")),
            self.global_root.as_ref().map(|p| p.join("scrolls")),
        ]
        .into_iter()
        .flatten()
        .collect();

        for dir in search_dirs {
            let candidate = dir.join(&scroll_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        None
    }

    /// Resolve security policy files across tiers (for ratchet merge).
    pub fn resolve_policy_files(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(ref global) = self.global_root {
            let p = global.join("config/security/policy.yaml");
            if p.exists() {
                paths.push(p);
            }
        }

        let user_policy = self.user_root.join("security/policy.yaml");
        if user_policy.exists() {
            paths.push(user_policy);
        }

        if let Some(ref project) = self.project_root {
            let p = project.join("security/policy.yaml");
            if p.exists() {
                paths.push(p);
            }
        }

        paths
    }

    /// Get the project directory (parent of .sage-lore/), if discovered.
    pub fn project_dir(&self) -> Option<&Path> {
        self.project_root.as_ref().and_then(|p| p.parent())
    }

    // -- Discovery methods --

    /// Discover global root (D4, D30):
    /// 1. SAGE_LORE_HOME env var
    /// 2. Compile-time SAGE_LORE_DATADIR
    /// 3. Exe-relative (walk up to find sibling scrolls/)
    /// 4. None
    fn discover_global() -> Option<PathBuf> {
        // 1. Env var always wins
        if let Ok(home) = std::env::var("SAGE_LORE_HOME") {
            let p = PathBuf::from(home);
            if p.exists() {
                return Some(p);
            }
        }

        // 2. Compile-time data dir (set by Nix/deb builds)
        if let Some(datadir) = option_env!("SAGE_LORE_DATADIR") {
            let p = PathBuf::from(datadir);
            if p.exists() {
                return Some(p);
            }
        }

        // 3. Exe-relative: walk up from binary to find sibling scrolls/
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                // Check sibling (bin/ next to scrolls/)
                if let Some(parent) = exe_dir.parent() {
                    let candidate = parent.join("share/sage-lore");
                    if candidate.join("scrolls").exists() {
                        return Some(candidate);
                    }
                    // Also check direct sibling
                    if parent.join("scrolls").exists() {
                        return Some(parent.to_path_buf());
                    }
                }
            }
        }

        // 4. No global tier
        None
    }

    /// Discover user config root via XDG (D3, D24).
    fn discover_user() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| {
                // Fallback: HOME/.config on Unix, should never hit on Windows
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(home).join(".config")
            })
            .join("sage-lore")
    }

    /// Discover project root by walking up from working_dir (D14, D25).
    ///
    /// Looks for `.sage-lore/` directory. Stops at:
    /// - `.git` (file or directory) — repo boundary
    /// - Different device ID — filesystem boundary
    /// - System root
    fn discover_project(working_dir: &Path) -> Option<PathBuf> {
        let working_dir = working_dir.canonicalize().unwrap_or_else(|_| working_dir.to_path_buf());

        #[cfg(unix)]
        let start_dev = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(&working_dir).ok().map(|m| m.dev())
        };

        let mut current = working_dir.as_path();

        loop {
            // Check for .sage-lore/ in current directory
            let candidate = current.join(".sage-lore");
            if candidate.is_dir() {
                return Some(candidate);
            }

            // Check stop conditions
            let git_dir = current.join(".git");
            if git_dir.exists() {
                // .git exists (file for worktrees, dir for normal repos) — this is the repo root
                // If .sage-lore/ wasn't here, it doesn't exist in this repo
                return None;
            }

            // Move to parent
            let parent = match current.parent() {
                Some(p) if p != current => p,
                _ => return None, // Hit system root
            };

            // Check filesystem boundary (Unix only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if let (Some(start), Ok(parent_meta)) = (start_dev, std::fs::metadata(parent)) {
                    if parent_meta.dev() != start {
                        return None; // Different filesystem
                    }
                }
            }

            current = parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_platform_detect() {
        let p = Platform::detect();
        // Just verify it returns something valid
        assert!(matches!(p, Platform::NixOS | Platform::Linux | Platform::Windows));
    }

    #[test]
    fn test_discover_project_with_sage_lore_dir() {
        let temp = TempDir::new().unwrap();
        let sage_dir = temp.path().join(".sage-lore");
        fs::create_dir_all(&sage_dir).unwrap();

        let result = PathResolver::discover_project(temp.path());
        assert!(result.is_some());
        assert!(result.unwrap().ends_with(".sage-lore"));
    }

    #[test]
    fn test_discover_project_none_when_missing() {
        let temp = TempDir::new().unwrap();
        // Create a .git to stop walk-up
        fs::create_dir_all(temp.path().join(".git")).unwrap();

        let result = PathResolver::discover_project(temp.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_discover_project_stops_at_git() {
        let temp = TempDir::new().unwrap();
        // Parent has .sage-lore/ but child has .git — should not find it
        let child = temp.path().join("sub");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(child.join(".git")).unwrap();
        fs::create_dir_all(temp.path().join(".sage-lore")).unwrap();

        let result = PathResolver::discover_project(&child);
        assert!(result.is_none(), "Should stop at .git, not find parent .sage-lore/");
    }

    #[test]
    fn test_discover_project_stops_at_git_file() {
        let temp = TempDir::new().unwrap();
        // .git as file (worktree)
        let child = temp.path().join("sub");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join(".git"), "gitdir: ../other/.git").unwrap();
        fs::create_dir_all(temp.path().join(".sage-lore")).unwrap();

        let result = PathResolver::discover_project(&child);
        assert!(result.is_none(), "Should stop at .git file (worktree)");
    }

    #[test]
    fn test_discover_project_nearest_wins() {
        let temp = TempDir::new().unwrap();
        let inner = temp.path().join("inner");
        fs::create_dir_all(inner.join(".sage-lore")).unwrap();
        fs::create_dir_all(temp.path().join(".sage-lore")).unwrap();

        let result = PathResolver::discover_project(&inner);
        assert!(result.is_some());
        // Should find inner/.sage-lore, not parent/.sage-lore
        let found = result.unwrap();
        assert!(found.starts_with(temp.path().join("inner")));
    }

    #[test]
    fn test_resolve_scroll_search_path() {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("project/.sage-lore");
        let user = temp.path().join("user");

        // Create scroll in user tier only
        fs::create_dir_all(user.join("scrolls/adapters")).unwrap();
        fs::write(user.join("scrolls/adapters/test.scroll"), "scroll {}").unwrap();

        let resolver = PathResolver::with_paths(None, user, Some(project));
        let result = resolver.resolve_scroll("adapters/test");
        assert!(result.is_some());
    }

    #[test]
    fn test_resolve_scroll_project_overrides_user() {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("project/.sage-lore");
        let user = temp.path().join("user");

        // Create same scroll in both tiers
        fs::create_dir_all(project.join("scrolls")).unwrap();
        fs::write(project.join("scrolls/test.scroll"), "project version").unwrap();
        fs::create_dir_all(user.join("scrolls")).unwrap();
        fs::write(user.join("scrolls/test.scroll"), "user version").unwrap();

        let resolver = PathResolver::with_paths(None, user.clone(), Some(project.clone()));
        let result = resolver.resolve_scroll("test");
        assert!(result.is_some());
        assert!(result.unwrap().starts_with(&project), "Project tier should win");
    }

    #[test]
    fn test_resolve_scroll_direct_path() {
        let temp = TempDir::new().unwrap();
        let scroll = temp.path().join("my-scroll.scroll");
        fs::write(&scroll, "scroll {}").unwrap();

        let resolver = PathResolver::with_paths(None, temp.path().join("user"), None);
        // Absolute path → direct resolution
        let result = resolver.resolve_scroll(&scroll.to_string_lossy());
        assert!(result.is_some());
    }

    #[test]
    fn test_resolve_config_files_ordering() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join("global");
        let user = temp.path().join("user");
        let project = temp.path().join("project/.sage-lore");

        // Create config in all three tiers
        fs::create_dir_all(global.join("config")).unwrap();
        fs::write(global.join("config/config.yaml"), "global: true").unwrap();
        fs::create_dir_all(&user).unwrap();
        fs::write(user.join("config.yaml"), "user: true").unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("config.yaml"), "project: true").unwrap();

        let resolver = PathResolver::with_paths(Some(global), user, Some(project));
        let files = resolver.resolve_config_files();
        assert_eq!(files.len(), 3);
        // Global first, project last (merge order)
        assert!(files[0].to_string_lossy().contains("global"));
        assert!(files[2].to_string_lossy().contains("project"));
    }
}
