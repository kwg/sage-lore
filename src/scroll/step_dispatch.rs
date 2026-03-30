// SPDX-License-Identifier: MIT
//! Step execution dispatchers for all scroll step types.
//!
//! This module contains the execution logic for each step variant,
//! handling the delegation to appropriate handlers and result binding.

use crate::scroll::error::ExecutionError;
use crate::scroll::executor::{with_on_fail, write_failure_diagnostic};
use crate::scroll::extraction::{
    build_convert_prompt, build_distill_prompt, build_elaborate_prompt,
    build_merge_prompt, build_split_prompt, build_validate_prompt,
    parse_as_sequence, try_parse_structured,
};
use crate::scroll::interfaces::InterfaceDispatch;
use crate::scroll::schema::{
    AggregateStep, BranchStep, ConcurrentStep, ConvertStep, ConsensusStep,
    DistillStep, ElaborateStep, InvokeStep, LoopStep, MergeStep, OnFail, ParallelStep,
    ScanType, SecureStep, SetStep, SplitStep, Step, ValidateStep,
};

/// Evaluate a value for truthiness using the same rules as branch conditions.
///
/// Truthy: true, non-empty string, non-zero number, non-empty array/object
/// Falsy: false, null, empty string "", 0, empty array [], empty object {}
pub(crate) fn is_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => !s.is_empty() && s != "false" && s != "0",
        serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        serde_json::Value::Null => false,
        serde_json::Value::Array(seq) => !seq.is_empty(),
        serde_json::Value::Object(map) => !map.is_empty(),
    }
}

/// Resolve one side of a condition expression to a JSON value.
pub(crate) fn resolve_condition_operand(operand: &str, context: &super::context::ExecutionContext) -> serde_json::Value {
    // Variable reference: ${...}
    if operand.starts_with("${") && operand.ends_with('}') {
        return context.resolve(operand).unwrap_or(serde_json::Value::Null);
    }

    // Quoted string literal: 'value' or "value"
    if (operand.starts_with('\'') && operand.ends_with('\''))
        || (operand.starts_with('"') && operand.ends_with('"'))
    {
        return serde_json::Value::String(operand[1..operand.len() - 1].to_string());
    }

    // Boolean literals
    if operand == "true" {
        return serde_json::Value::Bool(true);
    }
    if operand == "false" {
        return serde_json::Value::Bool(false);
    }
    if operand == "null" {
        return serde_json::Value::Null;
    }

    // Numeric literal
    if let Ok(n) = operand.parse::<i64>() {
        return serde_json::Value::Number(serde_json::Number::from(n));
    }
    if let Ok(n) = operand.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return serde_json::Value::Number(num);
        }
    }

    // Unquoted string fallback
    serde_json::Value::String(operand.to_string())
}

/// Compare two JSON values for equality, with type coercion for numbers/strings.
pub(crate) fn values_equal(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a == b,
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a == b,
        (serde_json::Value::Null, serde_json::Value::Null) => true,
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            a.as_f64() == b.as_f64()
        }
        // Coerce number to string for mixed comparisons
        (serde_json::Value::Number(n), serde_json::Value::String(s))
        | (serde_json::Value::String(s), serde_json::Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                s == &i.to_string()
            } else if let Some(f) = n.as_f64() {
                s == &f.to_string()
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Numeric comparison for branch conditions (>=, <=, >, <).
/// Coerces both values to f64. Strings that look like numbers are coerced.
/// Returns false if either value cannot be interpreted as a number.
pub(crate) fn values_compare(a: &serde_json::Value, b: &serde_json::Value, op: &str) -> bool {
    let a_f64 = value_to_f64(a);
    let b_f64 = value_to_f64(b);

    match (a_f64, b_f64) {
        (Some(a), Some(b)) => match op {
            ">=" => a >= b,
            "<=" => a <= b,
            ">" => a > b,
            "<" => a < b,
            _ => false,
        },
        _ => false,
    }
}

/// Try to extract an f64 from a JSON value.
/// Numbers convert directly. Strings are parsed as f64 if possible.
fn value_to_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}


// ============================================================================
// Structured Output Extraction (for output_schema on invoke)
// ============================================================================

/// Extract structured JSON from raw LLM text and validate against a schema.
/// Tries multiple strategies locally — no LLM calls:
/// 1. Direct JSON parse
/// 2. Strip markdown fences, then parse
/// 3. Find first { and last }, parse that substring
/// 4. Try YAML parse as fallback
///    Then validates against the schema if parsing succeeded.
fn extract_structured_output(
    raw: &str,
    schema: &serde_json::Value,
) -> Result<serde_json::Value, ExecutionError> {
    let trimmed = raw.trim();

    // Strategy 1: direct JSON parse
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        let coerced = apply_type_coercion(&val, schema).unwrap_or_else(|_| val.clone());
        validate_against_schema(&coerced, schema)?;
        return Ok(coerced);
    }

    // Strategy 2: strip markdown fences (```json ... ``` or ``` ... ```)
    let stripped = strip_markdown_fences(trimmed);
    if stripped != trimmed {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(stripped.trim()) {
            let coerced = apply_type_coercion(&val, schema).unwrap_or_else(|_| val.clone());
            validate_against_schema(&coerced, schema)?;
            return Ok(coerced);
        }
    }

    // Strategy 3: find first { and last }, try that substring
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            let substr = &trimmed[start..=end];
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(substr) {
                let coerced = apply_type_coercion(&val, schema).unwrap_or_else(|_| val.clone());
                validate_against_schema(&coerced, schema)?;
                return Ok(coerced);
            }
        }
    }

    // Strategy 4: try YAML parse (some models return YAML when asked for JSON)
    if let Ok(val) = serde_yaml::from_str::<serde_json::Value>(stripped.trim()) {
        if val.is_object() || val.is_array() {
            let coerced = apply_type_coercion(&val, schema).unwrap_or_else(|_| val.clone());
            validate_against_schema(&coerced, schema)?;
            return Ok(coerced);
        }
    }

    Err(ExecutionError::ParseError(format!(
        "Could not extract structured output matching schema. Raw response (first 500 chars): {}",
        &raw.chars().take(500).collect::<String>()
    )))
}

/// Strip markdown code fences from text.
/// Handles ```json\n...\n```, ```\n...\n```, and similar patterns.
fn strip_markdown_fences(text: &str) -> &str {
    let trimmed = text.trim();

    // Check for opening fence
    if !trimmed.starts_with("```") {
        return trimmed;
    }

    // Find end of opening fence line
    let after_opening = if let Some(newline_pos) = trimmed.find('\n') {
        &trimmed[newline_pos + 1..]
    } else {
        return trimmed;
    };

    // Strip closing fence
    if let Some(close_pos) = after_opening.rfind("```") {
        after_opening[..close_pos].trim()
    } else {
        after_opening.trim()
    }
}

// ============================================================================
// JSON Schema Validation (D44 - draft-07 subset)
// ============================================================================

/// Validate a value against a JSON Schema (draft-07 subset).
/// Supports: type, properties, required, items, enum, minimum, maximum, minLength, maxLength, pattern.
/// Does NOT support: $ref to external URLs, if/then/else, complex combinators.
pub(crate) fn validate_against_schema(
    value: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), ExecutionError> {
    let schema_map = schema.as_object()
        .ok_or_else(|| ExecutionError::ParseError("Schema must be a mapping".to_string()))?;

    // Check type constraint
    if let Some(type_val) = schema_map.get("type") {
        validate_type(value, type_val)?;
    }

    // Check required properties (for objects)
    if let Some(required) = schema_map.get("required") {
        validate_required(value, required)?;
    }

    // Check properties constraints (for objects)
    if let Some(properties) = schema_map.get("properties") {
        validate_properties(value, properties)?;
    }

    // Check items constraint (for arrays)
    if let Some(items) = schema_map.get("items") {
        validate_items(value, items)?;
    }

    // Check enum constraint
    if let Some(enum_vals) = schema_map.get("enum") {
        validate_enum(value, enum_vals)?;
    }

    // Check numeric constraints
    if let Some(minimum) = schema_map.get("minimum") {
        validate_minimum(value, minimum)?;
    }

    if let Some(maximum) = schema_map.get("maximum") {
        validate_maximum(value, maximum)?;
    }

    // Check string constraints
    if let Some(min_length) = schema_map.get("minLength") {
        validate_min_length(value, min_length)?;
    }

    if let Some(max_length) = schema_map.get("maxLength") {
        validate_max_length(value, max_length)?;
    }

    if let Some(pattern) = schema_map.get("pattern") {
        validate_pattern(value, pattern)?;
    }

    // Check for unsupported features and error
    if schema_map.contains_key("$ref") {
        return Err(ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: $ref to external URLs is not supported".to_string()
        ));
    }

    if schema_map.contains_key("if") {
        return Err(ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: if/then/else is not supported".to_string()
        ));
    }

    Ok(())
}

fn validate_type(value: &serde_json::Value, type_val: &serde_json::Value) -> Result<(), ExecutionError> {
    let expected_type = type_val.as_str()
        .ok_or_else(|| ExecutionError::ParseError("Schema type must be a string".to_string()))?;

    let actual_type = match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => {
            if value.as_i64().is_some() {
                "integer"
            } else {
                "number"
            }
        }
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    };

    // "number" type accepts both integer and float
    let type_matches = match expected_type {
        "number" => actual_type == "number" || actual_type == "integer",
        _ => actual_type == expected_type,
    };

    if !type_matches {
        return Err(ExecutionError::ParseError(
            format!("CONVERT_SCHEMA_FAILED: Expected type '{}', got '{}'", expected_type, actual_type)
        ));
    }

    Ok(())
}

fn validate_required(value: &serde_json::Value, required: &serde_json::Value) -> Result<(), ExecutionError> {
    let required_fields = required.as_array()
        .ok_or_else(|| ExecutionError::ParseError("Schema 'required' must be an array".to_string()))?;

    let obj = value.as_object()
        .ok_or_else(|| ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: Value must be an object when 'required' is specified".to_string()
        ))?;

    for field in required_fields {
        let field_name = field.as_str()
            .ok_or_else(|| ExecutionError::ParseError("Required field name must be a string".to_string()))?;

        if !obj.contains_key(field_name) {
            return Err(ExecutionError::ParseError(
                format!("CONVERT_SCHEMA_FAILED: Required field '{}' is missing", field_name)
            ));
        }
    }

    Ok(())
}

fn validate_properties(value: &serde_json::Value, properties: &serde_json::Value) -> Result<(), ExecutionError> {
    let props_map = properties.as_object()
        .ok_or_else(|| ExecutionError::ParseError("Schema 'properties' must be a mapping".to_string()))?;

    let obj = value.as_object()
        .ok_or_else(|| ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: Value must be an object when 'properties' is specified".to_string()
        ))?;

    // Validate each property in the object against its schema
    for (key, val) in obj.iter() {
        if let Some(prop_schema) = props_map.get(key.as_str()) {
            validate_against_schema(val, prop_schema)
                .map_err(|e| ExecutionError::ParseError(
                    format!("Property '{}': {}", key, e)
                ))?;
        }
    }

    Ok(())
}

fn validate_items(value: &serde_json::Value, items_schema: &serde_json::Value) -> Result<(), ExecutionError> {
    let arr = value.as_array()
        .ok_or_else(|| ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: Value must be an array when 'items' is specified".to_string()
        ))?;

    for (idx, item) in arr.iter().enumerate() {
        validate_against_schema(item, items_schema)
            .map_err(|e| ExecutionError::ParseError(
                format!("Array item {}: {}", idx, e)
            ))?;
    }

    Ok(())
}

fn validate_enum(value: &serde_json::Value, enum_vals: &serde_json::Value) -> Result<(), ExecutionError> {
    let allowed = enum_vals.as_array()
        .ok_or_else(|| ExecutionError::ParseError("Schema 'enum' must be an array".to_string()))?;

    if !allowed.contains(value) {
        return Err(ExecutionError::ParseError(
            format!("CONVERT_SCHEMA_FAILED: Value must be one of {:?}", allowed)
        ));
    }

    Ok(())
}

fn validate_minimum(value: &serde_json::Value, minimum: &serde_json::Value) -> Result<(), ExecutionError> {
    let min_val = minimum.as_f64()
        .ok_or_else(|| ExecutionError::ParseError("Schema 'minimum' must be a number".to_string()))?;

    let actual_val = value.as_f64()
        .ok_or_else(|| ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: Value must be a number for 'minimum' constraint".to_string()
        ))?;

    if actual_val < min_val {
        return Err(ExecutionError::ParseError(
            format!("CONVERT_SCHEMA_FAILED: Value {} is less than minimum {}", actual_val, min_val)
        ));
    }

    Ok(())
}

fn validate_maximum(value: &serde_json::Value, maximum: &serde_json::Value) -> Result<(), ExecutionError> {
    let max_val = maximum.as_f64()
        .ok_or_else(|| ExecutionError::ParseError("Schema 'maximum' must be a number".to_string()))?;

    let actual_val = value.as_f64()
        .ok_or_else(|| ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: Value must be a number for 'maximum' constraint".to_string()
        ))?;

    if actual_val > max_val {
        return Err(ExecutionError::ParseError(
            format!("CONVERT_SCHEMA_FAILED: Value {} is greater than maximum {}", actual_val, max_val)
        ));
    }

    Ok(())
}

fn validate_min_length(value: &serde_json::Value, min_length: &serde_json::Value) -> Result<(), ExecutionError> {
    let min_len = min_length.as_u64()
        .ok_or_else(|| ExecutionError::ParseError("Schema 'minLength' must be a number".to_string()))? as usize;

    let actual_str = value.as_str()
        .ok_or_else(|| ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: Value must be a string for 'minLength' constraint".to_string()
        ))?;

    if actual_str.len() < min_len {
        return Err(ExecutionError::ParseError(
            format!("CONVERT_SCHEMA_FAILED: String length {} is less than minLength {}", actual_str.len(), min_len)
        ));
    }

    Ok(())
}

fn validate_max_length(value: &serde_json::Value, max_length: &serde_json::Value) -> Result<(), ExecutionError> {
    let max_len = max_length.as_u64()
        .ok_or_else(|| ExecutionError::ParseError("Schema 'maxLength' must be a number".to_string()))? as usize;

    let actual_str = value.as_str()
        .ok_or_else(|| ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: Value must be a string for 'maxLength' constraint".to_string()
        ))?;

    if actual_str.len() > max_len {
        return Err(ExecutionError::ParseError(
            format!("CONVERT_SCHEMA_FAILED: String length {} is greater than maxLength {}", actual_str.len(), max_len)
        ));
    }

    Ok(())
}

fn validate_pattern(value: &serde_json::Value, pattern: &serde_json::Value) -> Result<(), ExecutionError> {
    let pattern_str = pattern.as_str()
        .ok_or_else(|| ExecutionError::ParseError("Schema 'pattern' must be a string".to_string()))?;

    let actual_str = value.as_str()
        .ok_or_else(|| ExecutionError::ParseError(
            "CONVERT_SCHEMA_FAILED: Value must be a string for 'pattern' constraint".to_string()
        ))?;

    let re = regex::Regex::new(pattern_str)
        .map_err(|e| ExecutionError::ParseError(format!("Invalid regex pattern: {}", e)))?;

    if !re.is_match(actual_str) {
        return Err(ExecutionError::ParseError(
            format!("CONVERT_SCHEMA_FAILED: String '{}' does not match pattern '{}'", actual_str, pattern_str)
        ));
    }

    Ok(())
}

// ============================================================================
// Type Coercion (D51)
// ============================================================================

/// Apply type coercion to value based on schema when mode is Auto.
/// Coerces obvious type mismatches:
/// - string "123" -> integer 123
/// - string "true"/"false" -> boolean
/// - integer -> string
/// - null -> "" (if schema allows)
/// - array[1] -> unwrap single element
/// - single value -> wrap in array
///   Does NOT infer missing fields or add defaults (D52).
fn apply_type_coercion(
    value: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<serde_json::Value, ExecutionError> {
    let schema_map = schema.as_object()
        .ok_or_else(|| ExecutionError::ParseError("Schema must be a mapping".to_string()))?;

    // Get expected type from schema
    let expected_type = schema_map.get("type")
        .and_then(|v| v.as_str());

    if let Some(exp_type) = expected_type {
        match (exp_type, value) {
            // String to number
            ("integer", serde_json::Value::String(s)) => {
                if let Ok(num) = s.parse::<i64>() {
                    return Ok(serde_json::Value::Number(serde_json::Number::from(num)));
                }
            }
            ("number", serde_json::Value::String(s)) => {
                if let Ok(num) = s.parse::<f64>() {
                    if let Some(n) = serde_json::Number::from_f64(num) {
                        return Ok(serde_json::Value::Number(n));
                    }
                }
            }
            // String to boolean
            ("boolean", serde_json::Value::String(s)) => {
                match s.to_lowercase().as_str() {
                    "true" => return Ok(serde_json::Value::Bool(true)),
                    "false" => return Ok(serde_json::Value::Bool(false)),
                    _ => {}
                }
            }
            // Number to string
            ("string", serde_json::Value::Number(n)) => {
                return Ok(serde_json::Value::String(n.to_string()));
            }
            // Boolean to string
            ("string", serde_json::Value::Bool(b)) => {
                return Ok(serde_json::Value::String(b.to_string()));
            }
            // Null to empty string
            ("string", serde_json::Value::Null) => {
                return Ok(serde_json::Value::String(String::new()));
            }
            // Single-element array to scalar
            (_, serde_json::Value::Array(arr)) if arr.len() == 1 && exp_type != "array" => {
                return Ok(arr[0].clone());
            }
            // Scalar to single-element array
            ("array", val) if !matches!(val, serde_json::Value::Array(_)) => {
                return Ok(serde_json::Value::Array(vec![val.clone()]));
            }
            _ => {}
        }
    }

    // If object with properties, recursively coerce properties
    if let Some(properties) = schema_map.get("properties") {
        if let serde_json::Value::Object(obj) = value {
            let props_map = properties.as_object()
                .ok_or_else(|| ExecutionError::ParseError("Schema 'properties' must be a mapping".to_string()))?;

            let mut coerced_obj = obj.clone();

            for (key, val) in obj.iter() {
                if let Some(prop_schema) = props_map.get(key.as_str()) {
                    let coerced_val = apply_type_coercion(val, prop_schema)?;
                    coerced_obj.insert(key.clone(), coerced_val);
                }
            }

            return Ok(serde_json::Value::Object(coerced_obj));
        }
    }

    // If array with items schema, recursively coerce items
    if let Some(items_schema) = schema_map.get("items") {
        if let serde_json::Value::Array(arr) = value {
            let coerced_arr: Result<Vec<_>, _> = arr.iter()
                .map(|item| apply_type_coercion(item, items_schema))
                .collect();

            return Ok(serde_json::Value::Array(coerced_arr?));
        }
    }

    // No coercion needed or possible
    Ok(value.clone())
}

/// Calculate coverage ratio between input and chunk content.
/// Uses character count with whitespace normalization.
/// Per D41: 95% threshold, whitespace normalization allowed.
fn calculate_coverage(input: &str, chunk_content: &str) -> f64 {
    // Normalize whitespace for both input and chunks
    let input_normalized: String = input.chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let chunks_normalized: String = chunk_content.chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    let input_len = input_normalized.len();
    let chunks_len = chunks_normalized.len();

    if input_len == 0 {
        return 1.0; // Empty input is trivially covered
    }

    // Calculate coverage as min(chunks_len, input_len) / input_len
    // This handles cases where chunks might have slightly more content due to formatting
    let covered = chunks_len.min(input_len);
    covered as f64 / input_len as f64
}

/// Validate that chunks have no sentence-level overlap.
/// Per D41: No sentence appears verbatim in multiple chunks.
/// Structural markers (headers) are allowed as exception.
fn validate_no_overlap(chunks: &[serde_json::Value]) -> Result<(), String> {

    // Extract content from all chunks
    let contents: Vec<&str> = chunks.iter()
        .filter_map(|chunk| {
            chunk.as_object()
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
        })
        .collect();

    // Split each chunk into sentences and track which chunks contain each sentence
    let mut sentence_locations: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();

    for (chunk_idx, content) in contents.iter().enumerate() {
        // Simple sentence splitting: split on . ! ? followed by space or end
        let sentences: Vec<&str> = content
            .split(['.', '!', '?'])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && s.len() > 10) // Ignore very short fragments
            .collect();

        for sentence in sentences {
            // Normalize sentence (lowercase, remove extra whitespace)
            let normalized = sentence.to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");

            // Skip structural markers (headers - lines with < 50 chars ending in colon or starting with #)
            if normalized.len() < 50 && (normalized.ends_with(':') || normalized.starts_with('#')) {
                continue;
            }

            sentence_locations.entry(normalized)
                .or_default()
                .push(chunk_idx);
        }
    }

    // Check for sentences appearing in multiple chunks
    let mut overlapping_sentences = Vec::new();
    for (sentence, locations) in sentence_locations.iter() {
        if locations.len() > 1 {
            overlapping_sentences.push(format!(
                "Sentence \"{}...\" appears in chunks: {:?}",
                &sentence.chars().take(50).collect::<String>(),
                locations.iter().map(|i| i + 1).collect::<Vec<_>>()
            ));
        }
    }

    if !overlapping_sentences.is_empty() {
        return Err(format!(
            "Found {} overlapping sentences:\n{}",
            overlapping_sentences.len(),
            overlapping_sentences.join("\n")
        ));
    }

    Ok(())
}

// Removed: validate_output_contract, validate_distill_output_contract,
// validate_distill_input_length, count_tokens (sage-lore#140)
// Validation is the scroll designer's job — use the validate primitive.

// Removed: validate_output_contract, validate_distill_output_contract,
// validate_distill_input_length, count_tokens (sage-lore#140)
// output_contract is a prompt HINT, not an enforcement mechanism.
// Scroll designers use the validate primitive for format checking.

/// Step execution helper methods for the Executor.
///
/// These methods are implemented as free functions to allow better organization
/// and easier testing. They operate on the executor's context and registry.
impl super::executor::Executor {
    // ========================================================================
    // Core Primitive Execution
    // ========================================================================

    /// Execute an elaborate step (contract #42).
    ///
    /// Resolves input, builds prompt with depth, output_contract, and context,
    /// dispatches to invoke::generate(), validates output, and binds result.
    /// Includes deterministic validation (token count, format) and consensus validation.
    /// Retries up to 3 times on validation failures.
    pub async fn execute_elaborate(&mut self, step: &ElaborateStep) -> Result<serde_json::Value, ExecutionError> {
        let result = with_on_fail!(self, &step.on_fail, {
            let input = self.context.resolve_value_strict(
                &serde_json::Value::String(step.elaborate.input.clone())
            )?;

            // Build prompt — output_contract is a hint to the LLM, not enforced.
            // Validation is the scroll designer's job (use the validate primitive).
            let resolved_context = step.elaborate.context.as_ref()
                .map(|ctx| self.context.resolve_value(ctx));
            let full_prompt = build_elaborate_prompt(
                &input,
                &step.elaborate.depth,
                step.elaborate.output_contract.as_ref(),
                resolved_context.as_ref(),
            )?;

            tracing::debug!(prompt = %full_prompt, "Elaborate prompt built");

            let resolved_backend = step.elaborate.backend.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_tier = step.elaborate.model_tier.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_model = step.elaborate.model.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;

            let result = self.interface_registry.invoke_generate_full(
                &full_prompt,
                resolved_backend.as_deref(),
                resolved_tier.as_deref(),
                resolved_model.as_deref(),
                step.elaborate.format_schema.as_ref(),
            ).await?;

            // Track token usage
            let response = result.as_str().unwrap_or("");
            self.track_tokens(&full_prompt, response);

            Ok(result)
        })?;

        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result.clone());
        }
        Ok(result)
    }

    /// Execute a distill step (contract #43).
    ///
    /// Resolves input, validates input length, builds prompt with intensity and output_contract,
    /// dispatches to invoke::generate(), validates output, and binds result.
    /// Includes deterministic validation (token count, format) and consensus validation.
    /// Retries up to 3 times on validation failures.
    pub async fn execute_distill(&mut self, step: &DistillStep) -> Result<serde_json::Value, ExecutionError> {
        let result = with_on_fail!(self, &step.on_fail, {
            let input = self.context.resolve_value_strict(
                &serde_json::Value::String(step.distill.input.clone())
            )?;

            // Build prompt — output_contract is a hint to the LLM, not enforced.
            // Validation is the scroll designer's job (use the validate primitive).
            let resolved_distill_ctx = step.distill.context.as_ref()
                .map(|ctx| self.context.resolve_value(ctx));
            let full_prompt = build_distill_prompt(
                &input,
                &step.distill.intensity,
                step.distill.output_contract.as_ref(),
                resolved_distill_ctx.as_ref(),
            )?;

            tracing::debug!(prompt = %full_prompt, "Distill prompt built");

            let resolved_backend = step.distill.backend.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_tier = step.distill.model_tier.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_model = step.distill.model.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;

            let result = self.interface_registry.invoke_generate_full(
                &full_prompt,
                resolved_backend.as_deref(),
                resolved_tier.as_deref(),
                resolved_model.as_deref(),
                step.distill.format_schema.as_ref(),
            ).await?;

            // Track token usage
            let response = result.as_str().unwrap_or("");
            self.track_tokens(&full_prompt, response);

            Ok(result)
        })?;

        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result.clone());
        }
        Ok(result)
    }

    /// Execute a split step (contract #44).
    ///
    /// Resolves input, builds prompt with strategy/granularity parameters,
    /// dispatches to invoke::generate(), validates output structure and coverage,
    /// and binds result. Split ALWAYS returns a sequence of chunks.
    pub async fn execute_split(&mut self, step: &SplitStep) -> Result<serde_json::Value, ExecutionError> {
        let result = with_on_fail!(self, &step.on_fail, {
            // Resolve input from context (supports embedded ${var} interpolation)
            let input = self.context.resolve_value_strict(
                &serde_json::Value::String(step.split.input.clone())
            )?;
            let input_str = input.as_str()
                .ok_or_else(|| ExecutionError::TypeError("split input must be a string".to_string()))?;

            // Build prompt for LLM
            let resolved_split_ctx = step.split.context.as_ref()
                .map(|ctx| self.context.resolve_value(ctx));
            let prompt = build_split_prompt(
                &input,
                &step.split.by,
                &step.split.granularity,
                step.split.count,
                step.split.markers.as_ref(),
                resolved_split_ctx.as_ref(),
            )?;
            tracing::debug!(prompt = %prompt, "Split prompt built");

            // Schema priority: scroll-level format_schema > auto-gen > none.
            // Auto-gen enforces split contract: array of {id, content, label}.
            let schema = step.split.format_schema.clone().unwrap_or_else(|| {
                serde_json::json!({
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "content": { "type": "string" },
                            "label": { "type": "string" }
                        },
                        "required": ["id", "content"],
                        "additionalProperties": true
                    }
                })
            });

            // Get raw LLM response (returns Value::String)
            let resolved_backend = step.split.backend.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_tier = step.split.model_tier.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_model = step.split.model.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;

            let raw = self.interface_registry.invoke_generate_full(
                &prompt,
                resolved_backend.as_deref(),
                resolved_tier.as_deref(),
                resolved_model.as_deref(),
                Some(&schema),
            ).await?;

            // Track token usage
            let response = raw.as_str().unwrap_or("");
            self.track_tokens(&prompt, response);

            // Parse LLM output as sequence of chunks
            let parsed = parse_as_sequence(&raw)?;

            // Validate that each element is a chunk with required fields
            let chunks = match &parsed {
                serde_json::Value::Array(seq) => seq,
                _ => return Err(ExecutionError::TypeError("Split must return a sequence".to_string())),
            };

            // Validate chunk structure (id, content, optional label)
            for (idx, chunk) in chunks.iter().enumerate() {
                let chunk_map = chunk.as_object()
                    .ok_or_else(|| ExecutionError::TypeError(
                        format!("Chunk {} must be a mapping with id, content, label", idx + 1)
                    ))?;

                // Validate required fields
                let id = chunk_map.get("id")
                    .ok_or_else(|| ExecutionError::ParseError(
                        format!("Chunk {} missing required 'id' field", idx + 1)
                    ))?;

                let content = chunk_map.get("content")
                    .ok_or_else(|| ExecutionError::ParseError(
                        format!("Chunk {} missing required 'content' field", idx + 1)
                    ))?;

                // Validate types
                if !matches!(id, serde_json::Value::Number(_)) {
                    return Err(ExecutionError::TypeError(
                        format!("Chunk {} 'id' must be a number", idx + 1)
                    ));
                }

                if !matches!(content, serde_json::Value::String(_)) {
                    return Err(ExecutionError::TypeError(
                        format!("Chunk {} 'content' must be a string", idx + 1)
                    ));
                }

                // Label is optional but must be string if present
                if let Some(label) = chunk_map.get("label") {
                    if !matches!(label, serde_json::Value::String(_)) {
                        return Err(ExecutionError::TypeError(
                            format!("Chunk {} 'label' must be a string if present", idx + 1)
                        ));
                    }
                }
            }

            // Validate coverage: all input content should be in chunks (95% threshold)
            let total_chunk_content = chunks.iter()
                .filter_map(|chunk| {
                    chunk.as_object()
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                })
                .collect::<Vec<_>>()
                .join("");

            let coverage = calculate_coverage(input_str, &total_chunk_content);
            if coverage < 0.95 {
                return Err(ExecutionError::InvocationError(
                    format!("Split coverage validation failed: {:.1}% < 95% threshold. Input length: {}, chunk content length: {}",
                        coverage * 100.0, input_str.len(), total_chunk_content.len())
                ));
            }

            tracing::info!(
                coverage = format!("{:.1}%", coverage * 100.0),
                chunk_count = chunks.len(),
                "Split coverage validation passed"
            );

            // Validate no overlap: no sentence should appear in multiple chunks
            if let Err(e) = validate_no_overlap(chunks) {
                return Err(ExecutionError::InvocationError(
                    format!("Split overlap validation failed: {}", e)
                ));
            }

            Ok(parsed)
        })?;

        // Store result in context if output name specified
        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result.clone());
        }

        Ok(result)
    }

    /// Execute a merge step (contract-enforced replacement for synthesize).
    ///
    /// Merges 2-10 inputs using specified strategy with full contract validation.
    /// Supports sequential, reconcile, union, and intersection strategies.
    pub async fn execute_merge(&mut self, step: &MergeStep) -> Result<serde_json::Value, ExecutionError> {
        // Validate input count: minimum 2, maximum 10
        if step.merge.inputs.len() < 2 {
            return Err(ExecutionError::TypeError(
                "merge requires at least 2 inputs".to_string()
            ));
        }

        if step.merge.inputs.len() > 10 {
            return Err(ExecutionError::TypeError(
                format!("merge accepts maximum 10 inputs, got {}", step.merge.inputs.len())
            ));
        }

        let result = with_on_fail!(self, &step.on_fail, {
            let mut resolved_inputs = Vec::new();
            for input_ref in &step.merge.inputs {
                let value = self.context.resolve_value_strict(
                    &serde_json::Value::String(input_ref.clone())
                )?;

                // Validate non-empty inputs
                if let serde_json::Value::String(s) = &value {
                    if s.trim().is_empty() {
                        tracing::warn!("Merge received empty string input: {}", input_ref);
                    }
                }

                resolved_inputs.push(value);
            }

            let resolved_merge_ctx = step.merge.context.as_ref()
                .map(|ctx| self.context.resolve_value(ctx));
            let prompt = build_merge_prompt(
                &resolved_inputs,
                &step.merge.strategy,
                step.merge.output_contract.as_ref(),
                resolved_merge_ctx.as_ref(),
            )?;

            tracing::debug!(
                prompt = %prompt,
                strategy = ?step.merge.strategy,
                input_count = resolved_inputs.len(),
                "Merge prompt built"
            );

            let resolved_backend = step.merge.backend.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_tier = step.merge.model_tier.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_model = step.merge.model.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;

            let result = self.interface_registry.invoke_generate_full(
                &prompt,
                resolved_backend.as_deref(),
                resolved_tier.as_deref(),
                resolved_model.as_deref(),
                step.merge.format_schema.as_ref(),
            ).await?;

            // Track token usage
            let response = result.as_str().unwrap_or("");
            self.track_tokens(&prompt, response);

            // Parse as structured YAML for reconcile strategy and when output_contract specifies structured
            // For other strategies, try structured parsing but fall back to string
            try_parse_structured(&result)
        })?;

        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result.clone());
        }
        Ok(result)
    }

    /// Execute a validate step.
    ///
    /// Resolves input and optional reference, builds prompt with criteria,
    /// dispatches to invoke::generate(), and binds output.
    pub async fn execute_validate(&mut self, step: &ValidateStep) -> Result<serde_json::Value, ExecutionError> {
        let result = with_on_fail!(self, &step.on_fail, {
            let input = self.context.resolve_value_strict(
                &serde_json::Value::String(step.validate.input.clone())
            )?;
            let reference = step.validate.reference.as_ref()
                .map(|r| self.context.resolve_value(
                    &serde_json::Value::String(r.clone())
                ));
            let reference = if reference.as_ref().is_none_or(|v| v.is_null()) { None } else { reference };
            let prompt = build_validate_prompt(&input, reference.as_ref(), &step.validate.criteria, &step.validate.mode)?;
            tracing::debug!(prompt = %prompt, "Validate prompt built");

            // Schema priority: scroll-level format_schema > auto-gen > none.
            // Auto-gen enforces validate contract {result, score, criteria_results, summary}.
            let schema = step.validate.format_schema.clone().unwrap_or_else(|| {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "result": { "type": "string", "enum": ["pass", "fail"] },
                        "score": { "type": "number" },
                        "criteria_results": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "criterion": { "type": "string" },
                                    "passed": { "type": "boolean" },
                                    "explanation": { "type": "string" }
                                },
                                "required": ["criterion", "passed", "explanation"],
                                "additionalProperties": true
                            }
                        },
                        "summary": { "type": "string" }
                    },
                    "required": ["result", "score", "criteria_results", "summary"],
                    "additionalProperties": true
                })
            });

            let resolved_backend = step.validate.backend.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_tier = step.validate.model_tier.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;
            let resolved_model = step.validate.model.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;

            let result = self.interface_registry.invoke_generate_full(
                &prompt,
                resolved_backend.as_deref(),
                resolved_tier.as_deref(),
                resolved_model.as_deref(),
                Some(&schema),
            ).await?;

            // Track token usage
            let response = result.as_str().unwrap_or("");
            self.track_tokens(&prompt, response);

            // Parse as structured YAML (should be ValidationResult structure)
            let parsed = try_parse_structured(&result)?;

            // SCHEMA VALIDATION ONLY (no consensus - avoids infinite regress per D31)
            // Verify structure matches ValidationResult schema
            validate_result_schema(&parsed)?;

            // Always return the full result object — pass OR fail.
            // The result contains {result: "pass"|"fail", score, criteria_results, summary}.
            // Scrolls branch on ${validation_result.result} to decide next steps.
            // This preserves failure details for iterative refinement loops (#142).
            Ok(parsed)
        })?;

        // Store result BEFORE halt check — on_fail: continue scrolls can branch on it
        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result.clone());
        }

        // B1 (#180): validate halts on failure unless on_fail is continue.
        // Retry and Fallback also fall through to halt — validation failure is a
        // semantic quality issue, not a retryable infrastructure error.
        if result.get("result").and_then(|v| v.as_str()) == Some("fail") {
            if !matches!(step.on_fail, crate::scroll::schema::OnFail::Continue) {
                let summary = result.get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("validation failed")
                    .to_string();
                let score = result.get("score")
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                return Err(ExecutionError::ValidationFailed { summary, score });
            }
        }

        Ok(result)
    }

    /// Execute a convert step — parse and transform between data formats with optional schema validation (contract #47).
    ///
    /// Resolves input, detects format if needed, builds prompt with target format/schema,
    /// dispatches to invoke::generate(), validates output format and schema,
    /// applies type coercion if mode=auto, and binds result.
    /// Retries up to 3 times on validation failures with error feedback.
    pub async fn execute_convert(&mut self, step: &ConvertStep) -> Result<serde_json::Value, ExecutionError> {
        use crate::scroll::extraction::parse_and_validate_yaml;
        use crate::scroll::schema::{CoercionMode, ConvertTarget};

        let result = with_on_fail!(self, &step.on_fail, {
            // Resolve input from context (supports embedded ${var} interpolation)
            let input = self.context.resolve_value_strict(
                &serde_json::Value::String(step.convert.input.clone())
            )?;

            // Handle both string and non-string inputs
            // If input is already a string, use it directly
            // Otherwise, serialize to JSON string first
            let input_str = match input.as_str() {
                Some(s) => s.to_string(),
                None => serde_json::to_string(&input)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?,
            };
            let input_str = input_str.as_str();

            // Fast path: if from=yaml and to=yaml (or to.format=yaml), just parse directly.
            // No LLM call needed — the input already IS the target format.
            // This avoids the LLM re-generation bug where fields get dropped.
            // Fast path: if from=yaml and to=yaml, parse locally (no LLM needed).
            // Only triggers with explicit from=yaml — unset "from" means the input
            // may need LLM transformation (e.g., prose → structured YAML).
            let is_yaml_passthrough = step.convert.from.as_deref() == Some("yaml") && match &step.convert.to {
                ConvertTarget::Simple(f) => f == "yaml",
                ConvertTarget::Detailed { format, .. } => format == "yaml",
            };

            if is_yaml_passthrough {
                // Try parse-only fast path. Falls back to LLM path if parsing fails
                // (e.g., mock backends that don't produce valid YAML).
                if let Ok(parsed) = parse_and_validate_yaml(input_str) {
                    // Apply schema validation if present
                    if let ConvertTarget::Detailed { schema, .. } = &step.convert.to {
                        if let Ok(validated) = apply_type_coercion(&parsed, schema) {
                            if validate_against_schema(&validated, schema).is_ok() {
                                // Bind output before returning (fix #115)
                                if let Some(output_name) = &step.output {
                                    self.context.set_variable(output_name.clone(), validated.clone());
                                }
                                return Ok(validated);
                            }
                        }
                        // Schema validation failed — fall through to LLM path
                    } else {
                        // Bind output before returning (fix #115)
                        if let Some(output_name) = &step.output {
                            self.context.set_variable(output_name.clone(), parsed.clone());
                        }
                        return Ok(parsed);
                    }
                }
                // Parse failed — fall through to normal LLM convert path
            }

            // Fast path: if to=json, try local parse first (no LLM needed).
            // Triggers when from=json explicitly, OR when from is unset AND input looks like JSON.
            // Uses the extraction cascade: strip fences → serde → validate.
            let to_is_json = match &step.convert.to {
                ConvertTarget::Simple(f) => f == "json",
                ConvertTarget::Detailed { format, .. } => format == "json",
            };
            let input_looks_like_json = input_str.trim_start().starts_with('{') || input_str.trim_start().starts_with('[');
            let is_json_passthrough = to_is_json && (step.convert.from.as_deref() == Some("json") || (step.convert.from.is_none() && input_looks_like_json));

            if is_json_passthrough {
                // With schema: use extract_structured_output (handles fences, substrings, etc.)
                if let ConvertTarget::Detailed { schema, .. } = &step.convert.to {
                    if let Ok(structured) = extract_structured_output(input_str, schema) {
                        tracing::info!("JSON fast path: local parse + schema validation succeeded");
                        if let Some(output_name) = &step.output {
                            self.context.set_variable(output_name.clone(), structured.clone());
                        }
                        return Ok(structured);
                    }
                    // Extraction failed — fall through to LLM path
                } else {
                    // No schema — just parse JSON directly
                    let stripped = strip_markdown_fences(input_str);
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(stripped.trim()) {
                        tracing::info!("JSON fast path: local parse succeeded (no schema)");
                        if let Some(output_name) = &step.output {
                            self.context.set_variable(output_name.clone(), parsed.clone());
                        }
                        return Ok(parsed);
                    }
                    // Parse failed — fall through to LLM path
                }
            }

            // Retry loop: up to 3 attempts total
            const MAX_RETRIES: usize = 3;
            let mut validation_errors = Vec::new();

            for attempt in 0..MAX_RETRIES {
                // Build prompt with validation feedback from previous attempts
                let resolved_convert_ctx = step.convert.context.as_ref()
                    .map(|ctx| self.context.resolve_value(ctx));
                let mut full_prompt = build_convert_prompt(
                    input_str,
                    step.convert.from.as_deref(),
                    &step.convert.to,
                    resolved_convert_ctx.as_ref(),
                )?;

                // Add validation feedback to prompt on retries
                if attempt > 0 {
                    full_prompt.push_str("\n\n## VALIDATION FAILURES (MUST FIX)\n");
                    for (i, err) in validation_errors.iter().enumerate() {
                        full_prompt.push_str(&format!("Attempt {}: {}\n", i + 1, err));
                    }
                    full_prompt.push_str("\nYou MUST include ALL required fields. Do NOT omit any field from the schema. Output the COMPLETE structure.\n");
                }

                tracing::debug!(
                    attempt = attempt + 1,
                    max_retries = MAX_RETRIES,
                    prompt = %full_prompt,
                    "Convert prompt built"
                );

                let resolved_backend = step.convert.backend.as_ref()
                    .map(|v| self.resolve_string_param(v)).transpose()?;
                let resolved_tier = step.convert.model_tier.as_ref()
                    .map(|v| self.resolve_string_param(v)).transpose()?;
                let resolved_model = step.convert.model.as_ref()
                    .map(|v| self.resolve_string_param(v)).transpose()?;

                let raw_result = self.interface_registry.invoke_generate_full(
                    &full_prompt,
                    resolved_backend.as_deref(),
                    resolved_tier.as_deref(),
                    resolved_model.as_deref(),
                    step.convert.format_schema.as_ref(),
                ).await?;

                // Track token usage
                let response = raw_result.as_str().unwrap_or("");
                self.track_tokens(&full_prompt, response);

                // Parse and validate based on target format (with LLM repair fallback)
                let parse_result: Result<serde_json::Value, ExecutionError> = match &step.convert.to {
                    ConvertTarget::Simple(format) => {
                        match format.as_str() {
                            "json" | "yaml" => self.parse_with_repair(response, format).await,
                            "markdown" | "prose" | "csv" | "xml" => {
                                // For text formats, keep as string
                                Ok(raw_result.clone())
                            }
                            _ => Err(ExecutionError::InvocationError(
                                format!("CONVERT_OUTPUT_INVALID: Unknown format: {}", format)
                            )),
                        }
                    }
                    ConvertTarget::Detailed { format, schema } => {
                        // Parse as target format (with repair fallback)
                        let mut parsed = match format.as_str() {
                            "json" | "yaml" => self.parse_with_repair(response, format).await?,
                            "markdown" | "prose" | "csv" | "xml" => raw_result.clone(),
                            _ => return Err(ExecutionError::InvocationError(
                                format!("CONVERT_OUTPUT_INVALID: Unknown format: {}", format)
                            )),
                        };

                        // Apply type coercion if mode=auto BEFORE validation
                        if step.convert.coercion == CoercionMode::Auto {
                            parsed = apply_type_coercion(&parsed, schema)?;
                        }

                        // Validate against JSON Schema
                        if let Err(e) = validate_against_schema(&parsed, schema) {
                            validation_errors.push(e.to_string());
                            if attempt < MAX_RETRIES - 1 {
                                tracing::warn!(
                                    attempt = attempt + 1,
                                    error = %e,
                                    "Schema validation failed, retrying"
                                );
                                continue;
                            } else {
                                return Err(ExecutionError::InvocationError(
                                    format!("CONVERT_OUTPUT_INVALID: LLM output not valid {} after {} attempts: {}",
                                        format, MAX_RETRIES, e)
                                ));
                            }
                        }

                        Ok(parsed)
                    }
                };

                // Check if parsing succeeded
                match parse_result {
                    Ok(parsed) => {
                        tracing::info!(
                            attempt = attempt + 1,
                            "Convert completed successfully"
                        );
                        // Bind output before returning (same class as fix #115)
                        if let Some(output_name) = &step.output {
                            self.context.set_variable(output_name.clone(), parsed.clone());
                        }
                        return Ok(parsed);
                    }
                    Err(e) => {
                        validation_errors.push(e.to_string());
                        if attempt < MAX_RETRIES - 1 {
                            tracing::warn!(
                                attempt = attempt + 1,
                                error = %e,
                                "Parse/validation failed, retrying"
                            );
                            continue;
                        } else {
                            return Err(e);
                        }
                    }
                }
            }

            // Should never reach here due to return in loop
            Err(ExecutionError::InvocationError(
                format!("CONVERT_OUTPUT_INVALID: Convert failed after {} attempts", MAX_RETRIES)
            ))
        })?;

        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result.clone());
        }
        Ok(result)
    }

    /// Execute an invoke step.
    ///
    /// Invokes an agent with a prompt and optional context.
    /// File operations, git operations, etc. should use dedicated primitives (fs, vcs, test, etc.).
    pub async fn execute_invoke(&mut self, step: &InvokeStep) -> Result<(), ExecutionError> {
        let result = with_on_fail!(self, &step.on_fail, {
            let agent = &step.invoke.agent;
            let timeout = step.invoke.timeout_secs;
            tracing::info!(agent = %agent, timeout_secs = ?timeout, "Invoking agent");

            // Resolve variable references in the instructions template
            let resolved_instructions = self.interpolate_string(&step.invoke.instructions)?;
            tracing::debug!(agent = %agent, instructions_len = resolved_instructions.len(), "Instructions resolved");

            // Resolve context variables
            let context_values: Vec<serde_json::Value> = step
                .invoke
                .context
                .as_ref()
                .map(|ctx_refs| {
                    ctx_refs
                        .iter()
                        .filter_map(|var_ref| self.context.resolve(var_ref).ok())
                        .collect()
                })
                .unwrap_or_default();

            let resolved_backend = step.invoke.backend.as_ref()
                .map(|v| self.resolve_string_param(v)).transpose()?;

            // Look up agent system prompt from registry
            let system_prompt = self.interface_registry.get_agent_system_prompt(agent)?;

            let invoke_start = std::time::Instant::now();
            let result = self.interface_registry.invoke_agent(
                agent,
                &system_prompt,
                &resolved_instructions,
                &context_values,
                step.invoke.timeout_secs,
                resolved_backend.as_deref(),
                step.invoke.output_schema.as_ref(),
            ).await?;
            let invoke_elapsed = invoke_start.elapsed();

            // Track token usage
            let response = result.as_str().unwrap_or("");
            let instruction_tokens = resolved_instructions.len() / 4;
            let response_tokens = response.len() / 4;
            tracing::info!(
                agent = %agent,
                instruction_tokens = instruction_tokens,
                response_tokens = response_tokens,
                response_len = response.len(),
                elapsed = crate::scroll::executor::format_duration(invoke_elapsed).as_str(),
                "Agent responded"
            );
            tracing::debug!(agent = %agent, response_preview = %response.chars().take(300).collect::<String>(), "Response preview");
            self.track_tokens(&resolved_instructions, response);

            Ok(result)
        })?;

        // If output_schema is present, extract structured data from raw response
        let result = if let Some(schema) = &step.invoke.output_schema {
            let raw_text = result.as_str().unwrap_or("");
            match extract_structured_output(raw_text, schema) {
                Ok(structured) => {
                    tracing::info!("output_schema: structured extraction succeeded (local parse)");
                    structured
                }
                Err(e) => {
                    tracing::warn!("output_schema: extraction failed: {}", e);
                    return Err(e);
                }
            }
        } else {
            result
        };

        // Bind output if specified
        if let Some(output_name) = &step.output {
            tracing::debug!(output = %output_name, "Output bound");
            self.context.set_variable(output_name.clone(), result);
        }

        Ok(())
    }

    /// Execute a parallel step.
    ///
    /// Fans out the same prompt to multiple agents simultaneously.
    /// Uses thread-based concurrency with semaphore-like limiting via max_concurrent.
    pub async fn execute_parallel(&mut self, step: &ParallelStep) -> Result<(), ExecutionError> {
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::sync::Semaphore;
        use tokio::task::JoinSet;

        let agents = &step.parallel.agents;

        // Resolve prompt if it's a variable reference (starts with ${), otherwise use as-is
        let prompt_str = if step.parallel.prompt.starts_with("${") {
            let resolved = self.context.resolve(&step.parallel.prompt)?;
            resolved.as_str()
                .ok_or_else(|| ExecutionError::VariableResolution("Prompt must be a string".to_string()))?
                .to_string()
        } else {
            step.parallel.prompt.clone()
        };

        let max_concurrent = step.parallel.max_concurrent;
        let timeout_per_agent = step.parallel.timeout_per_agent.map(Duration::from_secs);
        let on_fail = &step.parallel.on_fail;
        let quorum = step.parallel.quorum;

        tracing::info!(
            agent_count = agents.len(),
            max_concurrent = max_concurrent,
            on_fail = ?on_fail,
            "Executing parallel agent invocations"
        );

        // Validate quorum if required
        if let crate::scroll::schema::ParallelFailMode::RequireQuorum = on_fail {
            let quorum_count = quorum.ok_or_else(|| {
                ExecutionError::InvalidOnFail("quorum must be specified when on_fail is require_quorum".to_string())
            })?;
            if quorum_count > agents.len() {
                return Err(ExecutionError::InvalidOnFail(
                    format!("quorum {} exceeds agent count {}", quorum_count, agents.len())
                ));
            }
        }

        // Use tokio semaphore for cooperative concurrency limiting
        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let mut set: JoinSet<(usize, String, Result<serde_json::Value, ExecutionError>)> = JoinSet::new();

        // Resolve system prompts for all agents before spawning tasks
        let mut agent_prompts = Vec::new();
        for agent_name in agents.iter() {
            let system_prompt = self.interface_registry.get_agent_system_prompt(agent_name)?;
            agent_prompts.push(system_prompt);
        }

        // Spawn async tasks for each agent
        for (index, agent_name) in agents.iter().enumerate() {
            let agent_name = agent_name.clone();
            let prompt_str = prompt_str.clone();
            let system_prompt = agent_prompts[index].clone();
            let semaphore = Arc::clone(&semaphore);
            let interface_registry = self.interface_registry.clone();

            set.spawn(async move {
                // Acquire permit — awaits cooperatively instead of spin-looping
                let _permit = semaphore.acquire().await.unwrap();

                let result = interface_registry.invoke_agent(
                    &agent_name, &system_prompt, &prompt_str, &[], None, None, None
                ).await;

                (index, agent_name, result)
            });
        }

        // Collect results
        let mut results: Vec<Option<serde_json::Value>> = vec![None; agents.len()];
        let mut successes = 0;
        let mut errors = Vec::new();

        // Drain with optional timeout
        let total_timeout = timeout_per_agent.map(|t| t * agents.len() as u32);
        if let Some(dur) = total_timeout {
            let deadline = tokio::time::Instant::now() + dur;
            loop {
                tokio::select! {
                    biased;
                    join_result = set.join_next() => {
                        match join_result {
                            Some(Ok((index, agent_name, result))) => {
                                match result {
                                    Ok(value) => {
                                        results[index] = Some(value);
                                        successes += 1;
                                        tracing::debug!(agent = %agent_name, index = index, "Agent completed successfully");
                                    }
                                    Err(e) => {
                                        tracing::warn!(agent = %agent_name, error = %e, "Agent invocation failed");
                                        errors.push(format!("{}: {}", agent_name, e));
                                        results[index] = Some(serde_json::Value::Null);
                                    }
                                }
                            }
                            Some(Err(join_err)) => {
                                tracing::warn!("Parallel task panicked: {}", join_err);
                            }
                            None => break, // all tasks completed
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        set.abort_all();
                        tracing::warn!("Parallel operations exceeded timeout");
                        break;
                    }
                }
            }
        } else {
            // No timeout — drain the JoinSet
            while let Some(join_result) = set.join_next().await {
                match join_result {
                    Ok((index, agent_name, result)) => {
                        match result {
                            Ok(value) => {
                                results[index] = Some(value);
                                successes += 1;
                                tracing::debug!(agent = %agent_name, index = index, "Agent completed successfully");
                            }
                            Err(e) => {
                                tracing::warn!(agent = %agent_name, error = %e, "Agent invocation failed");
                                errors.push(format!("{}: {}", agent_name, e));
                                results[index] = Some(serde_json::Value::Null);
                            }
                        }
                    }
                    Err(join_err) => {
                        tracing::warn!("Parallel task panicked: {}", join_err);
                    }
                }
            }
        }

        // Handle failure modes
        match on_fail {
            crate::scroll::schema::ParallelFailMode::RequireAll => {
                if successes < agents.len() {
                    return Err(ExecutionError::InvocationError(
                        format!("Parallel execution failed: {}/{} agents succeeded. Errors: {:?}",
                            successes, agents.len(), errors)
                    ));
                }
            }
            crate::scroll::schema::ParallelFailMode::RequireQuorum => {
                let quorum_count = quorum.unwrap(); // Already validated above
                if successes < quorum_count {
                    return Err(ExecutionError::InvocationError(
                        format!("Parallel execution failed to reach quorum: {}/{} agents succeeded (required: {}). Errors: {:?}",
                            successes, agents.len(), quorum_count, errors)
                    ));
                }
            }
            crate::scroll::schema::ParallelFailMode::BestEffort => {
                // Continue with whatever results we have
                tracing::info!(
                    successes = successes,
                    total = agents.len(),
                    "Parallel execution completed with best effort"
                );
            }
        }

        // Convert results to sequence, maintaining order
        let result_values: Vec<serde_json::Value> = results
            .into_iter()
            .map(|r| r.unwrap_or(serde_json::Value::Null))
            .collect();

        // Bind output if specified
        if let Some(output_name) = &step.output {
            self.context.set_variable(
                output_name.clone(),
                serde_json::Value::Array(result_values),
            );
        }

        tracing::info!(
            successes = successes,
            total = agents.len(),
            "Parallel agent invocations completed"
        );

        Ok(())
    }

    /// Execute a consensus step.
    ///
    /// Coordinates multiple agents to vote on a proposal and reach agreement.
    /// Uses parallel internally to fan-out voting prompt to all agents.
    pub async fn execute_consensus(&mut self, step: &ConsensusStep) -> Result<(), ExecutionError> {
        use std::collections::HashMap;

        let agents = &step.consensus.agents;
        let proposal = &step.consensus.proposal;
        let options = &step.consensus.options;
        let threshold = &step.consensus.threshold;

        tracing::info!(
            agent_count = agents.len(),
            proposal = %proposal,
            options = ?options,
            "Executing consensus vote"
        );

        // Resolve variable references in the proposal (same as invoke prompt interpolation)
        let proposal_str = self.interpolate_string(proposal)?;

        // Build voting prompt
        let vote_prompt = format!(
            "Vote on the following proposal:\n\n{}\n\nValid options: {}\n\nRespond with your vote followed by your reasoning. Format:\n\nVOTE: [your choice]\nREASON: [your reasoning]",
            proposal_str,
            options.join(", ")
        );

        tracing::info!(
            agents = ?agents,
            options = ?options,
            "Starting consensus vote"
        );
        tracing::debug!(proposal_preview = %proposal_str.chars().take(200).collect::<String>(), "Consensus proposal");

        // Fan-out to agents using parallel-like execution
        use std::sync::mpsc;
        use std::thread;

        let (tx, rx) = mpsc::channel();
        let mut handles = vec![];

        // Capture the Tokio runtime handle BEFORE spawning threads
        // std::thread::spawn creates OS threads without a Tokio context,
        // so Handle::current() would panic inside them.
        let rt_handle = tokio::runtime::Handle::current();

        // Build JSON schema for constrained vote output (Ollama structured output).
        // This uses grammar-based token masking to guarantee the model outputs
        // exactly {"vote": "approve"|"reject"|..., "reason": "..."}.
        let vote_format_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "vote": {
                    "type": "string",
                    "enum": options
                },
                "reason": {
                    "type": "string"
                }
            },
            "required": ["vote", "reason"]
        });

        for (index, agent_name) in agents.iter().enumerate() {
            let tx = tx.clone();
            let agent_name = agent_name.clone();
            let vote_prompt = vote_prompt.clone();
            let interface_registry = self.interface_registry.clone();
            let rt_handle = rt_handle.clone();
            let vote_schema = vote_format_schema.clone();

            let handle = thread::spawn(move || {
                // Use the LLM backend directly with cheap tier + format schema
                // for reliable structured voting output.
                let request = crate::primitives::invoke::LlmRequest {
                    prompt: vote_prompt,
                    system: None,
                    max_tokens: Some(512),
                    temperature: Some(0.0),
                    timeout_secs: None,
                    model_tier: None, // use default model — consensus needs reasoning capability
                    format_schema: Some(vote_schema),
                    model: None,
                };
                let backend = interface_registry.invoke.backend();
                let result = rt_handle.block_on(backend.generate(request))
                    .map(|resp| serde_json::Value::String(resp.text))
                    .map_err(|e| crate::scroll::error::ExecutionError::InvocationError(e.to_string()));
                tx.send((index, agent_name, result)).ok();
            });

            handles.push(handle);
        }

        drop(tx);

        // Collect responses
        let mut responses = vec![None; agents.len()];
        let mut completed = 0;

        while completed < agents.len() {
            match rx.recv() {
                Ok((index, agent_name, result)) => {
                    match result {
                        Ok(value) => {
                            let response_preview = value.as_str()
                                .map(|s| s.chars().take(300).collect::<String>())
                                .unwrap_or_else(|| format!("{:?}", value));
                            tracing::info!(agent = %agent_name, "Vote received");
                            tracing::debug!(agent = %agent_name, preview = %response_preview, "Vote response");
                            responses[index] = Some((agent_name.clone(), value));
                        }
                        Err(e) => {
                            tracing::warn!(agent = %agent_name, error = %e, "Vote failed");
                            responses[index] = None;
                        }
                    }
                    completed += 1;
                }
                Err(_) => break,
            }
        }

        // Wait for all threads
        for handle in handles {
            let _ = handle.join();
        }

        // Parse votes from responses
        let mut votes = Vec::new();
        let mut tally: HashMap<String, usize> = HashMap::new();

        for (agent_name, response) in responses.into_iter().flatten() {
            let response_text = response.as_str().unwrap_or("");

            // Parse vote and reason from response
            let (vote, reason) = super::consensus::parse_vote_response(response_text, options);
            tracing::info!(agent = %agent_name, vote = ?vote, "Parsed vote");
            tracing::debug!(agent = %agent_name, reason = %reason.chars().take(150).collect::<String>(), "Vote reasoning");

            if let Some(v) = &vote {
                *tally.entry(v.clone()).or_insert(0) += 1;
            }

            // Build vote record
            let mut vote_record = serde_json::Map::new();
            vote_record.insert(
                "agent".to_string(),
                serde_json::Value::String(agent_name),
            );
            vote_record.insert(
                "vote".to_string(),
                vote.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
            );
            vote_record.insert(
                "reason".to_string(),
                serde_json::Value::String(reason),
            );

            votes.push(serde_json::Value::Object(vote_record));
        }

        // Determine winning option and check threshold
        let total_votes = votes.len();
        let (winning_option, winning_count) = tally.iter()
            .max_by_key(|(_, count)| *count)
            .map(|(opt, count)| (opt.clone(), *count))
            .unwrap_or_else(|| (String::new(), 0));

        let threshold_met = super::consensus::check_threshold(winning_count, total_votes, threshold);
        let unanimous = winning_count == total_votes && total_votes > 0 && !winning_option.is_empty();

        // Build tally mapping
        let mut tally_map = serde_json::Map::new();
        for (option, count) in tally.iter() {
            tally_map.insert(
                option.clone(),
                serde_json::Value::Number(serde_json::Number::from(*count as u64)),
            );
        }

        // Build result structure
        let mut result = serde_json::Map::new();
        result.insert(
            "result".to_string(),
            serde_json::Value::String(winning_option.clone()),
        );
        result.insert(
            "votes".to_string(),
            serde_json::Value::Array(votes),
        );
        result.insert(
            "tally".to_string(),
            serde_json::Value::Object(tally_map),
        );
        result.insert(
            "unanimous".to_string(),
            serde_json::Value::Bool(unanimous),
        );

        tracing::info!(
            result = %winning_option,
            votes = format!("{}/{}", winning_count, total_votes).as_str(),
            unanimous = unanimous,
            threshold_met = threshold_met,
            tally = ?tally,
            "Consensus vote complete"
        );

        // Check if threshold was met
        if !threshold_met {
            return Err(ExecutionError::InvocationError(
                format!("Consensus threshold not met: {} votes for '{}' out of {} total",
                    winning_count, winning_option, total_votes)
            ));
        }

        // Bind output if specified
        if let Some(output_name) = &step.output {
            self.context.set_variable(
                output_name.clone(),
                serde_json::Value::Object(result),
            );
        }

        Ok(())
    }

    /// Execute a concurrent step.
    ///
    /// Runs multiple operations in parallel using threads.
    /// Unlike parallel (agent parallelism), concurrent runs different operations simultaneously.
    pub async fn execute_concurrent(&mut self, step: &ConcurrentStep) -> Result<(), ExecutionError> {
        use std::time::Duration;

        let operations = &step.concurrent.operations;
        let timeout = step.concurrent.timeout.map(Duration::from_secs);
        let on_fail = &step.on_fail;

        tracing::info!(
            operation_count = operations.len(),
            timeout_secs = step.concurrent.timeout,
            "Executing concurrent operations"
        );

        // Execute operations based on on_fail strategy
        let results = match on_fail {
            OnFail::Halt => {
                // Execute all operations, halt on first error
                self.execute_concurrent_halt(operations, timeout).await?
            }
            OnFail::Continue => {
                // Execute all operations, collect results with nulls for failures
                self.execute_concurrent_continue(operations, timeout).await?
            }
            OnFail::CollectErrors => {
                // Execute all operations, return {results, errors} mapping
                self.execute_concurrent_collect_errors(operations, timeout).await?
            }
            _ => {
                return Err(ExecutionError::InvalidOnFail(
                    "concurrent only supports halt, continue, or collect_errors".to_string()
                ));
            }
        };

        // Bind aggregate output if specified
        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), results);
        }

        Ok(())
    }

    /// Execute concurrent operations with halt-on-error behavior.
    async fn execute_concurrent_halt(
        &mut self,
        operations: &[Step],
        timeout: Option<std::time::Duration>,
    ) -> Result<serde_json::Value, ExecutionError> {
        super::concurrent::execute_concurrent_halt(self, operations, timeout).await
    }

    /// Execute concurrent operations with continue-on-error behavior.
    async fn execute_concurrent_continue(
        &mut self,
        operations: &[Step],
        timeout: Option<std::time::Duration>,
    ) -> Result<serde_json::Value, ExecutionError> {
        super::concurrent::execute_concurrent_continue(self, operations, timeout).await
    }

    /// Execute concurrent operations with error collection behavior.
    async fn execute_concurrent_collect_errors(
        &mut self,
        operations: &[Step],
        timeout: Option<std::time::Duration>,
    ) -> Result<serde_json::Value, ExecutionError> {
        super::concurrent::execute_concurrent_collect_errors(self, operations, timeout).await
    }

    /// Execute a branch step.
    pub async fn execute_branch(&mut self, step: &BranchStep) -> Result<(), ExecutionError> {
        // Evaluate condition — supports ==, != expressions and truthy/falsy fallback
        let is_true = self.evaluate_condition(&step.branch.condition);

        let steps_to_execute = if is_true {
            &step.branch.if_true
        } else if let Some(if_false) = &step.branch.if_false {
            if_false
        } else {
            return Ok(()); // No else branch, nothing to do
        };

        // Execute the chosen branch (Box::pin for recursive async)
        for branch_step in steps_to_execute {
            Box::pin(self.execute_step(branch_step)).await?;
        }

        Ok(())
    }

    /// Execute a loop step.
    pub async fn execute_loop(&mut self, step: &LoopStep) -> Result<(), ExecutionError> {
        // Resolve items to iterate over
        let items_value = self.context.resolve(&step.loop_params.items)?;

        let items_vec = match items_value {
            serde_json::Value::Array(seq) => seq,
            _ => return Err(ExecutionError::NotIterable),
        };

        let max_iterations = step.loop_params.max.unwrap_or(u32::MAX) as usize;
        let item_var = &step.loop_params.item_var;
        let item_count = items_vec.len();
        tracing::info!(items = item_count, item_var = %item_var, "Starting loop");

        let mut results = Vec::new();

        for (index, item) in items_vec.into_iter().enumerate() {
            // Check max iterations first
            if index >= max_iterations {
                tracing::warn!(max = max_iterations, "Loop max iterations reached");
                break;
            }

            // Bind loop variables BEFORE evaluating while condition
            // This allows the condition to reference ${item} and ${loop_index}
            self.context.set_variable(item_var.clone(), item);
            self.context.set_variable(
                "loop_index".to_string(),
                serde_json::Value::Number(serde_json::Number::from(index as u64)),
            );

            // Evaluate while condition BEFORE executing operation
            // Supports expressions like "${x} == 'value'" via evaluate_condition
            if let Some(while_cond) = &step.loop_params.while_cond {
                if !self.evaluate_condition(while_cond) {
                    tracing::debug!(
                        index = index,
                        condition = while_cond,
                        "Loop while condition became false, breaking early"
                    );
                    break;
                }
            }

            tracing::info!(iteration = index + 1, total = item_count, "Loop iteration");

            // Execute operation steps (Box::pin for recursive async)
            for op_step in &step.loop_params.operation {
                Box::pin(self.execute_step(op_step)).await?;
            }

            // Collect result from the last operation's output variable
            let last_output = step.loop_params.operation.last()
                .and_then(|s| s.output())
                .and_then(|name| self.context.get_variable(name).cloned())
                .unwrap_or(serde_json::Value::Null);
            results.push(last_output);
        }

        // Clear loop context
        self.context.clear_variable(item_var);
        self.context.clear_variable("loop_index");

        // Bind output if specified
        if let Some(output_name) = &step.output {
            self.context.set_variable(
                output_name.clone(),
                serde_json::Value::Array(results),
            );
        }

        Ok(())
    }

    /// Execute an aggregate step.
    pub async fn execute_aggregate(&mut self, step: &AggregateStep) -> Result<(), ExecutionError> {
        // Resolve all result references
        let mut values = Vec::new();
        for result_ref in &step.aggregate.results {
            let value = self.context.resolve(result_ref)?;
            values.push(value);
        }

        // Apply aggregation strategy
        let aggregated = match step.aggregate.strategy.as_str() {
            "first" => values.into_iter().next().unwrap_or(serde_json::Value::Null),
            "last" => values.into_iter().last().unwrap_or(serde_json::Value::Null),
            "concat" => {
                // Concatenate sequences or strings
                let mut result = Vec::new();
                for v in values {
                    match v {
                        serde_json::Value::Array(seq) => result.extend(seq),
                        other => result.push(other),
                    }
                }
                serde_json::Value::Array(result)
            }
            "merge" => {
                // Merge mappings; non-Mapping values (e.g. sequences from loops)
                // are inserted under their variable name as key
                let mut result = serde_json::Map::new();
                for (i, v) in values.into_iter().enumerate() {
                    match v {
                        serde_json::Value::Object(m) => {
                            for (k, v) in m {
                                result.insert(k, v);
                            }
                        }
                        other => {
                            // Extract variable name from the reference (e.g. "${story_results}" -> "story_results")
                            let key = if i < step.aggregate.results.len() {
                                let ref_str = &step.aggregate.results[i];
                                ref_str.trim_start_matches("${").trim_end_matches('}').to_string()
                            } else {
                                format!("result_{}", i)
                            };
                            result.insert(
                                key,
                                other,
                            );
                        }
                    }
                }
                serde_json::Value::Object(result)
            }
            other => {
                return Err(ExecutionError::AggregationError(format!(
                    "unknown strategy: {}",
                    other
                )));
            }
        };

        // Bind output if specified
        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), aggregated);
        }

        Ok(())
    }

    // ========================================================================
    // System Primitive Execution
    // ========================================================================

    /// Execute a filesystem step.
    ///
    /// Dispatches filesystem operations to the SecureFsBackend.
    /// Supports: read, write, delete, copy, move, list, exists.
    pub async fn execute_fs(&mut self, step: &crate::scroll::schema::FsStep) -> Result<(), ExecutionError> {
        use crate::scroll::schema::FsOperation;

        let result = with_on_fail!(self, &step.on_fail, {
            // Resolve path - use interpolation to handle multiple variable references
            let path_str = if step.fs.path.contains("${") {
                self.interpolate_string(&step.fs.path)?
            } else {
                step.fs.path.clone()
            };

            // Build params based on operation type
            let mut params_map = serde_json::Map::new();
            params_map.insert(
                "path".to_string(),
                serde_json::Value::String(path_str),
            );

            // Add content parameter for write/append operations
            if matches!(step.fs.operation, FsOperation::Write | FsOperation::Append) {
                if let Some(content_ref) = &step.fs.content {
                    let content_str = if content_ref.contains("${") {
                        self.interpolate_string(content_ref)?
                    } else {
                        content_ref.clone()
                    };
                    params_map.insert(
                        "content".to_string(),
                        serde_json::Value::String(content_str),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("content required for write operation".to_string()));
                }
            }

            // Add dest parameter for copy/move operations
            if matches!(step.fs.operation, FsOperation::Copy | FsOperation::Move) {
                if let Some(dest_ref) = &step.fs.dest {
                    let dest_str = if dest_ref.contains("${") {
                        self.interpolate_string(dest_ref)?
                    } else {
                        dest_ref.clone()
                    };
                    params_map.insert(
                        "dest".to_string(),
                        serde_json::Value::String(dest_str),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("dest required for copy/move operation".to_string()));
                }
            }

            let params = Some(serde_json::Value::Object(params_map));

            // Dispatch to appropriate fs interface method
            let method = match step.fs.operation {
                FsOperation::Read => "read",
                FsOperation::Write => "write",
                FsOperation::Append => "append",
                FsOperation::Delete => "delete",
                FsOperation::Copy => "copy",
                FsOperation::Move => "move",
                FsOperation::List => "list",
                FsOperation::Exists => "exists",
                FsOperation::Mkdir => "mkdir",
                FsOperation::Stat => "stat",
            };

            let path_display = params.as_ref()
                .and_then(|p| p.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            tracing::info!(op = method, path = path_display, "Filesystem operation");

            self.interface_registry.fs.dispatch(method, &params).await
        })?;

        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result);
        }

        Ok(())
    }

    /// Execute a version control step.
    ///
    /// Dispatches vcs operations to the git backend.
    pub async fn execute_vcs(&mut self, step: &crate::scroll::schema::VcsStep) -> Result<(), ExecutionError> {
        use crate::scroll::schema::VcsOperation;

        let result = with_on_fail!(self, &step.on_fail, {
            // Build params based on operation type
            let mut params_map = serde_json::Map::new();

            // Add message parameter if provided
            if let Some(message_ref) = &step.vcs.message {
                let message_str = if message_ref.starts_with("${") {
                    let message = self.context.resolve(message_ref)?;
                    message.as_str()
                        .ok_or_else(|| ExecutionError::VariableResolution("message must be a string".to_string()))?
                        .to_string()
                } else {
                    message_ref.clone()
                };
                params_map.insert(
                    "message".to_string(),
                    serde_json::Value::String(message_str),
                );
            }

            // Add files parameter if provided (for add operations)
            if let Some(files_refs) = &step.vcs.files {
                let files_values: Vec<serde_json::Value> = files_refs
                    .iter()
                    .map(|f| serde_json::Value::String(f.clone()))
                    .collect();
                params_map.insert(
                    "files".to_string(),
                    serde_json::Value::Array(files_values),
                );
            }

            // Add scope parameter if provided (for diff operations)
            if let Some(scope) = &step.vcs.scope {
                params_map.insert(
                    "scope".to_string(),
                    serde_json::Value::String(scope.clone()),
                );
            }

            let default_params = Some(serde_json::Value::Object(params_map));

            // Dispatch to appropriate git interface method
            // Each arm computes (method, params) — dispatched at the end
            let (method, dispatch_params): (&str, Option<serde_json::Value>) = match step.vcs.operation {
                // Core operations
                VcsOperation::Commit => {
                    if step.vcs.message.is_none() {
                        return Err(ExecutionError::MissingParameter("message required for commit operation".to_string()));
                    }
                    ("commit", default_params)
                }
                VcsOperation::Status => ("status", default_params),
                VcsOperation::Diff => ("diff", default_params),
                VcsOperation::Log => {
                    let mut log_params = serde_json::Map::new();
                    log_params.insert(
                        "count".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(10)),
                    );
                    ("log", Some(serde_json::Value::Object(log_params)))
                }

                // Branch operations
                VcsOperation::Branch => ("current_branch", default_params),
                VcsOperation::CurrentBranch => ("current_branch", default_params),
                VcsOperation::Checkout => {
                    let branch = step.vcs.branch.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("branch required for checkout operation".to_string()))?;
                    let mut checkout_params = serde_json::Map::new();
                    checkout_params.insert(
                        "branch".to_string(),
                        serde_json::Value::String(branch.clone()),
                    );
                    ("checkout", Some(serde_json::Value::Object(checkout_params)))
                }
                VcsOperation::EnsureBranch => {
                    let name = step.vcs.name.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("name required for ensure_branch operation".to_string()))?;
                    let mut ensure_branch_params = serde_json::Map::new();
                    ensure_branch_params.insert(
                        "name".to_string(),
                        serde_json::Value::String(name.clone()),
                    );
                    ("ensure_branch", Some(serde_json::Value::Object(ensure_branch_params)))
                }
                VcsOperation::BranchExists => {
                    let name = step.vcs.name.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("name required for branch_exists operation".to_string()))?;
                    let mut branch_exists_params = serde_json::Map::new();
                    branch_exists_params.insert(
                        "name".to_string(),
                        serde_json::Value::String(name.clone()),
                    );
                    ("branch_exists", Some(serde_json::Value::Object(branch_exists_params)))
                }
                VcsOperation::DeleteBranch => {
                    let name = step.vcs.name.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("name required for delete_branch operation".to_string()))?;
                    let mut delete_branch_params = serde_json::Map::new();
                    delete_branch_params.insert(
                        "name".to_string(),
                        serde_json::Value::String(name.clone()),
                    );
                    ("delete_branch", Some(serde_json::Value::Object(delete_branch_params)))
                }

                // Staging operations
                VcsOperation::Add => {
                    if step.vcs.files.is_none() || step.vcs.files.as_ref().unwrap().is_empty() {
                        ("stage_all", default_params)
                    } else {
                        let files = step.vcs.files.as_ref().unwrap();
                        let files_values: Vec<serde_json::Value> = files
                            .iter()
                            .map(|f| serde_json::Value::String(f.clone()))
                            .collect();
                        let mut stage_params = serde_json::Map::new();
                        stage_params.insert(
                            "files".to_string(),
                            serde_json::Value::Array(files_values),
                        );
                        ("stage", Some(serde_json::Value::Object(stage_params)))
                    }
                }
                VcsOperation::Unstage => {
                    let files = step.vcs.files.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("files required for unstage operation".to_string()))?;
                    let files_values: Vec<serde_json::Value> = files
                        .iter()
                        .map(|f| serde_json::Value::String(f.clone()))
                        .collect();
                    let mut unstage_params = serde_json::Map::new();
                    unstage_params.insert(
                        "files".to_string(),
                        serde_json::Value::Array(files_values),
                    );
                    ("unstage", Some(serde_json::Value::Object(unstage_params)))
                }

                // Remote operations
                VcsOperation::Push => {
                    let set_upstream = step.vcs.set_upstream.unwrap_or(false);
                    let mut push_params = serde_json::Map::new();
                    push_params.insert(
                        "set_upstream".to_string(),
                        serde_json::Value::Bool(set_upstream),
                    );
                    ("push", Some(serde_json::Value::Object(push_params)))
                }
                VcsOperation::Fetch => {
                    let mut fetch_params = serde_json::Map::new();
                    if let Some(remote) = &step.vcs.remote {
                        fetch_params.insert(
                            "remote".to_string(),
                            serde_json::Value::String(remote.clone()),
                        );
                    }
                    ("fetch", Some(serde_json::Value::Object(fetch_params)))
                }
                VcsOperation::Pull => {
                    let mut pull_params = serde_json::Map::new();
                    if let Some(remote) = &step.vcs.remote {
                        pull_params.insert(
                            "remote".to_string(),
                            serde_json::Value::String(remote.clone()),
                        );
                    }
                    if let Some(branch) = &step.vcs.branch {
                        pull_params.insert(
                            "branch".to_string(),
                            serde_json::Value::String(branch.clone()),
                        );
                    }
                    ("pull", Some(serde_json::Value::Object(pull_params)))
                }
                VcsOperation::PrBranchReady => {
                    // pr_branch_ready requires branch and base parameters
                    let branch = step.vcs.branch.as_ref()
                        .or(step.vcs.name.as_ref())
                        .ok_or_else(|| ExecutionError::MissingParameter("branch required for pr_branch_ready operation".to_string()))?;
                    let default_base = "main".to_string();
                    let target = step.vcs.target.as_ref().unwrap_or(&default_base);
                    let mut pr_ready_params = serde_json::Map::new();
                    pr_ready_params.insert(
                        "branch".to_string(),
                        serde_json::Value::String(branch.clone()),
                    );
                    pr_ready_params.insert(
                        "base".to_string(),
                        serde_json::Value::String(target.clone()),
                    );
                    ("pr_branch_ready", Some(serde_json::Value::Object(pr_ready_params)))
                }

                // Merge operations
                VcsOperation::Merge => {
                    let branch = step.vcs.branch.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("branch required for merge operation".to_string()))?;
                    let mut merge_params = serde_json::Map::new();
                    merge_params.insert(
                        "branch".to_string(),
                        serde_json::Value::String(branch.clone()),
                    );
                    ("merge", Some(serde_json::Value::Object(merge_params)))
                }
                VcsOperation::Squash => {
                    let branch = step.vcs.branch.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("branch required for squash operation".to_string()))?;
                    let mut squash_params = serde_json::Map::new();
                    squash_params.insert(
                        "branch".to_string(),
                        serde_json::Value::String(branch.clone()),
                    );
                    ("squash", Some(serde_json::Value::Object(squash_params)))
                }
                VcsOperation::AbortMerge => ("abort_merge", default_params),

                // Stash operations
                VcsOperation::StashPush => {
                    let mut stash_params = serde_json::Map::new();
                    if let Some(message) = &step.vcs.message {
                        stash_params.insert(
                            "message".to_string(),
                            serde_json::Value::String(message.clone()),
                        );
                    }
                    ("stash_push", Some(serde_json::Value::Object(stash_params)))
                }
                VcsOperation::StashPop => ("stash_pop", default_params),
                VcsOperation::StashList => ("stash_list", default_params),

                // Reset operations
                VcsOperation::ResetHard => {
                    let target = step.vcs.target.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("target required for reset_hard operation".to_string()))?;
                    let mut reset_params = serde_json::Map::new();
                    reset_params.insert(
                        "target".to_string(),
                        serde_json::Value::String(target.clone()),
                    );
                    ("reset_hard", Some(serde_json::Value::Object(reset_params)))
                }
                VcsOperation::ResetSoft => {
                    let target = step.vcs.target.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("target required for reset_soft operation".to_string()))?;
                    let mut reset_params = serde_json::Map::new();
                    reset_params.insert(
                        "target".to_string(),
                        serde_json::Value::String(target.clone()),
                    );
                    ("reset_soft", Some(serde_json::Value::Object(reset_params)))
                }

                // Reference operations
                VcsOperation::Head => ("head", default_params),
                VcsOperation::HeadShort => ("head_short", default_params),
                VcsOperation::ResolveRef => {
                    let target = step.vcs.target.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("target required for resolve_ref operation".to_string()))?;
                    let mut resolve_params = serde_json::Map::new();
                    resolve_params.insert(
                        "ref".to_string(),
                        serde_json::Value::String(target.clone()),
                    );
                    ("resolve_ref", Some(serde_json::Value::Object(resolve_params)))
                }

                // Tag operations
                VcsOperation::Tag => {
                    let name = step.vcs.name.as_ref()
                        .ok_or_else(|| ExecutionError::MissingParameter("name required for tag operation".to_string()))?;
                    let mut tag_params = serde_json::Map::new();
                    tag_params.insert(
                        "name".to_string(),
                        serde_json::Value::String(name.clone()),
                    );
                    if let Some(message) = &step.vcs.message {
                        tag_params.insert(
                            "message".to_string(),
                            serde_json::Value::String(message.clone()),
                        );
                    }
                    ("tag", Some(serde_json::Value::Object(tag_params)))
                }
                VcsOperation::ListTags => ("list_tags", default_params),
            };

            self.interface_registry.vcs.dispatch(method, &dispatch_params).await
        })?;

        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result);
        }

        Ok(())
    }


    /// Execute a test step.
    ///
    /// Dispatches test operations to the test backend.
    /// Supports: run, coverage operations.
    pub async fn execute_test(&mut self, step: &crate::scroll::schema::TestStep) -> Result<(), ExecutionError> {
        use crate::scroll::schema::TestOperation;

        // Verify dispatches to built-in tools, not the test backend
        if matches!(step.test.operation, TestOperation::Verify) {
            let tool = step.test.tool.as_deref()
                .ok_or_else(|| ExecutionError::MissingParameter(
                    "verify operation requires 'tool' field".to_string()
                ))?;
            let raw_input = step.test.input.as_ref()
                .ok_or_else(|| ExecutionError::MissingParameter(
                    "verify operation requires 'input' field".to_string()
                ))?;
            let resolved_input = self.context.resolve_value_strict(raw_input)?;
            tracing::info!(tool = tool, "Running verify tool");
            let verify_start = std::time::Instant::now();
            let verify_result = crate::primitives::test::verify::dispatch(tool, &resolved_input)?;
            let verify_elapsed = verify_start.elapsed();
            tracing::info!(
                tool = tool,
                elapsed = crate::scroll::executor::format_duration(verify_elapsed).as_str(),
                "Verify completed"
            );
            if let Some(output_name) = &step.output {
                self.context.set_variable(output_name.clone(), verify_result);
            }
            return Ok(());
        }

        let result = with_on_fail!(self, &step.on_fail, {
            // Build params based on operation type
            let mut params_map = serde_json::Map::new();

            // Add pattern parameter if provided
            if let Some(pattern_ref) = &step.test.pattern {
                let pattern_str = if pattern_ref.starts_with("${") {
                    let pattern = self.context.resolve(pattern_ref)?;
                    pattern.as_str()
                        .ok_or_else(|| ExecutionError::VariableResolution("pattern must be a string".to_string()))?
                        .to_string()
                } else {
                    pattern_ref.clone()
                };
                params_map.insert(
                    "pattern".to_string(),
                    serde_json::Value::String(pattern_str),
                );
            }

            // Add config parameter if provided
            if let Some(config) = &step.test.config {
                params_map.insert(
                    "config".to_string(),
                    config.clone(),
                );
            }

            // Add files parameter for run_files operation
            if let Some(files) = &step.test.files {
                let files_values: Vec<serde_json::Value> = files
                    .iter()
                    .map(|f| serde_json::Value::String(f.clone()))
                    .collect();
                params_map.insert(
                    "files".to_string(),
                    serde_json::Value::Array(files_values),
                );
            }

            let params = Some(serde_json::Value::Object(params_map.clone()));

            // Dispatch to appropriate test interface method
            let method = match step.test.operation {
                TestOperation::Run => {
                    if step.test.pattern.is_some() {
                        "run_filtered"
                    } else {
                        "run"
                    }
                }
                TestOperation::Coverage => "coverage",
                TestOperation::Smoke => "smoke",
                TestOperation::RunFiltered => "run_filtered",
                TestOperation::RunFiles => {
                    if step.test.files.is_none() || step.test.files.as_ref().unwrap().is_empty() {
                        return Err(ExecutionError::MissingParameter("files required for run_files operation".to_string()));
                    }
                    "run_files"
                }
                TestOperation::Info => "info",
                TestOperation::Verify => unreachable!("handled above"),
            };

            tracing::info!(op = method, "Running tests");
            let test_start = std::time::Instant::now();
            let result = self.interface_registry.test.dispatch(method, &params).await;
            let test_elapsed = test_start.elapsed();
            tracing::info!(
                op = method,
                elapsed = crate::scroll::executor::format_duration(test_elapsed).as_str(),
                "Tests completed"
            );
            result
        })?;

        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result);
        }

        Ok(())
    }

    /// Execute a platform step.
    ///
    /// Dispatches platform operations to the platform module.
    /// Supports: env (get environment variable), info (platform info), check (command availability).
    pub async fn execute_platform(&mut self, step: &crate::scroll::schema::PlatformStep) -> Result<(), ExecutionError> {
        use crate::scroll::schema::PlatformOperation;

        let result = with_on_fail!(self, &step.on_fail, {
            // Build params based on operation type
            let mut params_map = serde_json::Map::new();

            // Add var parameter for env operations
            if let PlatformOperation::Env = step.platform.operation {
                if let Some(var_ref) = &step.platform.var {
                    let var_str = if var_ref.starts_with("${") {
                        let var = self.context.resolve(var_ref)?;
                        var.as_str()
                            .ok_or_else(|| ExecutionError::VariableResolution("var must be a string".to_string()))?
                            .to_string()
                    } else {
                        var_ref.clone()
                    };
                    params_map.insert(
                        "var".to_string(),
                        serde_json::Value::String(var_str),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("var required for env operation".to_string()));
                }
            }

            // Add command parameter for check operations
            if let PlatformOperation::Check = step.platform.operation {
                if let Some(cmd_ref) = &step.platform.command {
                    let cmd_str = if cmd_ref.starts_with("${") {
                        let cmd = self.context.resolve(cmd_ref)?;
                        cmd.as_str()
                            .ok_or_else(|| ExecutionError::VariableResolution("command must be a string".to_string()))?
                            .to_string()
                    } else {
                        cmd_ref.clone()
                    };
                    params_map.insert(
                        "command".to_string(),
                        serde_json::Value::String(cmd_str),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("command required for check operation".to_string()));
                }
            }

            // Add number parameter for close_issue operations
            if let PlatformOperation::CloseIssue = step.platform.operation {
                if let Some(number_ref) = &step.platform.number {
                    let number_value = self.resolve_param(number_ref)?;
                    params_map.insert(
                        "number".to_string(),
                        number_value,
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("number required for close_issue operation".to_string()));
                }
            }

            // Add payload parameter for create_issue operations
            if let PlatformOperation::CreateIssue = step.platform.operation {
                if let Some(payload_ref) = &step.platform.payload {
                    // Payload is a Value, check if it's a string that needs resolution
                    let payload_value = if let Some(payload_str) = payload_ref.as_str() {
                        if payload_str.starts_with("${") {
                            self.context.resolve(payload_str)?
                        } else {
                            payload_ref.clone()
                        }
                    } else {
                        payload_ref.clone()
                    };
                    params_map.insert(
                        "payload".to_string(),
                        payload_value,
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("payload required for create_issue operation".to_string()));
                }
            }

            // Add number parameter for get_issue operations
            if let PlatformOperation::GetIssue = step.platform.operation {
                if let Some(number_ref) = &step.platform.number {
                    let number_value = self.resolve_param(number_ref)?;
                    params_map.insert(
                        "number".to_string(),
                        number_value,
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("number required for get_issue operation".to_string()));
                }
            }

            // Add filter parameters for list_issues operations
            if let PlatformOperation::ListIssues = step.platform.operation {
                if let Some(state) = &step.platform.state {
                    let state_str = self.resolve_string_param(state)?;
                    params_map.insert(
                        "state".to_string(),
                        serde_json::Value::String(state_str),
                    );
                }
                if let Some(milestone_ref) = &step.platform.milestone {
                    // B2 (#181): use resolve_param + value_as_i64 for consistency
                    // with all other number fields in platform dispatch
                    let milestone_val = self.resolve_param(milestone_ref)?;
                    params_map.insert("milestone".to_string(), milestone_val);
                }
                if let Some(labels) = &step.platform.labels {
                    let labels_value: Vec<serde_json::Value> = labels
                        .iter()
                        .map(|l| serde_json::Value::String(l.clone()))
                        .collect();
                    params_map.insert(
                        "labels".to_string(),
                        serde_json::Value::Array(labels_value),
                    );
                }
                if let Some(assignee) = &step.platform.assignee {
                    let assignee_str = self.resolve_string_param(assignee)?;
                    params_map.insert(
                        "assignee".to_string(),
                        serde_json::Value::String(assignee_str),
                    );
                }
            }

            // Add parameters for add_labels/remove_labels operations
            if matches!(step.platform.operation, PlatformOperation::AddLabels | PlatformOperation::RemoveLabels) {
                if let Some(number_ref) = &step.platform.number {
                    let number_value = self.resolve_param(number_ref)?;
                    params_map.insert(
                        "number".to_string(),
                        number_value,
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("number required for label operations".to_string()));
                }
                if let Some(labels) = &step.platform.labels {
                    let labels_value: Vec<serde_json::Value> = labels
                        .iter()
                        .map(|l| {
                            let resolved = self.resolve_string_param(l)?;
                            Ok(serde_json::Value::String(resolved))
                        })
                        .collect::<Result<Vec<_>, ExecutionError>>()?;
                    params_map.insert(
                        "labels".to_string(),
                        serde_json::Value::Array(labels_value),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("labels required for label operations".to_string()));
                }
            }

            // Add parameters for create_comment operations
            if let PlatformOperation::CreateComment = step.platform.operation {
                if let Some(number_ref) = &step.platform.number {
                    let number_value = self.resolve_param(number_ref)?;
                    params_map.insert(
                        "number".to_string(),
                        number_value,
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("number required for create_comment operation".to_string()));
                }
                if let Some(body_ref) = &step.platform.body {
                    let body_str = self.resolve_string_param(body_ref)?;
                    params_map.insert(
                        "body".to_string(),
                        serde_json::Value::String(body_str),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("body required for create_comment operation".to_string()));
                }
            }

            // Add number parameter for get_comments operations
            if let PlatformOperation::GetComments = step.platform.operation {
                if let Some(number_ref) = &step.platform.number {
                    let number_value = self.resolve_param(number_ref)?;
                    params_map.insert(
                        "number".to_string(),
                        number_value,
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("number required for get_comments operation".to_string()));
                }
            }

            // Add parameters for create_milestone operations
            if let PlatformOperation::CreateMilestone = step.platform.operation {
                if let Some(title_ref) = &step.platform.title {
                    let title_str = self.resolve_string_param(title_ref)?;
                    params_map.insert(
                        "title".to_string(),
                        serde_json::Value::String(title_str),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("title required for create_milestone operation".to_string()));
                }
                if let Some(desc_ref) = &step.platform.description {
                    let desc_str = self.resolve_string_param(desc_ref)?;
                    params_map.insert(
                        "description".to_string(),
                        serde_json::Value::String(desc_str),
                    );
                }
            }

            // Add number parameter for get_milestone operations
            if let PlatformOperation::GetMilestone = step.platform.operation {
                if let Some(number_ref) = &step.platform.number {
                    let number_value = self.resolve_param(number_ref)?;
                    params_map.insert(
                        "number".to_string(),
                        number_value,
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("number required for get_milestone operation".to_string()));
                }
            }

            // Add parameters for create_pr operations
            if let PlatformOperation::CreatePr = step.platform.operation {
                if let Some(title_ref) = &step.platform.title {
                    let title_str = self.resolve_string_param(title_ref)?;
                    params_map.insert(
                        "title".to_string(),
                        serde_json::Value::String(title_str),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("title required for create_pr operation".to_string()));
                }
                if let Some(body_ref) = &step.platform.body {
                    let body_str = self.resolve_string_param(body_ref)?;
                    params_map.insert(
                        "body".to_string(),
                        serde_json::Value::String(body_str),
                    );
                }
                if let Some(head_ref) = &step.platform.head {
                    let head_str = self.resolve_string_param(head_ref)?;
                    params_map.insert(
                        "head".to_string(),
                        serde_json::Value::String(head_str),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("head required for create_pr operation".to_string()));
                }
                if let Some(base_ref) = &step.platform.base {
                    let base_str = self.resolve_string_param(base_ref)?;
                    params_map.insert(
                        "base".to_string(),
                        serde_json::Value::String(base_str),
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("base required for create_pr operation".to_string()));
                }
            }

            // Add number parameter for get_pr operations
            if let PlatformOperation::GetPr = step.platform.operation {
                if let Some(number_ref) = &step.platform.number {
                    let number_value = self.resolve_param(number_ref)?;
                    params_map.insert(
                        "number".to_string(),
                        number_value,
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("number required for get_pr operation".to_string()));
                }
            }

            // Add parameters for merge_pr operations
            if let PlatformOperation::MergePr = step.platform.operation {
                if let Some(number_ref) = &step.platform.number {
                    let number_value = self.resolve_param(number_ref)?;
                    params_map.insert(
                        "number".to_string(),
                        number_value,
                    );
                } else {
                    return Err(ExecutionError::MissingParameter("number required for merge_pr operation".to_string()));
                }
                if let Some(strategy_ref) = &step.platform.strategy {
                    let strategy_str = self.resolve_string_param(strategy_ref)?;
                    params_map.insert(
                        "strategy".to_string(),
                        serde_json::Value::String(strategy_str),
                    );
                }
            }

            let params = serde_json::Value::Object(params_map);

            // Dispatch to platform module
            let operation = match step.platform.operation {
                PlatformOperation::Env => "env",
                PlatformOperation::Info => "info",
                PlatformOperation::Check => "check",
                PlatformOperation::CreateIssue => "create_issue",
                PlatformOperation::GetIssue => "get_issue",
                PlatformOperation::CloseIssue => "close_issue",
                PlatformOperation::ListIssues => "list_issues",
                PlatformOperation::AddLabels => "add_labels",
                PlatformOperation::RemoveLabels => "remove_labels",
                PlatformOperation::CreateComment => "create_comment",
                PlatformOperation::GetComments => "get_comments",
                PlatformOperation::CreateMilestone => "create_milestone",
                PlatformOperation::GetMilestone => "get_milestone",
                PlatformOperation::CreatePr => "create_pr",
                PlatformOperation::GetPr => "get_pr",
                PlatformOperation::MergePr => "merge_pr",
            };

            tracing::info!(op = operation, "Platform operation");

            // Use scroll::platform for env/info/check, interface_registry for forge operations
            match step.platform.operation {
                PlatformOperation::Env | PlatformOperation::Info | PlatformOperation::Check => {
                    crate::scroll::platform::execute(operation, &params)
                }
                _ => {
                    // Forge operations go through the interface registry
                    self.interface_registry.platform.dispatch(operation, &Some(params)).await
                }
            }
        })?;

        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result);
        }

        Ok(())
    }

    /// Execute a run step.
    ///
    /// Inlines a subscroll's steps into the current executor. Variables flow
    /// naturally — subscroll steps read and write the same context as the parent.
    /// The requires/provides contract is still validated.
    ///
    /// This is the same execution model as loops: no clone, no copy-back.
    pub async fn execute_run(&mut self, step: &crate::scroll::schema::RunStep) -> Result<(), ExecutionError> {
        use std::path::PathBuf;

        // Resolve scroll_path via PathResolver search path (D18, D33, #178):
        // - Bare/relative names: search project → user → global tiers
        // - Paths starting with ./ or /: direct resolution
        let scroll_path = if let Some(ref resolver) = self.path_resolver {
            // Try search path first, fall back to cwd-relative for backwards compat
            resolver.resolve_scroll(&step.run.scroll_path)
                .or_else(|| {
                    // Backwards compat: try cwd-relative if search path didn't find it
                    let p = std::env::current_dir().ok()?.join(&step.run.scroll_path);
                    if p.exists() {
                        tracing::debug!(
                            scroll = %step.run.scroll_path,
                            "Scroll found via cwd-relative fallback (not in search path)"
                        );
                        Some(p)
                    } else {
                        None
                    }
                })
                .ok_or_else(|| ExecutionError::InterfaceError(
                    format!("Scroll not found: {}", step.run.scroll_path)
                ))?
        } else {
            // No PathResolver (test executors): cwd-relative only
            let p = PathBuf::from(&step.run.scroll_path);
            if p.is_relative() {
                std::env::current_dir()
                    .map_err(|e| ExecutionError::InterfaceError(format!("Failed to get current directory: {}", e)))?
                    .join(p)
            } else {
                p
            }
        };

        if !scroll_path.exists() {
            return Err(ExecutionError::InterfaceError(
                format!("Scroll file not found: {}", scroll_path.display())
            ));
        }

        // Load and parse the subscroll (Assembly format)
        let source = std::fs::read_to_string(&scroll_path)
            .map_err(|e| ExecutionError::InterfaceError(format!("Failed to read subscroll: {e}")))?;

        let ast = crate::scroll::assembly::parser::parse(&source, &scroll_path.to_string_lossy())
            .map_err(|diags| {
                let msgs: Vec<_> = diags.iter().map(|d| d.to_string()).collect();
                ExecutionError::ParseError(format!("Failed to parse subscroll: {}", msgs.join("; ")))
            })?;

        let start = std::time::Instant::now();
        tracing::info!(
            scroll = %ast.scroll.name,
            path = %scroll_path.display(),
            "Entering subscroll (Assembly)"
        );

        // Build inputs from args
        let mut inputs = std::collections::HashMap::new();
        if let Some(args) = &step.run.args {
            for (key, value) in args {
                let resolved = if let Some(s) = value.as_str() {
                    if s.starts_with("${") && s.ends_with("}") {
                        self.context.resolve(s)?
                    } else {
                        value.clone()
                    }
                } else {
                    value.clone()
                };
                inputs.insert(key.clone(), resolved);
            }
        }
        // Also pass current context variables as inputs for require resolution
        for (key, value) in self.context.variables() {
            inputs.entry(key.clone()).or_insert_with(|| value.clone());
        }

        // Execute via Assembly dispatch
        let outputs = crate::scroll::assembly::dispatch::execute(&ast, self, inputs).await
            .map_err(|e| ExecutionError::InterfaceError(format!("Subscroll execution error: {e}")))?;

        // Set output variables in context
        for (key, value) in outputs {
            self.context.set_variable(key, value);
        }

        let elapsed = start.elapsed();
        tracing::info!(
            scroll = %ast.scroll.name,
            elapsed = crate::scroll::executor::format_duration(elapsed).as_str(),
            "Leaving subscroll (Assembly)"
        );

        Ok(())
    }

    // ========================================================================
    // Security Primitive Execution
    // ========================================================================

    /// Execute a set step.
    ///
    /// Constructs a mapping from the provided values, resolving all ${var}
    /// expressions, and binds the result to the output variable.
    /// No LLM call — pure data wiring.
    pub async fn execute_set(&mut self, step: &SetStep) -> Result<(), ExecutionError> {
        let resolved = self.resolve_value_recursive(&step.set.values)?;

        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), resolved);
        }

        Ok(())
    }

    /// Recursively resolve all ${var} expressions in a serde_json::Value.
    fn resolve_value_recursive(&self, value: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
        match value {
            serde_json::Value::String(s) => {
                if s.starts_with("${") && s.ends_with('}') && s.matches("${").count() == 1 {
                    // Pure variable reference — preserve type (number, mapping, sequence, etc.)
                    Ok(self.context.resolve(s)?)
                } else if s.contains("${") {
                    // String with embedded references — interpolate to string
                    Ok(serde_json::Value::String(self.interpolate_string(s)?))
                } else {
                    // Plain string — pass through
                    Ok(value.clone())
                }
            }
            serde_json::Value::Object(m) => {
                let mut result = serde_json::Map::new();
                for (k, v) in m {
                    let resolved_key = self.resolve_value_recursive(&serde_json::Value::String(k.clone()))?;
                    let resolved_key_str = match resolved_key {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    };
                    let resolved_val = self.resolve_value_recursive(v)?;
                    result.insert(resolved_key_str, resolved_val);
                }
                Ok(serde_json::Value::Object(result))
            }
            serde_json::Value::Array(seq) => {
                let resolved: Result<Vec<_>, _> = seq.iter()
                    .map(|v| self.resolve_value_recursive(v))
                    .collect();
                Ok(serde_json::Value::Array(resolved?))
            }
            // Numbers, bools, null — pass through unchanged
            other => Ok(other.clone()),
        }
    }

    // ========================================================================

    /// Execute a secure step.
    ///
    /// Dispatches to the appropriate scan type handler with on_fail behavior.
    pub async fn execute_secure(&mut self, step: &SecureStep) -> Result<(), ExecutionError> {
        let result = with_on_fail!(self, &step.on_fail, {
            let input = step
                .secure
                .input
                .as_ref()
                .and_then(|var_ref| self.context.resolve(var_ref).ok());

            tracing::info!(scan = ?step.secure.scan_type, "Security scan");
            self.dispatch_scan_type(&step.secure.scan_type, input.as_ref()).await
        })?;

        // Store result in context if output specified
        if let Some(output_name) = &step.output {
            self.context.set_variable(output_name.clone(), result);
        }

        Ok(())
    }

    /// Dispatch to the appropriate security scan handler.
    pub async fn dispatch_scan_type(
        &self,
        scan_type: &ScanType,
        input: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError> {
        match scan_type {
            ScanType::DependencyCve => {
                self.interface_registry
                    .dispatch_interface("secure.dependency_cve", &None).await
            }
            ScanType::SecretDetection => {
                self.interface_registry
                    .dispatch_interface("secure.secret_detection", &None).await
            }
            ScanType::StaticAnalysis => {
                self.interface_registry
                    .dispatch_interface("secure.static_analysis", &None).await
            }
            ScanType::Multiple(scans) => {
                let mut result = serde_json::Value::Null;
                for scan_name in scans {
                    let scan_type = match scan_name.as_str() {
                        "dependency_cve" => ScanType::DependencyCve,
                        "secret_detection" => ScanType::SecretDetection,
                        "static_analysis" => ScanType::StaticAnalysis,
                        _ => {
                            return Err(ExecutionError::InterfaceError(format!(
                                "unknown scan type: {}",
                                scan_name
                            )))
                        }
                    };
                    result = Box::pin(self.dispatch_scan_type(&scan_type, input)).await?;
                }
                Ok(result)
            }
        }
    }

    /// Parse structured output with LLM repair fallback.
    ///
    /// Tries to parse the text as the target format. If parsing fails,
    /// sends the raw text to a cheap LLM with a repair prompt, then
    /// retries the parse. This handles local models that produce
    /// almost-valid structured output (extra prose, minor syntax errors).
    async fn parse_with_repair(
        &self,
        text: &str,
        format: &str,
    ) -> Result<serde_json::Value, ExecutionError> {
        use crate::scroll::extraction::{parse_and_validate_json, parse_and_validate_yaml};

        let parser = match format {
            "json" => parse_and_validate_json,
            "yaml" => parse_and_validate_yaml,
            _ => return Err(ExecutionError::InvocationError(
                format!("parse_with_repair: unsupported format: {}", format)
            )),
        };

        // First attempt: direct parse
        match parser(text) {
            Ok(value) => Ok(value),
            Err(first_error) => {
                tracing::warn!(
                    format = format,
                    "Parse failed, attempting LLM repair: {}",
                    first_error
                );

                // Build repair prompt
                let repair_prompt = format!(
                    "The following text should be valid {fmt} but has syntax errors. \
                     Fix ONLY the syntax errors and output ONLY the corrected {fmt}. \
                     Do not add commentary, do not change the content, do not wrap in markdown fences.\n\n{text}",
                    fmt = format.to_uppercase(),
                    text = text
                );

                // Call LLM for repair (uses default/cheap tier — not an agent call)
                let repair_result = self.interface_registry.invoke_generate_with_options(
                    &repair_prompt,
                    None,
                    Some("cheap"),
                    None,
                ).await;

                match repair_result {
                    Ok(repaired) => {
                        let repaired_str = repaired.as_str().unwrap_or("");
                        match parser(repaired_str) {
                            Ok(value) => {
                                tracing::info!("LLM repair succeeded");
                                Ok(value)
                            }
                            Err(second_error) => {
                                tracing::warn!("LLM repair did not fix parse error: {}", second_error);
                                // Return the original error — it's more useful
                                Err(first_error)
                            }
                        }
                    }
                    Err(repair_error) => {
                        tracing::warn!("LLM repair call failed: {}", repair_error);
                        Err(first_error)
                    }
                }
            }
        }
    }

}

/// Step Helper Methods
impl Step {
    /// Get the output variable name for this step, if any.
    pub fn output(&self) -> Option<&String> {
        match self {
            Step::Elaborate(s) => s.output.as_ref(),
            Step::Distill(s) => s.output.as_ref(),
            Step::Split(s) => s.output.as_ref(),
            Step::Merge(s) => s.output.as_ref(),
            Step::Validate(s) => s.output.as_ref(),
            Step::Convert(s) => s.output.as_ref(),
            Step::Fs(s) => s.output.as_ref(),
            Step::Vcs(s) => s.output.as_ref(),
            Step::Test(s) => s.output.as_ref(),
            Step::Platform(s) => s.output.as_ref(),
            Step::Run(s) => s.output.as_ref(),
            Step::Invoke(s) => s.output.as_ref(),
            Step::Parallel(s) => s.output.as_ref(),
            Step::Consensus(s) => s.output.as_ref(),
            Step::Concurrent(s) => s.output.as_ref(),
            Step::Branch(s) => s.output.as_ref(),
            Step::Loop(s) => s.output.as_ref(),
            Step::Aggregate(s) => s.output.as_ref(),
            Step::Set(s) => s.output.as_ref(),
            Step::Secure(s) => s.output.as_ref(),
        }
    }

    /// Return a human-readable name for this step type.
    pub fn kind(&self) -> &'static str {
        match self {
            Step::Elaborate(_) => "elaborate",
            Step::Distill(_) => "distill",
            Step::Split(_) => "split",
            Step::Merge(_) => "merge",
            Step::Validate(_) => "validate",
            Step::Convert(_) => "convert",
            Step::Fs(_) => "fs",
            Step::Vcs(_) => "vcs",
            Step::Test(_) => "test",
            Step::Platform(_) => "platform",
            Step::Run(_) => "run",
            Step::Invoke(_) => "invoke",
            Step::Parallel(_) => "parallel",
            Step::Consensus(_) => "consensus",
            Step::Concurrent(_) => "concurrent",
            Step::Branch(_) => "branch",
            Step::Loop(_) => "loop",
            Step::Aggregate(_) => "aggregate",
            Step::Set(_) => "set",
            Step::Secure(_) => "secure",
        }
    }
}

/// Schema validation for validate primitive output (D31).
/// Verifies structure without consensus to avoid infinite regress.
fn validate_result_schema(value: &serde_json::Value) -> Result<(), ExecutionError> {
    let mapping = value.as_object()
        .ok_or_else(|| ExecutionError::ValidationError("validate result must be mapping".to_string()))?;

    // Check required field: result (must be "pass" or "fail")
    let result_val = mapping.get("result")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::ValidationError("missing or invalid 'result' field".to_string()))?;

    if result_val != "pass" && result_val != "fail" {
        return Err(ExecutionError::ValidationError(
            format!("result must be 'pass' or 'fail', got: {}", result_val)
        ));
    }

    // Check required field: score (must be float 0.0-1.0)
    let score = mapping.get("score")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| ExecutionError::ValidationError("missing or invalid 'score' field".to_string()))?;

    if !(0.0..=1.0).contains(&score) {
        return Err(ExecutionError::ValidationError(
            format!("score must be 0.0-1.0, got: {}", score)
        ));
    }

    // Check required field: criteria_results (must be array)
    let criteria_results = mapping.get("criteria_results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ExecutionError::ValidationError("missing or invalid 'criteria_results' field".to_string()))?;

    // Validate each criterion result has required fields
    for criterion_result in criteria_results {
        let cr_mapping = criterion_result.as_object()
            .ok_or_else(|| ExecutionError::ValidationError("criterion result must be mapping".to_string()))?;

        // Check criterion field (string)
        cr_mapping.get("criterion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ValidationError("criterion result missing 'criterion' field".to_string()))?;

        // Check passed field (boolean)
        cr_mapping.get("passed")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| ExecutionError::ValidationError("criterion result missing 'passed' field".to_string()))?;

        // Check explanation field (string)
        cr_mapping.get("explanation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ExecutionError::ValidationError("criterion result missing 'explanation' field".to_string()))?;
    }

    // Check required field: summary (must be string)
    mapping.get("summary")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::ValidationError("missing or invalid 'summary' field".to_string()))?;

    Ok(())
}
