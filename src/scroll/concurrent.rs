// SPDX-License-Identifier: MIT
//! Concurrent execution helpers for the concurrent step.
//!
//! This module provides the three execution strategies for concurrent operations:
//! - halt: Stop on first error
//! - continue: Continue on error, replace failures with null
//! - collect_errors: Continue on error, return both results and errors
//!
//! Fixed in #168: operations now inherit the parent executor's security policy
//! and token limits. Token usage is aggregated back to the parent. Only declared
//! output variables are merged (not the entire child context).

use crate::scroll::error::ExecutionError;
use crate::scroll::schema::Step;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::task::JoinSet;

/// Result from a single concurrent operation: (result, output_value, output_name, tokens_used)
type OpResult = (Result<(), ExecutionError>, serde_json::Value, Option<String>, usize);

/// Run a single operation using a scoped executor that inherits the parent's
/// policy and token limits. Only the declared output variable is returned
/// (not the entire child context — fixes the context merge race in #168).
fn run_operation(
    operation: Step,
    context_snapshot: crate::scroll::context::ExecutionContext,
    interface_registry: crate::scroll::interfaces::InterfaceRegistry,
    policy: crate::scroll::policy::PolicyEnforcer,
    tokens_limit: usize,
) -> Pin<Box<dyn Future<Output = OpResult> + Send>> {
    Box::pin(async move {
        let mut local_executor = super::executor::Executor {
            context: context_snapshot,
            interface_registry,
            policy_enforcer: policy,
            tokens_used: 0,
            tokens_limit,
            path_resolver: None,
        };

        let output_name = operation.output().map(|s| s.to_string());
        let result = local_executor.execute_step(&operation).await;

        let output_value = match &result {
            Ok(_) => output_name
                .as_deref()
                .and_then(|name| local_executor.context.get_variable(name).cloned())
                .unwrap_or(serde_json::Value::Null),
            Err(_) => serde_json::Value::Null,
        };

        (result, output_value, output_name, local_executor.tokens_used)
    })
}

/// Spawn all operations into a JoinSet, inheriting parent's policy and token limits.
fn spawn_operations(
    executor: &super::executor::Executor,
    operations: &[Step],
) -> JoinSet<(usize, OpResult)> {
    let mut set = JoinSet::new();
    for (index, operation) in operations.iter().enumerate() {
        let operation = operation.clone();
        let context_snapshot = executor.context.clone();
        let interface_registry_clone = executor.interface_registry.clone();
        let policy = executor.policy_enforcer.clone();
        let tokens_limit = executor.tokens_limit;

        set.spawn(async move {
            let result = run_operation(
                operation,
                context_snapshot,
                interface_registry_clone,
                policy,
                tokens_limit,
            )
            .await;
            (index, result)
        });
    }
    set
}

/// Merge a completed operation's result into the parent executor.
/// Only sets the declared output variable (not the entire child context).
/// Aggregates token usage.
fn merge_op_result(
    executor: &mut super::executor::Executor,
    output_value: serde_json::Value,
    output_name: Option<String>,
    tokens_used: usize,
    results: &mut [serde_json::Value],
    index: usize,
) {
    results[index] = output_value.clone();
    if let Some(name) = output_name {
        executor.context.set_variable(name, output_value);
    }
    executor.tokens_used += tokens_used;
}

/// Execute concurrent operations with halt-on-error behavior.
pub async fn execute_concurrent_halt(
    executor: &mut super::executor::Executor,
    operations: &[Step],
    timeout_duration: Option<Duration>,
) -> Result<serde_json::Value, ExecutionError> {
    let mut set = spawn_operations(executor, operations);
    let mut results = vec![serde_json::Value::Null; operations.len()];

    if let Some(dur) = timeout_duration {
        let deadline = tokio::time::Instant::now() + dur;
        loop {
            tokio::select! {
                biased;
                join_result = set.join_next() => {
                    match join_result {
                        Some(Ok((index, (result, output_value, output_name, tokens_used)))) => {
                            if let Err(e) = result {
                                set.abort_all();
                                return Err(e);
                            }
                            merge_op_result(executor, output_value, output_name, tokens_used, &mut results, index);
                        }
                        Some(Err(join_err)) => {
                            set.abort_all();
                            return Err(ExecutionError::Timeout(format!("concurrent task panicked: {join_err}")));
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    set.abort_all();
                    return Err(ExecutionError::Timeout("concurrent operations exceeded timeout".into()));
                }
            }
        }
    } else {
        while let Some(join_result) = set.join_next().await {
            match join_result {
                Ok((index, (result, output_value, output_name, tokens_used))) => {
                    if let Err(e) = result {
                        set.abort_all();
                        return Err(e);
                    }
                    merge_op_result(executor, output_value, output_name, tokens_used, &mut results, index);
                }
                Err(join_err) => {
                    set.abort_all();
                    return Err(ExecutionError::Timeout(format!("concurrent task panicked: {join_err}")));
                }
            }
        }
    }

    Ok(serde_json::Value::Array(results))
}

/// Execute concurrent operations with continue-on-error behavior.
pub async fn execute_concurrent_continue(
    executor: &mut super::executor::Executor,
    operations: &[Step],
    timeout_duration: Option<Duration>,
) -> Result<serde_json::Value, ExecutionError> {
    let mut set = spawn_operations(executor, operations);
    let mut results = vec![serde_json::Value::Null; operations.len()];

    if let Some(dur) = timeout_duration {
        let deadline = tokio::time::Instant::now() + dur;
        loop {
            tokio::select! {
                biased;
                join_result = set.join_next() => {
                    match join_result {
                        Some(Ok((index, (_result, output_value, output_name, tokens_used)))) => {
                            merge_op_result(executor, output_value, output_name, tokens_used, &mut results, index);
                        }
                        Some(Err(_)) => { /* Task panicked — null result, continue */ }
                        None => break,
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    set.abort_all();
                    tracing::warn!("concurrent operations exceeded timeout, returning partial results");
                    break;
                }
            }
        }
    } else {
        while let Some(join_result) = set.join_next().await {
            match join_result {
                Ok((index, (_result, output_value, output_name, tokens_used))) => {
                    merge_op_result(executor, output_value, output_name, tokens_used, &mut results, index);
                }
                Err(_) => { /* Task panicked — null result, continue */ }
            }
        }
    }

    Ok(serde_json::Value::Array(results))
}

/// Execute concurrent operations with error collection behavior.
pub async fn execute_concurrent_collect_errors(
    executor: &mut super::executor::Executor,
    operations: &[Step],
    timeout_duration: Option<Duration>,
) -> Result<serde_json::Value, ExecutionError> {
    let mut set = spawn_operations(executor, operations);
    let mut results = vec![serde_json::Value::Null; operations.len()];
    let mut errors: Vec<Option<String>> = vec![None; operations.len()];

    if let Some(dur) = timeout_duration {
        let deadline = tokio::time::Instant::now() + dur;
        loop {
            tokio::select! {
                biased;
                join_result = set.join_next() => {
                    match join_result {
                        Some(Ok((index, (result, output_value, output_name, tokens_used)))) => {
                            if let Err(e) = &result {
                                errors[index] = Some(format!("{e:?}"));
                            }
                            merge_op_result(executor, output_value, output_name, tokens_used, &mut results, index);
                        }
                        Some(Err(join_err)) => {
                            tracing::warn!("concurrent task panicked: {join_err}");
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    set.abort_all();
                    tracing::warn!("concurrent operations exceeded timeout, returning partial results");
                    for i in 0..operations.len() {
                        if errors[i].is_none() && results[i].is_null() {
                            errors[i] = Some("timeout: operation did not complete".to_string());
                        }
                    }
                    break;
                }
            }
        }
    } else {
        while let Some(join_result) = set.join_next().await {
            match join_result {
                Ok((index, (result, output_value, output_name, tokens_used))) => {
                    if let Err(e) = &result {
                        errors[index] = Some(format!("{e:?}"));
                    }
                    merge_op_result(executor, output_value, output_name, tokens_used, &mut results, index);
                }
                Err(join_err) => {
                    tracing::warn!("concurrent task panicked: {join_err}");
                }
            }
        }
    }

    let mut result_map = serde_json::Map::new();
    result_map.insert("results".to_string(), serde_json::Value::Array(results));
    let error_values: Vec<serde_json::Value> = errors
        .into_iter()
        .map(|e| match e {
            Some(err) => serde_json::Value::String(err),
            None => serde_json::Value::Null,
        })
        .collect();
    result_map.insert("errors".to_string(), serde_json::Value::Array(error_values));
    Ok(serde_json::Value::Object(result_map))
}
