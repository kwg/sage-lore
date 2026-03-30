// SPDX-License-Identifier: MIT
//! Filesystem primitive for sandboxed operations in SAGE scrolls.
//!
//! This module provides secure filesystem operations bounded to the project root.
//! All operations enforce a defense-in-depth security model with 5 layers:
//!
//! 1. **Sandbox Bounds**: All paths must resolve within `project_root`
//! 2. **Protected Paths**: Deny list that cannot be written/deleted
//! 3. **Allowed Extensions**: Allow list for write operations
//! 4. **Content Scanning**: All writes scanned for secrets via `secure` primitive
//! 5. **Symlink Handling**: Symlinks resolved before validation to prevent escapes

mod backend;
mod mock;
mod policy;
mod types;

// Re-export public API
pub use backend::{FsBackend, SecureFsBackend};
pub use mock::{FsCall, MockFsBackend};
pub use policy::{FsPolicy, DEFAULT_ALLOWED_EXTENSIONS, DEFAULT_PROTECTED};
pub use types::{DeletePolicy, FileMeta, FsError};
