// SPDX-License-Identifier: MIT
//! Step dispatch adapter: executes AST nodes using existing primitives.
//!
//! Bridges the Scroll Assembly AST to the existing YAML-based executor.
//! For primitive calls (platform, fs, invoke, etc.), builds the corresponding
//! Step variant and delegates to executor.execute_step().
//! For control flow and expressions, handles execution directly.

use super::ast::*;
use crate::scroll::context::ExecutionContext;
use crate::scroll::error::ExecutionError;
use crate::scroll::executor::Executor;
use crate::scroll::schema::Step;
use serde_json::{json, Value};
use std::collections::HashMap;

// ============================================================================
// Public API
// ============================================================================

/// Execute a parsed and type-checked scroll file.
pub async fn execute(
    ast: &ScrollFile,
    executor: &mut Executor,
    inputs: HashMap<String, Value>,
) -> Result<HashMap<String, Value>, ExecutionError> {
    // Set require variables from inputs
    for req in &ast.scroll.requires {
        if let Some(val) = inputs.get(&req.name) {
            executor.context.set_variable(req.name.clone(), val.clone());
        } else if let Some(default) = &req.default {
            let val = eval_expr(default, &executor.context)?;
            executor.context.set_variable(req.name.clone(), val);
        } else {
            return Err(ExecutionError::MissingVariable(format!(
                "required variable '{}' not provided", req.name
            )));
        }
    }

    // Debug: log require variable values at scroll entry
    for req in &ast.scroll.requires {
        let val = executor.context.get_variable(&req.name);
        tracing::debug!(scroll = %ast.scroll.name, var = %req.name, value = ?val, "Scroll require value");
    }

    // Execute body
    exec_block_body(&ast.scroll.body, executor).await?;

    // Collect provide variables
    let mut outputs = HashMap::new();
    for prov in &ast.scroll.provides {
        if let Some(val) = executor.context.get_variable(&prov.name) {
            outputs.insert(prov.name.clone(), val.clone());
        } else {
            return Err(ExecutionError::MissingVariable(format!(
                "provide variable '{}' not set after execution", prov.name
            )));
        }
    }

    Ok(outputs)
}

// ============================================================================
// Block Body Execution
// ============================================================================

fn exec_block_body<'a>(
    body: &'a BlockBody,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Value>, ExecutionError>> + Send + 'a>> {
    Box::pin(async move {
        for stmt in &body.statements {
            exec_statement(stmt, executor).await?;
        }
        if let Some(tail) = &body.tail_expr {
            let val = eval_expr_async(tail, executor).await?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    })
}

// ============================================================================
// Statement Execution
// ============================================================================

fn exec_statement<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ExecutionError>> + Send + 'a>> {
    Box::pin(async move {
        match stmt {
            Statement::SetDecl(sd) => {
                let val = eval_expr_async(&sd.value, executor).await?;
                executor.context.set_variable(sd.name.clone(), val);
            }
            Statement::Binding(b) => {
                exec_binding(b, executor).await?;
            }
            Statement::Assignment(a) => {
                let val = eval_expr_async(&a.value, executor).await?;
                match a.op {
                    AssignOp::Assign => {
                        executor.context.set_variable(a.target.clone(), val);
                    }
                    AssignOp::AddAssign => {
                        let current = executor.context.get_variable(&a.target)
                            .cloned().unwrap_or(Value::Null);
                        let result = numeric_op(&current, &val, |a, b| a + b)?;
                        executor.context.set_variable(a.target.clone(), result);
                    }
                    AssignOp::SubAssign => {
                        let current = executor.context.get_variable(&a.target)
                            .cloned().unwrap_or(Value::Null);
                        let result = numeric_op(&current, &val, |a, b| a - b)?;
                        executor.context.set_variable(a.target.clone(), result);
                    }
                    AssignOp::AppendAssign => {
                        let current = executor.context.get_variable(&a.target)
                            .cloned().unwrap_or(json!([]));
                        let result = array_append(&current, &val)?;
                        executor.context.set_variable(a.target.clone(), result);
                    }
                }
            }
            Statement::Break(_) => {
                return Err(ExecutionError::LoopBreak);
            }
            Statement::BlockExpr(expr) | Statement::ExprStmt(expr) => {
                eval_expr_async(expr, executor).await?;
            }
        }
        Ok(())
    })
}

// ============================================================================
// Binding Execution
// ============================================================================

async fn exec_binding(
    binding: &BindingStmt,
    executor: &mut Executor,
) -> Result<(), ExecutionError> {
    let result = exec_call_for_binding(&binding.source, executor).await;
    let value = apply_error_chain(result, &binding.error_chain, executor).await?;
    executor.context.set_variable(binding.name.clone(), value);
    Ok(())
}

fn apply_error_chain<'a>(
    result: Result<Value, ExecutionError>,
    chain: &'a [ErrorHandler],
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutionError>> + Send + 'a>> {
    Box::pin(async move {
    match result {
        Ok(val) => Ok(val),
        Err(e) => {
            if chain.is_empty() {
                return Err(e);
            }
            // Process chain left-to-right
            let current_err = e;
            if let Some(handler) = chain.iter().next() {
                match handler {
                    ErrorHandler::Continue => {
                        crate::scroll::executor::write_failure_diagnostic(
                            &ExecutionError::InvalidStep(format!("continue: {current_err}"))
                        );
                        return Ok(Value::Null);
                    }
                    ErrorHandler::Retry(n) => {
                        // Already failed once, retry n more times
                        // For v1, retry is a simplified re-execution concept
                        // The actual retry logic is handled by the existing executor's on_fail
                        let _ = n;
                        return Err(current_err);
                    }
                    ErrorHandler::Fallback(body) => {
                        let val = exec_block_body(body, executor).await?;
                        return Ok(val.unwrap_or(Value::Null));
                    }
                }
            }
            Err(current_err)
        }
    }
    })
}

/// Execute an expression that's the source of a binding (-> var: Type).
/// This typically dispatches a primitive call through the executor.
async fn exec_call_for_binding(
    expr: &Expr,
    executor: &mut Executor,
) -> Result<Value, ExecutionError> {
    match &expr.kind {
        ExprKind::Call { target, args, config } => {
            dispatch_call(target, args, config.as_deref(), executor).await
        }
        ExprKind::FieldAccess { .. } => {
            // Could be a method-less field access — evaluate as expression
            eval_expr(expr, &executor.context)
        }
        _ => eval_expr(expr, &executor.context),
    }
}

// ============================================================================
// Primitive Call Dispatch
// ============================================================================

/// Dispatch a function call to the existing executor.
/// Builds the appropriate Step variant from the AST call data.
async fn dispatch_call(
    target: &Expr,
    args: &[CallArg],
    config: Option<&[ConfigField]>,
    executor: &mut Executor,
) -> Result<Value, ExecutionError> {
    let (namespace, method) = extract_call_target(target)?;
    let arg_map = build_arg_map(args, &executor.context)?;
    let config_map = build_config_map(config, &executor.context)?;

    // For system primitives (platform, fs, vcs, test), call the interface
    // dispatch directly — avoids the Step schema string/typed value mismatch.
    if matches!(namespace.as_str(), "platform" | "fs" | "vcs" | "test") {
        use crate::scroll::interfaces::InterfaceDispatch;
        let mut params_map = serde_json::Map::new();
        for (k, v) in &arg_map {
            if !k.starts_with("__pos_") {
                params_map.insert(k.clone(), v.clone());
            }
        }
        let params = Some(Value::Object(params_map));
        tracing::debug!(namespace = %namespace, method = %method, params = ?params, "Direct interface dispatch");
        // Validate required params aren't null before dispatching
        if method == "write" {
            if let Some(ref p) = params {
                if let Some(content) = p.get("content") {
                    if content.is_null() {
                        tracing::warn!(namespace = %namespace, method = %method, "fs.write content is null — check data flow");
                    }
                } else {
                    tracing::warn!(namespace = %namespace, method = %method, "fs.write missing content param");
                }
            }
        }
        let result = match namespace.as_str() {
            "platform" => executor.interface_registry.platform.dispatch(&method, &params).await?,
            "fs" => executor.interface_registry.fs.dispatch(&method, &params).await?,
            "vcs" => executor.interface_registry.vcs.dispatch(&method, &params).await?,
            "test" => executor.interface_registry.test.dispatch(&method, &params).await?,
            _ => unreachable!(),
        };
        return Ok(result);
    }

    // For steps that use the YAML executor (invoke, consensus, aggregate, etc.),
    // store resolved values as temporary context variables and pass ${var} refs.
    // This bridges the Assembly dispatch (values already resolved) with the
    // YAML executor (resolves ${var} references at execution time).
    let mut temp_vars: Vec<String> = Vec::new();
    let mut patched_args = arg_map.clone();
    let mut patched_config = config_map.clone();

    // Patch context arrays (invoke, consensus)
    for map in [&mut patched_config, &mut patched_args] {
        if let Some(Value::Array(items)) = map.get("context").cloned() {
            let mut refs = Vec::new();
            for item in items.iter() {
                let var_name = format!("__tmp_{}", temp_vars.len());
                executor.context.set_variable(var_name.clone(), item.clone());
                refs.push(Value::String(format!("${{{var_name}}}")));
                temp_vars.push(var_name);
            }
            map.insert("context".to_string(), Value::Array(refs));
        }
        // Patch results arrays (aggregate)
        if let Some(Value::Array(items)) = map.get("results").cloned() {
            let mut refs = Vec::new();
            for item in items.iter() {
                let var_name = format!("__tmp_{}", temp_vars.len());
                executor.context.set_variable(var_name.clone(), item.clone());
                refs.push(Value::String(format!("${{{var_name}}}")));
                temp_vars.push(var_name);
            }
            map.insert("results".to_string(), Value::Array(refs));
        }
        // Patch input/reference fields (validate, elaborate, distill, convert)
        for field in ["input", "reference", "proposal"] {
            if let Some(val) = map.get(field).cloned() {
                if !val.is_string() || !val.as_str().unwrap_or("").starts_with("${") {
                    let var_name = format!("__tmp_{}", temp_vars.len());
                    executor.context.set_variable(var_name.clone(), val);
                    map.insert(field.to_string(), Value::String(format!("${{{var_name}}}")));
                    temp_vars.push(var_name);
                }
            }
        }
    }

    // For other primitives, build Step JSON and dispatch through executor
    let step_json = build_step_json(&namespace, &method, &patched_args, &patched_config, &temp_vars)?;
    tracing::debug!(namespace = %namespace, method = %method, json = %step_json, "Dispatching step");
    let step: Step = serde_json::from_value(step_json.clone())
        .map_err(|e| ExecutionError::InvalidStep(format!("Failed to build step from {step_json}: {e}")))?;

    // Temporary output name for capturing the result
    let output_name = "__dispatch_result__";

    // Inject output name into the step if possible
    let step_with_output = inject_output_name(step, output_name);

    executor.execute_step(&step_with_output).await?;

    // Retrieve the result
    let result = executor.context.get_variable(output_name)
        .cloned()
        .or_else(|| executor.context.prev().cloned())
        .unwrap_or(Value::Null);

    executor.context.clear_variable(output_name);

    // Clean up temp variables to prevent stale data leaking across calls
    for var_name in &temp_vars {
        executor.context.clear_variable(var_name);
    }

    Ok(result)
}

fn extract_call_target(target: &Expr) -> Result<(String, String), ExecutionError> {
    match &target.kind {
        ExprKind::FieldAccess { object, field } => {
            if let ExprKind::Identifier(ns) = &object.kind {
                Ok((ns.clone(), field.clone()))
            } else {
                Err(ExecutionError::InvalidStep("nested field access not supported for dispatch".into()))
            }
        }
        ExprKind::Identifier(name) => {
            // Top-level function: invoke, parallel, consensus, run, elaborate, etc.
            Ok((name.clone(), String::new()))
        }
        _ => Err(ExecutionError::InvalidStep("invalid call target".into())),
    }
}

fn build_arg_map(args: &[CallArg], ctx: &ExecutionContext) -> Result<HashMap<String, Value>, ExecutionError> {
    let mut map = HashMap::new();
    for (i, arg) in args.iter().enumerate() {
        match arg {
            CallArg::Named { name, value } => {
                map.insert(name.clone(), eval_expr(value, ctx)?);
            }
            CallArg::Positional(value) => {
                map.insert(format!("__pos_{i}"), eval_expr(value, ctx)?);
            }
        }
    }
    Ok(map)
}

fn build_config_map(config: Option<&[ConfigField]>, ctx: &ExecutionContext) -> Result<HashMap<String, Value>, ExecutionError> {
    let mut map = HashMap::new();
    if let Some(fields) = config {
        for field in fields {
            map.insert(field.name.clone(), eval_expr(&field.value, ctx)?);
        }
    }
    Ok(map)
}

fn build_step_json(
    namespace: &str,
    method: &str,
    args: &HashMap<String, Value>,
    config: &HashMap<String, Value>,
    _temp_ctx_vars: &[String],
) -> Result<Value, ExecutionError> {
    match namespace {
        "platform" => {
            let mut params = json!({ "operation": method });
            for (k, v) in args {
                if !k.starts_with("__pos_") {
                    params[k] = value_to_string_value(v);
                }
            }
            Ok(json!({ "platform": params }))
        }
        "fs" => {
            let mut params = json!({ "operation": method });
            for (k, v) in args {
                if !k.starts_with("__pos_") {
                    params[k] = value_to_string_value(v);
                }
            }
            Ok(json!({ "fs": params }))
        }
        "vcs" => {
            let mut params = json!({ "operation": method });
            for (k, v) in args {
                if !k.starts_with("__pos_") {
                    params[k] = value_to_string_value(v);
                }
            }
            Ok(json!({ "vcs": params }))
        }
        "test" => {
            let mut params = json!({ "operation": method });
            for (k, v) in args {
                if !k.starts_with("__pos_") {
                    params[k] = value_to_string_value(v);
                }
            }
            Ok(json!({ "test": params }))
        }
        "invoke" => {
            let mut params = json!({});
            for (k, v) in args {
                if !k.starts_with("__pos_") {
                    params[k] = v.clone();
                }
            }
            // Merge config (schema, tier, timeout, etc.)
            for (k, v) in config {
                match k.as_str() {
                    "schema" => {
                        // Convert type name string to JSON schema object.
                        // The invoke primitive expects output_schema as a JSON Schema.
                        if let Value::String(type_name) = v {
                            params["output_schema"] = type_name_to_json_schema(type_name);
                        } else {
                            params["output_schema"] = v.clone();
                        }
                    }
                    "tier" => { params["model_tier"] = v.clone(); }
                    "timeout" => { params["timeout_secs"] = v.clone(); }
                    _ => { params[k] = v.clone(); }
                }
            }
            // Context is already patched by dispatch_call with ${__tmp_N} refs.
            Ok(json!({ "invoke": params }))
        }
        "parallel" => {
            let mut params = json!({});
            for (k, v) in args {
                if !k.starts_with("__pos_") {
                    params[k] = v.clone();
                }
            }
            for (k, v) in config {
                params[k] = v.clone();
            }
            Ok(json!({ "parallel": params }))
        }
        "consensus" => {
            let mut params = json!({});
            for (k, v) in args {
                if !k.starts_with("__pos_") {
                    params[k] = v.clone();
                }
            }
            for (k, v) in config {
                params[k] = v.clone();
            }
            Ok(json!({ "consensus": params }))
        }
        "run" => {
            // run("scroll-path") { named_args } OR run(scroll_path: "path", args: { ... })
            let scroll_path = args.get("__pos_0")
                .or_else(|| args.get("scroll_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut inputs = json!({});
            // Config fields are the inputs
            for (k, v) in config {
                if k != "scroll_path" {
                    inputs[k] = v.clone();
                }
            }
            // Also check for an "args" map in args
            if let Some(Value::Object(args_map)) = args.get("args") {
                for (k, v) in args_map {
                    inputs[k] = v.clone();
                }
            }
            // Also check for an "args" map in config
            if let Some(Value::Object(args_map)) = config.get("args") {
                for (k, v) in args_map {
                    inputs[k] = v.clone();
                }
            }
            Ok(json!({ "run": { "scroll_path": scroll_path, "args": inputs } }))
        }
        "elaborate" => {
            let mut params = json!({});
            for (k, v) in args { params[k] = v.clone(); }
            Ok(json!({ "elaborate": params }))
        }
        "distill" => {
            let mut params = json!({});
            for (k, v) in args { params[k] = v.clone(); }
            Ok(json!({ "distill": params }))
        }
        "validate" => {
            let mut params = json!({});
            for (k, v) in args { params[k] = v.clone(); }
            for (k, v) in config { params[k] = v.clone(); }
            Ok(json!({ "validate": params }))
        }
        "convert" => {
            let mut params = json!({});
            for (k, v) in args { params[k] = v.clone(); }
            for (k, v) in config {
                if k != "schema" { params[k] = v.clone(); }
            }
            // If schema is provided, wrap "to" as { format: to_value, schema: schema_value }
            if let Some(schema) = config.get("schema").or_else(|| args.get("schema")) {
                let format_str = params.get("to").and_then(|v| v.as_str()).unwrap_or("yaml").to_string();
                params["to"] = json!({ "format": format_str, "schema": schema });
                params.as_object_mut().map(|m| m.remove("schema"));
            }
            Ok(json!({ "convert": params }))
        }
        "aggregate" => {
            let mut params = json!({});
            for (k, v) in args { params[k] = v.clone(); }
            Ok(json!({ "aggregate": params }))
        }
        _ => Err(ExecutionError::InvalidStep(format!("unknown namespace: '{namespace}'"))),
    }
}

/// Inject an output name into a Step variant.
fn inject_output_name(step: Step, name: &str) -> Step {
    let mut json = serde_json::to_value(&step).unwrap_or(Value::Null);
    if let Value::Object(ref mut map) = json {
        map.insert("output".to_string(), Value::String(name.to_string()));
    }
    serde_json::from_value(json).unwrap_or(step)
}

/// Convert a type name to a JSON Schema object for the invoke primitive's output_schema.
/// Generates a permissive schema that accepts any properties with the given type name.
fn type_name_to_json_schema(type_name: &str) -> Value {
    // Generate a permissive object schema — the invoke primitive will
    // extract structured data from LLM output and validate against this.
    json!({
        "type": "object",
        "title": type_name
    })
}

/// Convert a value to a string for interpolation in YAML step fields.
fn value_to_string_value(v: &Value) -> Value {
    match v {
        Value::String(_) => v.clone(),
        _ => Value::String(v.to_string()),
    }
}

// ============================================================================
// Expression Evaluation (async — for expressions containing calls)
// ============================================================================

/// Evaluate an expression that may contain async calls (invoke, platform, etc.)
fn eval_expr_async<'a>(
    expr: &'a Expr,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutionError>> + Send + 'a>> {
    Box::pin(async move {
    match &expr.kind {
        ExprKind::Call { target, args, config } => {
            dispatch_call(target, args, config.as_deref(), executor).await
        }
        ExprKind::If { condition, then_body, else_body } => {
            let cond = eval_expr(condition, &executor.context)?;
            if is_truthy(&cond) {
                let val = exec_block_body(then_body, executor).await?;
                Ok(val.unwrap_or(Value::Null))
            } else if let Some(else_clause) = else_body {
                match else_clause {
                    ElseClause::ElseBlock(body) => {
                        let val = exec_block_body(body, executor).await?;
                        Ok(val.unwrap_or(Value::Null))
                    }
                    ElseClause::ElseIf(if_expr) => {
                        eval_expr_async(if_expr, executor).await
                    }
                }
            } else {
                Ok(Value::Null)
            }
        }
        ExprKind::Match { target, arms } => {
            let target_val = eval_expr(target, &executor.context)?;
            for arm in arms {
                let pattern_val = eval_expr(&arm.pattern, &executor.context)?;
                if values_match(&target_val, &pattern_val) {
                    return match &arm.body {
                        MatchArmBody::Expr(e) => eval_expr_async(e, executor).await,
                        MatchArmBody::Block(body) => {
                            let val = exec_block_body(body, executor).await?;
                            Ok(val.unwrap_or(Value::Null))
                        }
                    };
                }
            }
            Ok(Value::Null) // No arm matched
        }
        ExprKind::For { binding, iterable, body } => {
            let iter_val = eval_expr(iterable, &executor.context)?;
            tracing::debug!(binding = %binding, iterable_type = ?iter_val.is_array(), iterable_len = ?iter_val.as_array().map(|a| a.len()), "For loop starting");
            let items = iter_val.as_array()
                .ok_or_else(|| {
                    tracing::error!(binding = %binding, iterable = ?iter_val, "for: iterable is not an array");
                    ExecutionError::InvalidStep(format!("for: iterable is not an array (got {:?})",
                        match &iter_val { Value::Null => "null", Value::String(_) => "string", Value::Number(_) => "number", Value::Bool(_) => "bool", Value::Object(_) => "object", Value::Array(_) => "array" }))
                })?;
            let mut results = Vec::new();
            for item in items {
                executor.context.set_variable(binding.clone(), item.clone());
                match exec_block_body(body, executor).await {
                    Ok(Some(val)) => results.push(val),
                    Ok(None) => results.push(Value::Null),
                    Err(ExecutionError::LoopBreak) => break,
                    Err(e) => return Err(e),
                }
            }
            executor.context.clear_variable(binding);
            Ok(Value::Array(results))
        }
        ExprKind::While { condition, body } => {
            loop {
                let cond = eval_expr(condition, &executor.context)?;
                if !is_truthy(&cond) { break; }
                match exec_block_body(body, executor).await {
                    Ok(_) => {}
                    Err(ExecutionError::LoopBreak) => break,
                    Err(e) => return Err(e),
                }
            }
            Ok(Value::Null)
        }
        ExprKind::ConcurrentBlock { body } => {
            // For v1, execute sequentially (concurrent dispatch is S5 territory)
            exec_block_body(body, executor).await?;
            Ok(Value::Null)
        }
        ExprKind::ConcurrentFor { binding, iterable, body } => {
            // For v1, execute as regular for loop
            let iter_val = eval_expr(iterable, &executor.context)?;
            let items = iter_val.as_array()
                .ok_or_else(|| ExecutionError::InvalidStep("concurrent for: iterable is not an array".into()))?;
            let mut results = Vec::new();
            for item in items {
                executor.context.set_variable(binding.clone(), item.clone());
                match exec_block_body(body, executor).await {
                    Ok(Some(val)) => results.push(val),
                    Ok(None) => results.push(Value::Null),
                    Err(e) => return Err(e),
                }
            }
            executor.context.clear_variable(binding);
            Ok(Value::Array(results))
        }
        // For non-async expressions, delegate to sync eval
        _ => eval_expr(expr, &executor.context),
    }
    })
}

// ============================================================================
// Expression Evaluation (sync — pure computation, no I/O)
// ============================================================================

/// Evaluate a pure expression (no I/O calls).
fn eval_expr(expr: &Expr, ctx: &ExecutionContext) -> Result<Value, ExecutionError> {
    match &expr.kind {
        ExprKind::IntLit(n) => Ok(json!(*n)),
        ExprKind::FloatLit(n) => Ok(json!(*n)),
        ExprKind::BoolLit(b) => Ok(json!(*b)),
        ExprKind::NullLit => Ok(Value::Null),
        ExprKind::RawStringLit(s) => Ok(json!(s)),

        ExprKind::StringLit(segments) => {
            let mut result = String::new();
            for seg in segments {
                match seg {
                    StringSegment::Literal(s) => result.push_str(s),
                    StringSegment::Escape(c) => result.push(*c),
                    StringSegment::Interpolation(expr) => {
                        let val = eval_expr(expr, ctx)?;
                        match val {
                            Value::String(s) => result.push_str(&s),
                            Value::Null => result.push_str("null"),
                            other => result.push_str(&other.to_string()),
                        }
                    }
                }
            }
            Ok(Value::String(result))
        }

        ExprKind::Identifier(name) => {
            // Look up variable first; if not found, treat as a bare identifier
            // (enum value, type name, or runtime constant like "premium", "majority").
            match ctx.get_variable(name) {
                Some(val) => Ok(val.clone()),
                None => {
                    tracing::trace!(var = %name, "Variable not found, falling back to string literal");
                    Ok(Value::String(name.clone()))
                }
            }
        }

        ExprKind::FieldAccess { object, field } => {
            let obj_val = eval_expr(object, ctx)?;
            match &obj_val {
                Value::Object(map) => {
                    Ok(map.get(field).cloned().unwrap_or(Value::Null))
                }
                _ => Ok(Value::Null),
            }
        }

        ExprKind::ArrayLit(elements) => {
            let vals: Result<Vec<_>, _> = elements.iter().map(|e| eval_expr(e, ctx)).collect();
            Ok(Value::Array(vals?))
        }

        ExprKind::StructLit { fields, .. } | ExprKind::MapLit(fields) => {
            let mut map = serde_json::Map::new();
            for field in fields {
                let val = eval_expr(&field.value, ctx)?;
                map.insert(field.name.clone(), val);
            }
            Ok(Value::Object(map))
        }

        ExprKind::BinaryOp { left, op, right } => {
            let l = eval_expr(left, ctx)?;
            let r = eval_expr(right, ctx)?;
            eval_binary_op(&l, *op, &r)
        }

        ExprKind::UnaryOp { op: UnaryOp::Not, operand } => {
            let val = eval_expr(operand, ctx)?;
            Ok(json!(!is_truthy(&val)))
        }

        ExprKind::Ternary { condition, true_val, false_val } => {
            let cond = eval_expr(condition, ctx)?;
            if is_truthy(&cond) {
                eval_expr(true_val, ctx)
            } else {
                eval_expr(false_val, ctx)
            }
        }

        ExprKind::NullCoalesce { left, right } => {
            let l = eval_expr(left, ctx)?;
            if l.is_null() {
                eval_expr(right, ctx)
            } else {
                Ok(l)
            }
        }

        // Block expressions in sync context — shouldn't happen (handled by async path)
        ExprKind::Call { .. }
        | ExprKind::If { .. }
        | ExprKind::Match { .. }
        | ExprKind::For { .. }
        | ExprKind::While { .. }
        | ExprKind::ConcurrentBlock { .. }
        | ExprKind::ConcurrentFor { .. } => {
            Err(ExecutionError::InvalidStep("async expression in sync context".into()))
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn eval_binary_op(left: &Value, op: BinOp, right: &Value) -> Result<Value, ExecutionError> {
    match op {
        BinOp::Add | BinOp::MapMerge => {
            // Numeric add or map merge
            if left.is_object() && right.is_object() {
                let mut result = left.as_object().unwrap().clone();
                for (k, v) in right.as_object().unwrap() {
                    result.insert(k.clone(), v.clone());
                }
                Ok(Value::Object(result))
            } else {
                numeric_op(left, right, |a, b| a + b)
            }
        }
        BinOp::Sub => numeric_op(left, right, |a, b| a - b),
        BinOp::Concat => {
            let mut result = left.as_array().cloned().unwrap_or_default();
            result.extend(right.as_array().cloned().unwrap_or_default());
            Ok(Value::Array(result))
        }
        BinOp::Eq => Ok(json!(left == right)),
        BinOp::NotEq => Ok(json!(left != right)),
        BinOp::Gt => Ok(json!(compare_values(left, right) == Some(std::cmp::Ordering::Greater))),
        BinOp::Lt => Ok(json!(compare_values(left, right) == Some(std::cmp::Ordering::Less))),
        BinOp::GtEq => Ok(json!(matches!(compare_values(left, right), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)))),
        BinOp::LtEq => Ok(json!(matches!(compare_values(left, right), Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)))),
        BinOp::And => Ok(json!(is_truthy(left) && is_truthy(right))),
        BinOp::Or => Ok(json!(is_truthy(left) || is_truthy(right))),
    }
}

fn numeric_op(left: &Value, right: &Value, op: fn(f64, f64) -> f64) -> Result<Value, ExecutionError> {
    let l = to_number(left)?;
    let r = to_number(right)?;
    let result = op(l, r);
    // Return int if both inputs were ints and result is whole
    if left.is_i64() && right.is_i64() && result.fract() == 0.0 {
        Ok(json!(result as i64))
    } else {
        Ok(json!(result))
    }
}

fn to_number(v: &Value) -> Result<f64, ExecutionError> {
    match v {
        Value::Number(n) => n.as_f64().ok_or(ExecutionError::InvalidStep("non-finite number".into())),
        _ => Err(ExecutionError::InvalidStep(format!("expected number, got {v}"))),
    }
}

fn array_append(arr: &Value, item: &Value) -> Result<Value, ExecutionError> {
    let mut result = arr.as_array().cloned().unwrap_or_default();
    result.push(item.clone());
    Ok(Value::Array(result))
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

fn compare_values(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().and_then(|a| b.as_f64().map(|b| a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)))
        }
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

fn values_match(target: &Value, pattern: &Value) -> bool {
    target == pattern
}
