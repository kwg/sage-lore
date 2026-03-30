// SPDX-License-Identifier: MIT
//! Scroll Assembly language module.
//!
//! This module implements the Scroll Assembly language — a strongly-typed
//! replacement for YAML-based scroll definitions. Built alongside the
//! existing YAML parser; cutover replaces it entirely (D9, D22).
//!
//! ## Module Structure
//!
//! - `ast` — AST node types representing parsed scroll structure
//! - `grammar` — pest parser definition (PEG grammar)
//!
//! ## Future modules (S2-S4)
//!
//! - `parser` — pest pairs → AST conversion (S2)
//! - `typechecker` — AST validation (S3)
//! - `dispatch` — AST → primitive execution (S4)

pub mod ast;
pub mod dispatch;
pub mod grammar;
pub mod parser;
pub mod typechecker;

#[cfg(test)]
mod tests;
