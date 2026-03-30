// SPDX-License-Identifier: MIT
//! Pest parser definition for the Scroll Assembly language.
//!
//! This module derives the pest parser from the PEG grammar file.
//! The generated `Rule` enum and `ScrollAssemblyParser` struct are
//! used by the parser module (S2) to convert source text into ASTs.

use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "scroll/assembly/scroll_assembly.pest"]
pub struct ScrollAssemblyParser;
