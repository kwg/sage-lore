// SPDX-License-Identifier: MIT
//! Watch mode types for continuous test execution.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::framework::Framework;

// ============================================================================
// Watch Mode Types
// ============================================================================

/// Handle to a running watch mode process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchHandle {
    /// Unique identifier for this watch session
    pub id: String,
    /// Framework being watched
    pub framework: Framework,
    /// Project root being watched
    pub project_root: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // WatchHandle tests
    // ========================================================================

    #[test]
    fn test_watch_handle_creation() {
        use std::path::PathBuf;
        let handle = WatchHandle {
            id: "watch-123".to_string(),
            framework: Framework::Cargo,
            project_root: PathBuf::from("/tmp/project"),
        };

        assert_eq!(handle.id, "watch-123");
        assert_eq!(handle.framework, Framework::Cargo);
        assert_eq!(handle.project_root, PathBuf::from("/tmp/project"));
    }
}
