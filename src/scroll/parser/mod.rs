// SPDX-License-Identifier: MIT
//! Legacy YAML scroll parsing (retained for schema types and secret scanning).
//!
//! The primary parser is now `assembly::parser`. This module is retained for:
//! - `secure::scan_for_secrets` — secret detection in scroll source
//! - The parse functions below (deprecated, used only in legacy code paths)

use std::path::Path;

use crate::scroll::error::ParseError;
use crate::scroll::schema::Scroll;

// Secret detection module
pub mod secure;

// Re-export the secret scanning function
pub use secure::scan_for_secrets;

/// Parse a YAML string into a validated Scroll struct.
///
/// **Deprecated**: Use `assembly::parser::parse()` for Scroll Assembly format.
pub fn parse_scroll(yaml: &str) -> Result<Scroll, ParseError> {
    scan_for_secrets(yaml)?;
    let scroll: Scroll = serde_yaml::from_str(yaml)?;
    validate_scroll(&scroll)?;
    Ok(scroll)
}

/// Load and parse a YAML file from disk.
///
/// **Deprecated**: Use `assembly::parser::parse()` for Scroll Assembly format.
pub fn parse_scroll_file(path: &Path) -> Result<Scroll, ParseError> {
    let yaml = std::fs::read_to_string(path)?;
    parse_scroll(&yaml)
}

fn validate_scroll(scroll: &Scroll) -> Result<(), ParseError> {
    if scroll.steps.is_empty() {
        return Err(ParseError::EmptySteps);
    }
    Ok(())
}
