// SPDX-License-Identifier: MIT
//! Verify tools for the test primitive.
//!
//! Each tool is a deterministic function: input → structured JSON result.
//! Tools are a closed registry — no arbitrary execution.
//!
//! # Tools
//!
//! - `python-syntax`: Check Python code compiles (via subprocess)
//! - `python-exec`: Run Python code and capture output
//! - `line-count`: Compare line counts between inputs
//! - `json-schema`: Validate JSON against a schema
//! - `flesch-kincaid`: Compute Flesch-Kincaid grade level

use crate::scroll::ExecutionError;

/// Dispatch a verify tool by name.
pub fn dispatch(tool: &str, input: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    match tool {
        "python-syntax" => verify_python_syntax(input),
        "python-exec" => verify_python_exec(input),
        "line-count" => verify_line_count(input),
        "json-schema" => verify_json_schema(input),
        "flesch-kincaid" => verify_flesch_kincaid(input),
        _ => Err(ExecutionError::NotImplemented(
            format!("Unknown verify tool: '{}'. Available: python-syntax, python-exec, line-count, json-schema, flesch-kincaid", tool),
        )),
    }
}

/// Check if Python code compiles without syntax errors.
///
/// Input: string (code) or {"code": "..."}.
/// Output: {"passed": true} or {"passed": false, "error": "...", "line": N}
fn verify_python_syntax(input: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    let code = extract_string(input, "code")?;

    let output = std::process::Command::new("python3")
        .arg("-c")
        .arg("import sys; code = sys.stdin.read(); compile(code, '<check>', 'exec')")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(code.as_bytes())?;
            }
            child.wait_with_output()
        })
        .map_err(|e| ExecutionError::InterfaceError(
            format!("Failed to run python3 for syntax check: {}. Is python3 installed?", e),
        ))?;

    if output.status.success() {
        Ok(serde_json::json!({"passed": true}))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Try to extract line number from Python SyntaxError output
        let line = extract_python_error_line(&stderr);
        let mut result = serde_json::json!({
            "passed": false,
            "error": stderr.trim(),
        });
        if let Some(line_num) = line {
            result["line"] = serde_json::json!(line_num);
        }
        Ok(result)
    }
}

/// Extract line number from Python SyntaxError stderr.
/// Looks for patterns like "line 11" in the error output.
fn extract_python_error_line(stderr: &str) -> Option<u32> {
    for segment in stderr.split("line ") {
        if let Some(num_str) = segment.split(|c: char| !c.is_ascii_digit()).next() {
            if let Ok(n) = num_str.parse::<u32>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Run Python code and capture stdout, stderr, and exit code.
///
/// Input: {"code": "...", "stdin": "..." (optional), "timeout": N (optional, default 5, max 30)}.
/// Output: {"passed": bool, "stdout": "...", "stderr": "...", "exit_code": N, "timed_out": bool}
///
/// `passed` = exit_code == 0 AND not timed_out.
fn verify_python_exec(input: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    let code = extract_string(input, "code")?;

    let stdin_text = input.as_object()
        .and_then(|obj| obj.get("stdin"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let timeout_secs: u64 = input.as_object()
        .and_then(|obj| obj.get("timeout"))
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .min(30);

    // Write code to a tempfile so python3 runs it as a script (not -c, which has quoting issues)
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let tmp_dir = std::env::temp_dir();
    let tmp_path = tmp_dir.join(format!(
        "sage_exec_{}_{}.py",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::write(&tmp_path, &code).map_err(|e| ExecutionError::InterfaceError(
        format!("python-exec: failed to write temp file: {}", e),
    ))?;

    let mut child = std::process::Command::new("python3")
        .arg(&tmp_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            ExecutionError::InterfaceError(
                format!("python-exec: failed to spawn python3: {}. Is python3 installed?", e),
            )
        })?;

    // Write stdin and drop to close the pipe
    if let Some(ref mut child_stdin) = child.stdin {
        use std::io::Write;
        let _ = child_stdin.write_all(stdin_text.as_bytes());
    }
    child.stdin.take(); // close stdin

    // Wait with timeout using a channel + thread.
    // Grab the PID before moving child so we can kill on timeout.
    let child_pid = child.id();
    let deadline = std::time::Duration::from_secs(timeout_secs);
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let output = child.wait_with_output();
        let _ = tx.send(output);
    });

    let result = match rx.recv_timeout(deadline) {
        Ok(Ok(output)) => {
            let exit_code = output.status.code().unwrap_or(-1);
            Ok(serde_json::json!({
                "passed": exit_code == 0,
                "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                "exit_code": exit_code,
                "timed_out": false,
            }))
        }
        Ok(Err(e)) => {
            Err(ExecutionError::InterfaceError(
                format!("python-exec: wait failed: {}", e),
            ))
        }
        Err(_) => {
            // Timed out — kill the process by PID
            unsafe { libc::kill(child_pid as i32, libc::SIGKILL); }
            Ok(serde_json::json!({
                "passed": false,
                "stdout": "",
                "stderr": format!("Process timed out after {}s", timeout_secs),
                "exit_code": -1,
                "timed_out": true,
            }))
        }
    };

    let _ = std::fs::remove_file(&tmp_path);
    result
}

/// Compare line counts between one or two inputs.
///
/// Input: {"a": "...", "b": "..."} for comparison, or string/{"text": "..."} for single count.
/// Output: {"a": N, "b": M, "match": bool} or {"count": N}
fn verify_line_count(input: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    if let Some(obj) = input.as_object() {
        if obj.contains_key("a") && obj.contains_key("b") {
            let a = obj.get("a")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ExecutionError::MissingParameter("line-count: 'a' must be a string".into()))?;
            let b = obj.get("b")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ExecutionError::MissingParameter("line-count: 'b' must be a string".into()))?;
            let count_a = a.lines().count();
            let count_b = b.lines().count();
            return Ok(serde_json::json!({
                "a": count_a,
                "b": count_b,
                "match": count_a == count_b,
            }));
        }
    }
    // Single input mode
    let text = extract_string(input, "text")?;
    Ok(serde_json::json!({"count": text.lines().count()}))
}

/// Validate JSON against a JSON Schema.
///
/// Input: {"data": <value>, "schema": <schema>}
/// Output: {"passed": true} or {"passed": false, "errors": [...]}
fn verify_json_schema(input: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    let obj = input.as_object()
        .ok_or_else(|| ExecutionError::MissingParameter("json-schema: input must be an object with 'data' and 'schema'".into()))?;

    let data = obj.get("data")
        .ok_or_else(|| ExecutionError::MissingParameter("json-schema: missing 'data' field".into()))?;
    let schema = obj.get("schema")
        .ok_or_else(|| ExecutionError::MissingParameter("json-schema: missing 'schema' field".into()))?;

    match crate::scroll::step_dispatch::validate_against_schema(data, schema) {
        Ok(()) => Ok(serde_json::json!({"passed": true})),
        Err(e) => Ok(serde_json::json!({
            "passed": false,
            "error": e.to_string(),
        })),
    }
}

/// Compute Flesch-Kincaid grade level via Python textstat.
///
/// Uses the textstat library for research-grade accuracy.
/// Requires: `pip install textstat` in the host environment.
///
/// Input: string or {"text": "..."}.
/// Output: {"grade": f64, "words": N, "sentences": N, "syllables": N}
fn verify_flesch_kincaid(input: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    let text = extract_string(input, "text")?;

    let python_script = r#"
import sys, json, textstat
text = sys.stdin.read()
if not text.strip():
    json.dump({"grade": 0.0, "words": 0, "sentences": 0, "syllables": 0}, sys.stdout)
else:
    grade = textstat.flesch_kincaid_grade(text)
    words = textstat.lexicon_count(text, removepunct=True)
    sentences = textstat.sentence_count(text)
    syllables = textstat.syllable_count(text)
    json.dump({"grade": round(grade, 1), "words": words, "sentences": sentences, "syllables": syllables}, sys.stdout)
"#;

    let output = std::process::Command::new("python3")
        .arg("-c")
        .arg(python_script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait_with_output()
        })
        .map_err(|e| ExecutionError::InterfaceError(
            format!("Failed to run python3 for flesch-kincaid: {}. Is python3 installed?", e),
        ))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ExecutionError::InterfaceError(
            format!("flesch-kincaid failed: {}. Is textstat installed? (pip install textstat)", stderr.trim()),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).map_err(|e| ExecutionError::InterfaceError(
        format!("flesch-kincaid: failed to parse output: {}", e),
    ))
}

/// Extract a string from input — handles both plain strings and {"field": "..."} objects.
fn extract_string(input: &serde_json::Value, field: &str) -> Result<String, ExecutionError> {
    if let Some(s) = input.as_str() {
        return Ok(s.to_string());
    }
    if let Some(obj) = input.as_object() {
        if let Some(val) = obj.get(field) {
            if let Some(s) = val.as_str() {
                return Ok(s.to_string());
            }
        }
        // Also try "code" for backwards compat
        if field != "code" {
            if let Some(val) = obj.get("code") {
                if let Some(s) = val.as_str() {
                    return Ok(s.to_string());
                }
            }
        }
    }
    Err(ExecutionError::MissingParameter(
        format!("verify tool: input must be a string or object with '{}' field", field),
    ))
}


#[cfg(test)]
mod tests {
    use super::*;

    // ---- python-syntax ----

    #[test]
    fn test_python_syntax_valid() {
        let input = serde_json::json!("x = 1\nprint(x)");
        let result = verify_python_syntax(&input).unwrap();
        assert_eq!(result["passed"], true);
    }

    #[test]
    fn test_python_syntax_invalid() {
        let input = serde_json::json!("def foo(\n  x = ");
        let result = verify_python_syntax(&input).unwrap();
        assert_eq!(result["passed"], false);
        assert!(result["error"].as_str().unwrap().contains("SyntaxError"));
    }

    #[test]
    fn test_python_syntax_object_input() {
        let input = serde_json::json!({"code": "import os"});
        let result = verify_python_syntax(&input).unwrap();
        assert_eq!(result["passed"], true);
    }

    // ---- line-count ----

    #[test]
    fn test_line_count_comparison_match() {
        let input = serde_json::json!({"a": "line1\nline2\nline3", "b": "a\nb\nc"});
        let result = verify_line_count(&input).unwrap();
        assert_eq!(result["a"], 3);
        assert_eq!(result["b"], 3);
        assert_eq!(result["match"], true);
    }

    #[test]
    fn test_line_count_comparison_mismatch() {
        let input = serde_json::json!({"a": "line1\nline2", "b": "a\nb\nc"});
        let result = verify_line_count(&input).unwrap();
        assert_eq!(result["a"], 2);
        assert_eq!(result["b"], 3);
        assert_eq!(result["match"], false);
    }

    #[test]
    fn test_line_count_single() {
        let input = serde_json::json!("one\ntwo\nthree\nfour");
        let result = verify_line_count(&input).unwrap();
        assert_eq!(result["count"], 4);
    }

    // ---- json-schema ----
    // (json-schema tests depend on validate_against_schema visibility,
    //  tested indirectly through integration tests)

    // ---- flesch-kincaid (via Python textstat) ----
    // These tests require `textstat` Python package. They skip if not installed.

    fn has_textstat() -> bool {
        std::process::Command::new("python3")
            .args(["-c", "import textstat"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn test_flesch_kincaid_simple_text() {
        if !has_textstat() { eprintln!("SKIP: textstat not installed"); return; }
        let input = serde_json::json!("The cat sat on the mat.");
        let result = verify_flesch_kincaid(&input).unwrap();
        assert!(result["words"].as_u64().unwrap() > 0);
        assert_eq!(result["sentences"], 1);
        let grade = result["grade"].as_f64().unwrap();
        assert!(grade < 5.0, "Simple sentence should be low grade, got {}", grade);
    }

    #[test]
    fn test_flesch_kincaid_empty_text() {
        if !has_textstat() { eprintln!("SKIP: textstat not installed"); return; }
        let input = serde_json::json!("");
        let result = verify_flesch_kincaid(&input).unwrap();
        assert_eq!(result["words"], 0);
        assert_eq!(result["grade"], 0.0);
    }

    #[test]
    fn test_flesch_kincaid_object_input() {
        if !has_textstat() { eprintln!("SKIP: textstat not installed"); return; }
        let input = serde_json::json!({"text": "Dogs are nice. Cats are nice."});
        let result = verify_flesch_kincaid(&input).unwrap();
        assert_eq!(result["sentences"], 2);
        assert!(result["grade"].as_f64().is_some());
    }

    // ---- python-exec ----

    #[test]
    fn test_python_exec_simple() {
        let input = serde_json::json!({"code": "print('hello')"});
        let result = verify_python_exec(&input).unwrap();
        assert_eq!(result["passed"], true);
        assert_eq!(result["stdout"], "hello\n");
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["timed_out"], false);
    }

    #[test]
    fn test_python_exec_with_stdin() {
        let input = serde_json::json!({
            "code": "x = int(input())\nprint(x * 2)",
            "stdin": "21"
        });
        let result = verify_python_exec(&input).unwrap();
        assert_eq!(result["passed"], true);
        assert_eq!(result["stdout"], "42\n");
    }

    #[test]
    fn test_python_exec_multiline_stdin() {
        let input = serde_json::json!({
            "code": "total = 0\nfor _ in range(3):\n    total += int(input())\nprint(total)",
            "stdin": "10\n20\n30"
        });
        let result = verify_python_exec(&input).unwrap();
        assert_eq!(result["passed"], true);
        assert_eq!(result["stdout"], "60\n");
    }

    #[test]
    fn test_python_exec_runtime_error() {
        let input = serde_json::json!({"code": "x = 1 / 0"});
        let result = verify_python_exec(&input).unwrap();
        assert_eq!(result["passed"], false);
        assert_eq!(result["timed_out"], false);
        assert!(result["stderr"].as_str().unwrap().contains("ZeroDivisionError"));
        assert_ne!(result["exit_code"], 0);
    }

    #[test]
    fn test_python_exec_timeout() {
        let input = serde_json::json!({
            "code": "import time\ntime.sleep(60)",
            "timeout": 1
        });
        let result = verify_python_exec(&input).unwrap();
        assert_eq!(result["passed"], false);
        assert_eq!(result["timed_out"], true);
    }

    #[test]
    fn test_python_exec_stderr_capture() {
        let input = serde_json::json!({"code": "import sys\nprint('err', file=sys.stderr)\nprint('out')"});
        let result = verify_python_exec(&input).unwrap();
        assert_eq!(result["passed"], true);
        assert_eq!(result["stdout"], "out\n");
        assert!(result["stderr"].as_str().unwrap().contains("err"));
    }

    #[test]
    fn test_python_exec_float_precision() {
        // The IEEE 754 case from the issue
        let input = serde_json::json!({"code": "print(3.99 + 1.50 + 7.25 + 2.00 + 4.75)"});
        let result = verify_python_exec(&input).unwrap();
        assert_eq!(result["passed"], true);
        let stdout = result["stdout"].as_str().unwrap().trim();
        // Python will print 19.49 or 19.490000000000002 — this tool lets the scroll check which
        assert!(stdout.starts_with("19.49"));
    }

    // ---- extract_python_error_line ----

    #[test]
    fn test_extract_python_error_line() {
        let stderr = "  File \"<check>\", line 11\n    x = \n        ^\nSyntaxError: invalid syntax";
        assert_eq!(extract_python_error_line(stderr), Some(11));
    }

    #[test]
    fn test_extract_python_error_line_none() {
        assert_eq!(extract_python_error_line("some other error"), None);
    }
}
