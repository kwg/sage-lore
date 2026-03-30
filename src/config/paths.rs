// SPDX-License-Identifier: MIT
//! Scroll path configuration for the SAGE Method engine.

use std::path::PathBuf;

/// Paths for scroll resolution.
///
/// Scrolls are resolved in order: project → user → global (D18, #178).
#[derive(Debug, Clone)]
pub struct ScrollPaths {
    /// Project-local scrolls (e.g., `.sage-lore/scrolls/`)
    pub project: PathBuf,
    /// User's global scrolls (e.g., `~/.config/sage-lore/scrolls/`)
    pub user: PathBuf,
    /// Global/built-in scrolls (e.g., `/opt/sage-lore/scrolls/`)
    pub global: Option<PathBuf>,
}
