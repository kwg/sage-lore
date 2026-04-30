// SPDX-License-Identifier: MIT
//! Pure deterministic builtin primitives for Scroll Assembly.
//!
//! Builtins are dispatched directly inside the assembly executor — no backend,
//! no interface registry, no I/O. They exist so scrolls can do simple
//! transformations (split a line out of a string, lowercase a field, etc.)
//! without delegating to an LLM (#190).

pub mod string;
