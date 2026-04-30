// SPDX-License-Identifier: MIT
//! `string.*` builtin namespace — pure, deterministic string operations.
//!
//! Every method is a pure function of its arguments. No I/O. No mutation of
//! inputs. Errors are returned as `ExecutionError::InvalidStep` so callers see
//! a useful message at the call site.

use crate::scroll::error::ExecutionError;
use serde_json::{Map, Value};

/// Dispatch a `string.<method>` call.
///
/// `args` carries both keyword args (by name) and positional args (encoded as
/// `__pos_0`, `__pos_1`, ...). The dispatcher resolves either form into the
/// canonical parameter names for each method.
pub fn dispatch(method: &str, args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    match method {
        "line" => line(args),
        "split" => split(args),
        "join" => join(args),
        "lines" => lines(args),
        "trim" => trim(args),
        "lower" => lower(args),
        "upper" => upper(args),
        "contains" => contains(args),
        "replace" => replace(args),
        other => Err(ExecutionError::InvalidStep(format!(
            "unknown string method: 'string.{other}'"
        ))),
    }
}

// ============================================================================
// Argument helpers
// ============================================================================

fn err(msg: impl Into<String>) -> ExecutionError {
    ExecutionError::InvalidStep(msg.into())
}

fn pick<'a>(args: &'a Map<String, Value>, name: &str, pos: usize) -> Option<&'a Value> {
    args.get(name).or_else(|| args.get(&format!("__pos_{pos}")))
}

fn get_str<'a>(args: &'a Map<String, Value>, name: &str, pos: usize) -> Result<&'a str, ExecutionError> {
    let v = pick(args, name, pos)
        .ok_or_else(|| err(format!("string.* missing argument '{name}'")))?;
    v.as_str().ok_or_else(|| {
        err(format!(
            "string.* argument '{name}' must be a string, got {}",
            type_name(v)
        ))
    })
}

fn get_int(args: &Map<String, Value>, name: &str, pos: usize) -> Result<i64, ExecutionError> {
    let v = pick(args, name, pos)
        .ok_or_else(|| err(format!("string.* missing argument '{name}'")))?;
    v.as_i64().ok_or_else(|| {
        err(format!(
            "string.* argument '{name}' must be an integer, got {}",
            type_name(v)
        ))
    })
}

fn get_str_array<'a>(
    args: &'a Map<String, Value>,
    name: &str,
    pos: usize,
) -> Result<Vec<&'a str>, ExecutionError> {
    let v = pick(args, name, pos)
        .ok_or_else(|| err(format!("string.* missing argument '{name}'")))?;
    let arr = v.as_array().ok_or_else(|| {
        err(format!(
            "string.* argument '{name}' must be an array, got {}",
            type_name(v)
        ))
    })?;
    arr.iter()
        .enumerate()
        .map(|(i, item)| {
            item.as_str().ok_or_else(|| {
                err(format!(
                    "string.* argument '{name}'[{i}] must be a string, got {}",
                    type_name(item)
                ))
            })
        })
        .collect()
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// ============================================================================
// Methods
// ============================================================================

fn line(args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let input = get_str(args, "input", 0)?;
    let n = get_int(args, "n", 1)?;
    if n < 1 {
        return Err(err(format!(
            "string.line: line number must be >= 1, got {n}"
        )));
    }
    let idx = (n - 1) as usize;
    let lines: Vec<&str> = input.split('\n').collect();
    let line = lines.get(idx).ok_or_else(|| {
        err(format!(
            "string.line: line {n} out of bounds (input has {} lines)",
            lines.len()
        ))
    })?;
    Ok(Value::String((*line).to_string()))
}

fn split(args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let input = get_str(args, "input", 0)?;
    let separator = get_str(args, "separator", 1)?;
    if separator.is_empty() {
        return Err(err("string.split: separator must not be empty"));
    }
    let parts: Vec<Value> = input
        .split(separator)
        .map(|s| Value::String(s.to_string()))
        .collect();
    Ok(Value::Array(parts))
}

fn join(args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let parts = get_str_array(args, "parts", 0)?;
    let separator = get_str(args, "separator", 1)?;
    Ok(Value::String(parts.join(separator)))
}

fn lines(args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let input = get_str(args, "input", 0)?;
    let parts: Vec<Value> = input
        .split('\n')
        .map(|s| Value::String(s.to_string()))
        .collect();
    Ok(Value::Array(parts))
}

fn trim(args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let input = get_str(args, "input", 0)?;
    Ok(Value::String(input.trim().to_string()))
}

fn lower(args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let input = get_str(args, "input", 0)?;
    Ok(Value::String(input.to_lowercase()))
}

fn upper(args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let input = get_str(args, "input", 0)?;
    Ok(Value::String(input.to_uppercase()))
}

fn contains(args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let input = get_str(args, "input", 0)?;
    let needle = get_str(args, "needle", 1)?;
    Ok(Value::Bool(input.contains(needle)))
}

fn replace(args: &Map<String, Value>) -> Result<Value, ExecutionError> {
    let input = get_str(args, "input", 0)?;
    let from = get_str(args, "from", 1)?;
    let to = get_str(args, "to", 2)?;
    Ok(Value::String(input.replace(from, to)))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn m(pairs: &[(&str, Value)]) -> Map<String, Value> {
        let mut map = Map::new();
        for (k, v) in pairs {
            map.insert((*k).to_string(), v.clone());
        }
        map
    }

    // ---- string.line ------------------------------------------------------

    #[test]
    fn line_one_indexed_correctness() {
        let args = m(&[("input", json!("a\nb\nc")), ("n", json!(2))]);
        assert_eq!(dispatch("line", &args).unwrap(), json!("b"));
    }

    #[test]
    fn line_first_line() {
        let args = m(&[("input", json!("first\nsecond")), ("n", json!(1))]);
        assert_eq!(dispatch("line", &args).unwrap(), json!("first"));
    }

    #[test]
    fn line_oob_errors() {
        let args = m(&[("input", json!("only")), ("n", json!(2))]);
        assert!(dispatch("line", &args).is_err());
    }

    #[test]
    fn line_zero_or_negative_errors() {
        let args = m(&[("input", json!("a")), ("n", json!(0))]);
        assert!(dispatch("line", &args).is_err());
    }

    #[test]
    fn line_empty_input() {
        let args = m(&[("input", json!("")), ("n", json!(1))]);
        assert_eq!(dispatch("line", &args).unwrap(), json!(""));
    }

    #[test]
    fn line_trailing_newline_creates_empty_last() {
        let args = m(&[("input", json!("a\nb\n")), ("n", json!(3))]);
        assert_eq!(dispatch("line", &args).unwrap(), json!(""));
    }

    #[test]
    fn line_positional_args() {
        let args = m(&[("__pos_0", json!("x\ny")), ("__pos_1", json!(2))]);
        assert_eq!(dispatch("line", &args).unwrap(), json!("y"));
    }

    #[test]
    fn line_type_error_on_int_input() {
        let args = m(&[("input", json!(5)), ("n", json!(1))]);
        assert!(dispatch("line", &args).is_err());
    }

    #[test]
    fn line_type_error_on_string_n() {
        let args = m(&[("input", json!("a")), ("n", json!("1"))]);
        assert!(dispatch("line", &args).is_err());
    }

    // ---- string.split -----------------------------------------------------

    #[test]
    fn split_basic() {
        let args = m(&[("input", json!("a,b,c")), ("separator", json!(","))]);
        assert_eq!(dispatch("split", &args).unwrap(), json!(["a", "b", "c"]));
    }

    #[test]
    fn split_empty_input() {
        let args = m(&[("input", json!("")), ("separator", json!(","))]);
        assert_eq!(dispatch("split", &args).unwrap(), json!([""]));
    }

    #[test]
    fn split_separator_not_found_returns_input() {
        let args = m(&[("input", json!("hello")), ("separator", json!(","))]);
        assert_eq!(dispatch("split", &args).unwrap(), json!(["hello"]));
    }

    #[test]
    fn split_empty_separator_errors() {
        let args = m(&[("input", json!("abc")), ("separator", json!(""))]);
        assert!(dispatch("split", &args).is_err());
    }

    #[test]
    fn split_type_error() {
        let args = m(&[("input", json!(1)), ("separator", json!(","))]);
        assert!(dispatch("split", &args).is_err());
    }

    // ---- string.join ------------------------------------------------------

    #[test]
    fn join_basic() {
        let args = m(&[("parts", json!(["a", "b", "c"])), ("separator", json!("-"))]);
        assert_eq!(dispatch("join", &args).unwrap(), json!("a-b-c"));
    }

    #[test]
    fn join_empty_array() {
        let args = m(&[("parts", json!([])), ("separator", json!(","))]);
        assert_eq!(dispatch("join", &args).unwrap(), json!(""));
    }

    #[test]
    fn join_type_error_on_non_string_element() {
        let args = m(&[("parts", json!(["a", 2, "c"])), ("separator", json!(","))]);
        assert!(dispatch("join", &args).is_err());
    }

    #[test]
    fn join_type_error_on_non_array() {
        let args = m(&[("parts", json!("a,b")), ("separator", json!(","))]);
        assert!(dispatch("join", &args).is_err());
    }

    // ---- string.lines -----------------------------------------------------

    #[test]
    fn lines_matches_split_newline() {
        let input = json!("one\ntwo\nthree");
        let lines_args = m(&[("input", input.clone())]);
        let split_args = m(&[("input", input), ("separator", json!("\n"))]);
        assert_eq!(
            dispatch("lines", &lines_args).unwrap(),
            dispatch("split", &split_args).unwrap()
        );
    }

    #[test]
    fn lines_empty_input() {
        let args = m(&[("input", json!(""))]);
        assert_eq!(dispatch("lines", &args).unwrap(), json!([""]));
    }

    // ---- string.trim ------------------------------------------------------

    #[test]
    fn trim_happy() {
        let args = m(&[("input", json!("  hi  "))]);
        assert_eq!(dispatch("trim", &args).unwrap(), json!("hi"));
    }

    #[test]
    fn trim_unicode_whitespace() {
        let args = m(&[("input", json!("\u{00A0}hi\u{2003}"))]);
        assert_eq!(dispatch("trim", &args).unwrap(), json!("hi"));
    }

    #[test]
    fn trim_type_error() {
        let args = m(&[("input", json!(5))]);
        assert!(dispatch("trim", &args).is_err());
    }

    // ---- string.lower / upper --------------------------------------------

    #[test]
    fn lower_happy() {
        let args = m(&[("input", json!("ABC"))]);
        assert_eq!(dispatch("lower", &args).unwrap(), json!("abc"));
    }

    #[test]
    fn upper_happy() {
        let args = m(&[("input", json!("abc"))]);
        assert_eq!(dispatch("upper", &args).unwrap(), json!("ABC"));
    }

    #[test]
    fn lower_type_error() {
        let args = m(&[("input", json!(true))]);
        assert!(dispatch("lower", &args).is_err());
    }

    #[test]
    fn upper_type_error() {
        let args = m(&[("input", json!(null))]);
        assert!(dispatch("upper", &args).is_err());
    }

    // ---- string.contains --------------------------------------------------

    #[test]
    fn contains_true() {
        let args = m(&[("input", json!("hello world")), ("needle", json!("world"))]);
        assert_eq!(dispatch("contains", &args).unwrap(), json!(true));
    }

    #[test]
    fn contains_false() {
        let args = m(&[("input", json!("hello")), ("needle", json!("xyz"))]);
        assert_eq!(dispatch("contains", &args).unwrap(), json!(false));
    }

    #[test]
    fn contains_type_error() {
        let args = m(&[("input", json!("hi")), ("needle", json!(5))]);
        assert!(dispatch("contains", &args).is_err());
    }

    // ---- string.replace ---------------------------------------------------

    #[test]
    fn replace_all_occurrences() {
        let args = m(&[
            ("input", json!("aaa")),
            ("from", json!("a")),
            ("to", json!("b")),
        ]);
        assert_eq!(dispatch("replace", &args).unwrap(), json!("bbb"));
    }

    #[test]
    fn replace_no_match_returns_input() {
        let args = m(&[
            ("input", json!("hello")),
            ("from", json!("xyz")),
            ("to", json!("q")),
        ]);
        assert_eq!(dispatch("replace", &args).unwrap(), json!("hello"));
    }

    #[test]
    fn replace_type_error() {
        let args = m(&[
            ("input", json!("hi")),
            ("from", json!(1)),
            ("to", json!("x")),
        ]);
        assert!(dispatch("replace", &args).is_err());
    }

    // ---- dispatch errors --------------------------------------------------

    #[test]
    fn unknown_method_errors() {
        let args = m(&[]);
        assert!(dispatch("nope", &args).is_err());
    }
}
