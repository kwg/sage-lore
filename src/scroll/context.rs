// SPDX-License-Identifier: MIT
//! Execution context for variable resolution in scroll execution.

use crate::config::SecretResolver;
use std::collections::HashMap;

/// Context for variable resolution during scroll execution.
#[derive(Clone)]
pub struct ExecutionContext {
    /// Named variables from step outputs
    variables: HashMap<String, serde_json::Value>,

    /// Output of the previous step
    prev: Option<serde_json::Value>,

    /// Loop context (if inside a loop)
    loop_context: Option<LoopContext>,

    /// Secret resolver for ${VAR} injection
    secret_resolver: Option<SecretResolver>,
}

/// Context for loop iteration state.
#[derive(Debug, Clone)]
pub struct LoopContext {
    /// Current item variable name (e.g., "item", "chunk")
    pub item_var: String,
    /// Current item value
    pub item: serde_json::Value,
    /// Current iteration index (0-based)
    pub index: usize,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionContext {
    /// Create a new ExecutionContext with empty state and no secret resolver.
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            prev: None,
            loop_context: None,
            secret_resolver: None,
        }
    }

    /// Create a new ExecutionContext with a secret resolver.
    pub fn with_secret_resolver(secret_resolver: SecretResolver) -> Self {
        Self {
            variables: HashMap::new(),
            prev: None,
            loop_context: None,
            secret_resolver: Some(secret_resolver),
        }
    }

    /// Set the secret resolver for this context.
    pub fn set_secret_resolver(&mut self, resolver: SecretResolver) {
        self.secret_resolver = Some(resolver);
    }

    /// Return an immutable reference to the prev value if it exists.
    pub fn prev(&self) -> Option<&serde_json::Value> {
        self.prev.as_ref()
    }

    /// Return an immutable reference to a named variable if it exists.
    pub fn get_variable(&self, name: &str) -> Option<&serde_json::Value> {
        self.variables.get(name)
    }

    /// Insert or overwrite a named variable in the variables HashMap.
    pub fn set_variable(&mut self, name: String, value: serde_json::Value) {
        self.variables.insert(name, value);
    }

    /// Clear/remove a variable from context.
    pub fn clear_variable(&mut self, name: &str) {
        self.variables.remove(name);
    }

    /// Get an iterator over all variables.
    pub fn variables(&self) -> impl Iterator<Item = (&String, &serde_json::Value)> {
        self.variables.iter()
    }

    /// Set the previous step output value.
    pub fn set_prev(&mut self, value: serde_json::Value) {
        self.prev = Some(value);
    }

    /// Clear the previous step output by setting prev to None.
    pub fn clear_prev(&mut self) {
        self.prev = None;
    }

    /// Enter a loop context by setting the current item, item variable name, and iteration index.
    pub fn set_loop_context(&mut self, item_var: String, item: serde_json::Value, index: usize) {
        self.loop_context = Some(LoopContext {
            item_var,
            item,
            index,
        });
    }

    /// Exit the loop context by setting loop_context to None.
    pub fn clear_loop_context(&mut self) {
        self.loop_context = None;
    }

    /// Strictly resolve `${var}` references — errors on unresolved variables.
    ///
    /// Use this for primary inputs where an unresolved variable is a bug.
    /// Full-value references (`"${var}"` as entire string) propagate errors.
    /// Embedded references (`"prefix ${var} suffix"`) also propagate errors.
    /// Objects and arrays recurse strictly into children.
    pub fn resolve_value_strict(&self, value: &serde_json::Value) -> Result<serde_json::Value, ResolveError> {
        match value {
            serde_json::Value::String(s) => {
                // Full-value reference: entire string is "${var}"
                if s.starts_with("${") && s.ends_with('}') && s.matches("${").count() == 1 {
                    return self.resolve(s);
                }
                // Embedded references: "text ${var} more text"
                // Uses advance-past pattern: after resolving a ${var}, the scan
                // advances past the replacement so resolved content (e.g. Python
                // f-strings like ${total:.2f}) is never re-scanned as a variable.
                if s.contains("${") {
                    let mut result = String::new();
                    let mut remaining = s.as_str();
                    while let Some(start) = remaining.find("${") {
                        result.push_str(&remaining[..start]);
                        let after_start = &remaining[start + 2..];
                        if let Some(end) = after_start.find('}') {
                            let ref_str = &remaining[start..start + 2 + end + 1];
                            let resolved = self.resolve(ref_str)?;
                            let replacement = match &resolved {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            result.push_str(&replacement);
                            remaining = &remaining[start + 2 + end + 1..];
                        } else {
                            // Unclosed ${ — push it literally and stop
                            result.push_str(&remaining[start..]);
                            remaining = "";
                            break;
                        }
                    }
                    result.push_str(remaining);
                    return Ok(serde_json::Value::String(result));
                }
                Ok(value.clone())
            }
            serde_json::Value::Object(map) => {
                let resolved: Result<serde_json::Map<String, serde_json::Value>, _> = map
                    .iter()
                    .map(|(k, v)| self.resolve_value_strict(v).map(|rv| (k.clone(), rv)))
                    .collect();
                Ok(serde_json::Value::Object(resolved?))
            }
            serde_json::Value::Array(arr) => {
                let resolved: Result<Vec<serde_json::Value>, _> = arr
                    .iter()
                    .map(|v| self.resolve_value_strict(v))
                    .collect();
                Ok(serde_json::Value::Array(resolved?))
            }
            // Numbers, bools, null — pass through
            _ => Ok(value.clone()),
        }
    }

    /// Recursively resolve `${var}` references within a JSON value (best-effort).
    ///
    /// Walks the JSON tree. For string values, replaces `${var}` patterns
    /// with their resolved values. For objects and arrays, recurses into children.
    /// Non-string scalars (numbers, bools, null) are returned unchanged.
    ///
    /// Supports both full-value references (`"${var}"` → the resolved value directly)
    /// and embedded references (`"prefix ${var} suffix"` → interpolated string).
    ///
    /// NOTE: This is best-effort — unresolved variables are left as-is.
    /// Use resolve_value_strict() for primary inputs where unresolved vars are errors.
    pub fn resolve_value(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => {
                // Full-value reference: entire string is "${var}"
                if s.starts_with("${") && s.ends_with('}') && s.matches("${").count() == 1 {
                    if let Ok(resolved) = self.resolve(s) {
                        return resolved;
                    }
                }
                // Embedded references: "text ${var} more text"
                // Uses advance-past pattern: resolved content is never re-scanned.
                // Best-effort: unresolvable refs are passed through literally.
                if s.contains("${") {
                    let mut result = String::new();
                    let mut remaining = s.as_str();
                    while let Some(start) = remaining.find("${") {
                        result.push_str(&remaining[..start]);
                        let after_start = &remaining[start + 2..];
                        if let Some(end) = after_start.find('}') {
                            let ref_str = &remaining[start..start + 2 + end + 1];
                            if let Ok(resolved) = self.resolve(ref_str) {
                                let replacement = match &resolved {
                                    serde_json::Value::String(s) => s.clone(),
                                    other => other.to_string(),
                                };
                                result.push_str(&replacement);
                            } else {
                                // Can't resolve — pass through literally
                                result.push_str(ref_str);
                            }
                            remaining = &remaining[start + 2 + end + 1..];
                        } else {
                            // Unclosed ${ — push it literally and stop
                            result.push_str(&remaining[start..]);
                            remaining = "";
                            break;
                        }
                    }
                    result.push_str(remaining);
                    return serde_json::Value::String(result);
                }
                value.clone()
            }
            serde_json::Value::Object(map) => {
                let resolved: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), self.resolve_value(v)))
                    .collect();
                serde_json::Value::Object(resolved)
            }
            serde_json::Value::Array(arr) => {
                let resolved: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|v| self.resolve_value(v))
                    .collect();
                serde_json::Value::Array(resolved)
            }
            // Numbers, bools, null — pass through
            _ => value.clone(),
        }
    }

    /// Split a path at the first dot into (variable_name, remaining_path).
    fn split_path<'a>(&self, s: &'a str) -> (&'a str, &'a str) {
        match s.find('.') {
            Some(i) => (&s[..i], &s[i + 1..]),
            None => (s, ""),
        }
    }

    /// Recursively navigate a dot-separated path through a Value, handling Mapping access and errors.
    fn resolve_path(
        &self,
        path: &str,
        value: &serde_json::Value,
    ) -> Result<serde_json::Value, ResolveError> {
        if path.is_empty() {
            return Ok(value.clone());
        }

        let (field, rest) = self.split_path(path);

        match value {
            serde_json::Value::Object(map) => {
                let next = map
                    .get(field)
                    .ok_or_else(|| ResolveError::FieldNotFound(field.to_string()))?;
                self.resolve_path(rest, next)
            }
            _ => Err(ResolveError::NotAMapping(field.to_string())),
        }
    }

    /// Resolve a variable reference string like "${name}" or "${name.field}" to its serde_json::Value.
    ///
    /// Resolution order for non-special variables:
    /// 1. Loop context variables (if in a loop)
    /// 2. Named variables from scroll execution
    /// 3. Secrets (via SecretResolver: env → .env → secrets.yaml)
    pub fn resolve(&self, reference: &str) -> Result<serde_json::Value, ResolveError> {
        // Strip ${ and }
        let inner = reference
            .strip_prefix("${")
            .and_then(|s| s.strip_suffix("}"))
            .ok_or(ResolveError::InvalidSyntax)?;

        // Handle special cases
        match inner {
            "prev" => self.prev.clone().ok_or(ResolveError::NoPrevious),
            "loop_index" => {
                let ctx = self.loop_context.as_ref().ok_or(ResolveError::NotInLoop)?;
                Ok(serde_json::Value::Number(serde_json::Number::from(
                    ctx.index as u64,
                )))
            }
            _ => {
                // Check loop item first (shadows scroll variables)
                if let Some(ctx) = &self.loop_context {
                    if inner == ctx.item_var {
                        return Ok(ctx.item.clone());
                    }
                    if let Some(rest) = inner.strip_prefix(&format!("{}.", ctx.item_var)) {
                        return self.resolve_path(rest, &ctx.item);
                    }
                }

                // Check named variables
                let (var_name, path) = self.split_path(inner);

                if let Some(value) = self.variables.get(var_name) {
                    if path.is_empty() {
                        return Ok(value.clone());
                    } else {
                        return self.resolve_path(path, value);
                    }
                }

                // If not in variables, try secret resolution (only for simple names, not paths)
                if path.is_empty() {
                    if let Some(resolver) = &self.secret_resolver {
                        if let Some(secret_value) = resolver.resolve(var_name) {
                            // SECURITY: Do NOT log the secret value
                            return Ok(serde_json::Value::String(secret_value));
                        }
                    }
                }

                // Not found anywhere
                Err(ResolveError::Undefined(var_name.to_string()))
            }
        }
    }
}

/// Errors that can occur during variable resolution.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Invalid variable syntax")]
    InvalidSyntax,

    #[error("No previous step output available")]
    NoPrevious,

    #[error("Not inside a loop context")]
    NotInLoop,

    #[error("Undefined variable: {0}")]
    Undefined(String),

    #[error("Field not found: {0}")]
    FieldNotFound(String),

    #[error("Cannot access field on non-mapping: {0}")]
    NotAMapping(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // Constructor Tests

    #[test]
    fn test_new_creates_empty_context() {
        let ctx = ExecutionContext::new();
        assert!(ctx.prev.is_none());
        assert!(ctx.variables.is_empty());
        assert!(ctx.loop_context.is_none());
    }

    // Accessor Tests

    #[test]
    fn test_prev_returns_none_when_not_set() {
        let ctx = ExecutionContext::new();
        assert!(ctx.prev().is_none());
    }

    #[test]
    fn test_prev_returns_value_when_set() {
        let mut ctx = ExecutionContext::new();
        let val = serde_json::Value::String("test".to_string());
        ctx.set_prev(val.clone());
        assert_eq!(ctx.prev().unwrap().as_str().unwrap(), "test");
    }

    #[test]
    fn test_get_variable_returns_none_when_not_set() {
        let ctx = ExecutionContext::new();
        assert!(ctx.get_variable("name").is_none());
    }

    #[test]
    fn test_get_variable_returns_value_when_set() {
        let mut ctx = ExecutionContext::new();
        let val = serde_json::Value::String("test".to_string());
        ctx.set_variable("name".to_string(), val);
        assert_eq!(ctx.get_variable("name").unwrap().as_str().unwrap(), "test");
    }

    // Mutator Tests

    #[test]
    fn test_set_variable_inserts_new_variable() {
        let mut ctx = ExecutionContext::new();
        let val = serde_json::Value::String("value".to_string());
        ctx.set_variable("name".to_string(), val);
        assert!(ctx.variables.contains_key("name"));
    }

    #[test]
    fn test_set_variable_overwrites_existing() {
        let mut ctx = ExecutionContext::new();
        ctx.set_variable(
            "name".to_string(),
            serde_json::Value::String("first".to_string()),
        );
        ctx.set_variable(
            "name".to_string(),
            serde_json::Value::String("second".to_string()),
        );
        assert_eq!(
            ctx.get_variable("name").unwrap().as_str().unwrap(),
            "second"
        );
    }

    #[test]
    fn test_set_prev_updates_value() {
        let mut ctx = ExecutionContext::new();
        let val = serde_json::Value::String("prev_value".to_string());
        ctx.set_prev(val);
        assert!(ctx.prev.is_some());
    }

    #[test]
    fn test_clear_prev_resets_to_none() {
        let mut ctx = ExecutionContext::new();
        ctx.set_prev(serde_json::Value::String("value".to_string()));
        ctx.clear_prev();
        assert!(ctx.prev.is_none());
    }

    // Loop Context Tests

    #[test]
    fn test_set_loop_context_creates_context() {
        let mut ctx = ExecutionContext::new();
        let item = serde_json::Value::String("item_value".to_string());
        ctx.set_loop_context("item".to_string(), item, 0);
        assert!(ctx.loop_context.is_some());
        let loop_ctx = ctx.loop_context.as_ref().unwrap();
        assert_eq!(loop_ctx.item_var, "item");
        assert_eq!(loop_ctx.index, 0);
    }

    #[test]
    fn test_clear_loop_context_resets_to_none() {
        let mut ctx = ExecutionContext::new();
        ctx.set_loop_context(
            "item".to_string(),
            serde_json::Value::String("val".to_string()),
            0,
        );
        ctx.clear_loop_context();
        assert!(ctx.loop_context.is_none());
    }

    // split_path Tests

    #[test]
    fn test_split_path_no_dot() {
        let ctx = ExecutionContext::new();
        let (name, path) = ctx.split_path("name");
        assert_eq!(name, "name");
        assert_eq!(path, "");
    }

    #[test]
    fn test_split_path_single_dot() {
        let ctx = ExecutionContext::new();
        let (name, path) = ctx.split_path("name.field");
        assert_eq!(name, "name");
        assert_eq!(path, "field");
    }

    #[test]
    fn test_split_path_multiple_dots() {
        let ctx = ExecutionContext::new();
        let (name, path) = ctx.split_path("name.field.sub");
        assert_eq!(name, "name");
        assert_eq!(path, "field.sub");
    }

    // resolve_path Tests

    #[test]
    fn test_resolve_path_empty_path_returns_value() {
        let ctx = ExecutionContext::new();
        let val = serde_json::Value::String("test".to_string());
        let result = ctx.resolve_path("", &val).unwrap();
        assert_eq!(result.as_str().unwrap(), "test");
    }

    #[test]
    fn test_resolve_path_single_field() {
        let ctx = ExecutionContext::new();
        let mut map = serde_json::Map::new();
        map.insert("field".to_string(),
            serde_json::Value::String("value".to_string()),
        );
        let val = serde_json::Value::Object(map);
        let result = ctx.resolve_path("field", &val).unwrap();
        assert_eq!(result.as_str().unwrap(), "value");
    }

    #[test]
    fn test_resolve_path_nested_fields() {
        let ctx = ExecutionContext::new();
        let mut inner_map = serde_json::Map::new();
        inner_map.insert("sub".to_string(),
            serde_json::Value::String("deep_value".to_string()),
        );
        let mut outer_map = serde_json::Map::new();
        outer_map.insert("field".to_string(),
            serde_json::Value::Object(inner_map),
        );
        let val = serde_json::Value::Object(outer_map);
        let result = ctx.resolve_path("field.sub", &val).unwrap();
        assert_eq!(result.as_str().unwrap(), "deep_value");
    }

    #[test]
    fn test_resolve_path_field_not_found() {
        let ctx = ExecutionContext::new();
        let map = serde_json::Map::new();
        let val = serde_json::Value::Object(map);
        let result = ctx.resolve_path("missing", &val);
        assert!(matches!(result, Err(ResolveError::FieldNotFound(_))));
    }

    #[test]
    fn test_resolve_path_not_a_mapping() {
        let ctx = ExecutionContext::new();
        let val = serde_json::Value::String("not a map".to_string());
        let result = ctx.resolve_path("field", &val);
        assert!(matches!(result, Err(ResolveError::NotAMapping(_))));
    }

    // resolve() Special Cases Tests

    #[test]
    fn test_resolve_prev_when_set() {
        let mut ctx = ExecutionContext::new();
        let val = serde_json::Value::String("previous".to_string());
        ctx.set_prev(val);
        let result = ctx.resolve("${prev}").unwrap();
        assert_eq!(result.as_str().unwrap(), "previous");
    }

    #[test]
    fn test_resolve_prev_error_when_not_set() {
        let ctx = ExecutionContext::new();
        let result = ctx.resolve("${prev}");
        assert!(matches!(result, Err(ResolveError::NoPrevious)));
    }

    #[test]
    fn test_resolve_loop_index_when_in_loop() {
        let mut ctx = ExecutionContext::new();
        ctx.set_loop_context(
            "item".to_string(),
            serde_json::Value::String("val".to_string()),
            5,
        );
        let result = ctx.resolve("${loop_index}").unwrap();
        assert_eq!(result.as_u64().unwrap(), 5u64);
    }

    #[test]
    fn test_resolve_loop_index_error_when_not_in_loop() {
        let ctx = ExecutionContext::new();
        let result = ctx.resolve("${loop_index}");
        assert!(matches!(result, Err(ResolveError::NotInLoop)));
    }

    #[test]
    fn test_resolve_loop_item_variable_item() {
        let mut ctx = ExecutionContext::new();
        let item = serde_json::Value::String("item_value".to_string());
        ctx.set_loop_context("item".to_string(), item.clone(), 0);
        let result = ctx.resolve("${item}").unwrap();
        assert_eq!(result.as_str().unwrap(), "item_value");
    }

    #[test]
    fn test_resolve_loop_item_variable_chunk() {
        let mut ctx = ExecutionContext::new();
        let item = serde_json::Value::String("chunk_value".to_string());
        ctx.set_loop_context("chunk".to_string(), item.clone(), 0);
        let result = ctx.resolve("${chunk}").unwrap();
        assert_eq!(result.as_str().unwrap(), "chunk_value");
    }

    #[test]
    fn test_resolve_loop_item_field_access() {
        let mut ctx = ExecutionContext::new();
        let mut map = serde_json::Map::new();
        map.insert("field".to_string(),
            serde_json::Value::String("field_value".to_string()),
        );
        let item = serde_json::Value::Object(map);
        ctx.set_loop_context("item".to_string(), item, 0);
        let result = ctx.resolve("${item.field}").unwrap();
        assert_eq!(result.as_str().unwrap(), "field_value");
    }

    // resolve() Named Variable Tests

    #[test]
    fn test_resolve_named_variable() {
        let mut ctx = ExecutionContext::new();
        ctx.set_variable(
            "name".to_string(),
            serde_json::Value::String("value".to_string()),
        );
        let result = ctx.resolve("${name}").unwrap();
        assert_eq!(result.as_str().unwrap(), "value");
    }

    #[test]
    fn test_resolve_undefined_variable() {
        let ctx = ExecutionContext::new();
        let result = ctx.resolve("${missing}");
        assert!(matches!(result, Err(ResolveError::Undefined(_))));
    }

    #[test]
    fn test_resolve_variable_field() {
        let mut ctx = ExecutionContext::new();
        let mut map = serde_json::Map::new();
        map.insert("field".to_string(),
            serde_json::Value::String("value".to_string()),
        );
        ctx.set_variable("name".to_string(), serde_json::Value::Object(map));
        let result = ctx.resolve("${name.field}").unwrap();
        assert_eq!(result.as_str().unwrap(), "value");
    }

    // resolve() Error Handling Tests

    #[test]
    fn test_resolve_invalid_syntax_no_prefix() {
        let ctx = ExecutionContext::new();
        let result = ctx.resolve("name}");
        assert!(matches!(result, Err(ResolveError::InvalidSyntax)));
    }

    #[test]
    fn test_resolve_invalid_syntax_no_suffix() {
        let ctx = ExecutionContext::new();
        let result = ctx.resolve("${name");
        assert!(matches!(result, Err(ResolveError::InvalidSyntax)));
    }

    #[test]
    fn test_resolve_invalid_syntax_missing_both() {
        let ctx = ExecutionContext::new();
        let result = ctx.resolve("name");
        assert!(matches!(result, Err(ResolveError::InvalidSyntax)));
    }

    // Shadowing Tests

    #[test]
    fn test_loop_variable_shadows_scroll_variable() {
        let mut ctx = ExecutionContext::new();
        // Set scroll-level variable named "item"
        ctx.set_variable(
            "item".to_string(),
            serde_json::Value::String("scroll_value".to_string()),
        );
        // Enter loop context with same variable name
        let loop_item = serde_json::Value::String("loop_value".to_string());
        ctx.set_loop_context("item".to_string(), loop_item, 0);
        // Resolve should return loop item, not scroll variable
        let result = ctx.resolve("${item}").unwrap();
        assert_eq!(result.as_str().unwrap(), "loop_value");
    }

    // ========================================================================
    // Secret Resolution Tests
    // ========================================================================

    #[test]
    fn test_resolve_secret_from_env() {
        use crate::config::SecretResolver;
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // Set environment variable
        env::set_var("SECRET_FROM_ENV", "env_value");

        let resolver = SecretResolver::new(temp_dir.path());
        let ctx = ExecutionContext::with_secret_resolver(resolver);

        let result = ctx.resolve("${SECRET_FROM_ENV}").unwrap();
        assert_eq!(result.as_str().unwrap(), "env_value");

        // Cleanup
        env::remove_var("SECRET_FROM_ENV");
    }

    #[test]
    fn test_resolve_secret_from_dotenv() {
        use crate::config::SecretResolver;
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let dotenv_path = temp_dir.path().join(".env");

        fs::write(&dotenv_path, "SECRET_FROM_DOTENV=dotenv_value\n").unwrap();

        let resolver = SecretResolver::new(temp_dir.path());
        let ctx = ExecutionContext::with_secret_resolver(resolver);

        let result = ctx.resolve("${SECRET_FROM_DOTENV}").unwrap();
        assert_eq!(result.as_str().unwrap(), "dotenv_value");
    }

    #[test]
    fn test_secret_resolution_priority() {
        use crate::config::SecretResolver;
        use std::env;
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let dotenv_path = temp_dir.path().join(".env");

        // Create .env with a value
        fs::write(&dotenv_path, "PRIORITY_TEST=dotenv_value\n").unwrap();

        // Set env var (should win over .env)
        env::set_var("PRIORITY_TEST", "env_value");

        let resolver = SecretResolver::new(temp_dir.path());
        let ctx = ExecutionContext::with_secret_resolver(resolver);

        let result = ctx.resolve("${PRIORITY_TEST}").unwrap();
        assert_eq!(result.as_str().unwrap(), "env_value");

        // Cleanup
        env::remove_var("PRIORITY_TEST");
    }

    #[test]
    fn test_scroll_variables_shadow_secrets() {
        use crate::config::SecretResolver;
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // Set environment variable
        env::set_var("SHADOW_TEST", "secret_value");

        let resolver = SecretResolver::new(temp_dir.path());
        let mut ctx = ExecutionContext::with_secret_resolver(resolver);

        // Set scroll variable with same name (should shadow secret)
        ctx.set_variable(
            "SHADOW_TEST".to_string(),
            serde_json::Value::String("scroll_value".to_string()),
        );

        let result = ctx.resolve("${SHADOW_TEST}").unwrap();
        assert_eq!(result.as_str().unwrap(), "scroll_value");

        // Cleanup
        env::remove_var("SHADOW_TEST");
    }

    #[test]
    fn test_resolve_without_secret_resolver() {
        let ctx = ExecutionContext::new();

        // Should fail when trying to resolve undefined variable
        let result = ctx.resolve("${SOME_SECRET}");
        assert!(matches!(result, Err(ResolveError::Undefined(_))));
    }

    #[test]
    fn test_resolve_secret_not_found() {
        use crate::config::SecretResolver;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let resolver = SecretResolver::new(temp_dir.path());
        let ctx = ExecutionContext::with_secret_resolver(resolver);

        let result = ctx.resolve("${NONEXISTENT_SECRET}");
        assert!(matches!(result, Err(ResolveError::Undefined(_))));
    }

    #[test]
    fn test_secrets_do_not_support_paths() {
        use crate::config::SecretResolver;
        use std::env;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        env::set_var("SECRET_VAR", "secret_value");

        let resolver = SecretResolver::new(temp_dir.path());
        let ctx = ExecutionContext::with_secret_resolver(resolver);

        // Secrets don't support field access
        let result = ctx.resolve("${SECRET_VAR.field}");
        assert!(matches!(result, Err(ResolveError::Undefined(_))));

        // Cleanup
        env::remove_var("SECRET_VAR");
    }

    // =========================================================================
    // Interpolation: no re-scanning of resolved content (#161)
    // =========================================================================

    #[test]
    fn test_strict_embedded_does_not_rescan_replacement() {
        // Simulates LLM output containing ${total:.2f} (Python f-string)
        // stored in a variable, then used in an embedded template.
        let mut ctx = ExecutionContext::new();
        ctx.set_variable(
            "code".to_string(),
            serde_json::Value::String("print(f\"${total:.2f}\")".to_string()),
        );
        // Template references ${code} — the resolved value contains ${total:.2f}
        // which must NOT be re-scanned as a variable reference.
        let template = serde_json::Value::String("Result: ${code} done".to_string());
        let result = ctx.resolve_value_strict(&template).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            "Result: print(f\"${total:.2f}\") done"
        );
    }

    #[test]
    fn test_best_effort_embedded_does_not_rescan_replacement() {
        let mut ctx = ExecutionContext::new();
        ctx.set_variable(
            "output".to_string(),
            serde_json::Value::String("cost is ${amount}".to_string()),
        );
        let template = serde_json::Value::String("The ${output} end".to_string());
        let result = ctx.resolve_value(&template);
        // ${output} resolves, but the ${amount} inside the resolved value
        // should be left as-is (not resolved, not errored)
        assert_eq!(
            result.as_str().unwrap(),
            "The cost is ${amount} end"
        );
    }

    #[test]
    fn test_strict_embedded_multiple_vars_no_rescan() {
        let mut ctx = ExecutionContext::new();
        ctx.set_variable(
            "a".to_string(),
            serde_json::Value::String("${b}".to_string()),
        );
        ctx.set_variable(
            "b".to_string(),
            serde_json::Value::String("resolved_b".to_string()),
        );
        // Template has ${a} then ${b}. After resolving ${a} to "${b}",
        // the scanner must NOT re-resolve that "${b}" — only the original ${b} at the end.
        let template = serde_json::Value::String("${a} and ${b}".to_string());
        let result = ctx.resolve_value_strict(&template).unwrap();
        assert_eq!(result.as_str().unwrap(), "${b} and resolved_b");
    }

    #[test]
    fn test_best_effort_unresolvable_ref_passed_through() {
        let ctx = ExecutionContext::new();
        let template = serde_json::Value::String("hello ${unknown} world".to_string());
        let result = ctx.resolve_value(&template);
        assert_eq!(result.as_str().unwrap(), "hello ${unknown} world");
    }

    #[test]
    fn test_strict_full_value_ref_not_rescanned() {
        // Full-value reference: the resolved string itself contains ${...}
        // but since it's returned as-is (not embedded), no re-scanning occurs.
        let mut ctx = ExecutionContext::new();
        ctx.set_variable(
            "code".to_string(),
            serde_json::Value::String("f\"${x:.2f}\"".to_string()),
        );
        let template = serde_json::Value::String("${code}".to_string());
        let result = ctx.resolve_value_strict(&template).unwrap();
        assert_eq!(result.as_str().unwrap(), "f\"${x:.2f}\"");
    }
}
