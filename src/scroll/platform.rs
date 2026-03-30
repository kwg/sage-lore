// SPDX-License-Identifier: MIT

//! Platform information primitive.
//!
//! Provides access to platform and environment information, including:
//! - Operating system detection
//! - Environment variable access
//! - System capabilities
//! - Platform-specific paths

use crate::scroll::error::ExecutionError;
use std::env;

/// Execute a platform information query.
///
/// # Parameters
/// - `operation`: The type of platform query (env, info, check)
/// - `params`: Operation-specific parameters
///
/// # Returns
/// Result containing the platform information or an error.
pub fn execute(operation: &str, params: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    match operation {
        "env" => get_env_var(params),
        "info" => get_platform_info(),
        "check" => check_command(params),
        _ => Err(ExecutionError::NotImplemented(format!("platform.{}", operation))),
    }
}

/// Get an environment variable value.
///
/// # Parameters
/// - `params`: Must contain a "var" field with the variable name
///
/// # Returns
/// The environment variable value as a string, or an error if not found.
fn get_env_var(params: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    let var_name = params
        .get("var")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::MissingParameter("var required for env operation".to_string()))?;

    match env::var(var_name) {
        Ok(value) => Ok(serde_json::Value::String(value)),
        Err(_) => Err(ExecutionError::InterfaceError(format!(
            "Environment variable '{}' not found",
            var_name
        ))),
    }
}

/// Get platform information including OS, architecture, and family.
///
/// # Returns
/// A mapping containing platform details: os, arch, family, os_version
fn get_platform_info() -> Result<serde_json::Value, ExecutionError> {
    let mut info = serde_json::Map::new();

    info.insert("os".to_string(), serde_json::Value::String(env::consts::OS.to_string()));
    info.insert("arch".to_string(), serde_json::Value::String(env::consts::ARCH.to_string()));
    info.insert("family".to_string(), serde_json::Value::String(env::consts::FAMILY.to_string()));
    info.insert("dll_extension".to_string(), serde_json::Value::String(env::consts::DLL_EXTENSION.to_string()));
    info.insert("exe_extension".to_string(), serde_json::Value::String(env::consts::EXE_EXTENSION.to_string()));

    Ok(serde_json::Value::Object(info))
}

/// Check if a command/tool is available on the system.
///
/// # Parameters
/// - `params`: Must contain a "command" field with the command name
///
/// # Returns
/// A mapping containing "available" (bool) and optionally "path" (string) if found
fn check_command(params: &serde_json::Value) -> Result<serde_json::Value, ExecutionError> {
    let command_name = params
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ExecutionError::MissingParameter("command required for check operation".to_string()))?;

    let mut result = serde_json::Map::new();

    match which::which(command_name) {
        Ok(path) => {
            result.insert("available".to_string(), serde_json::Value::Bool(true));
            result.insert("path".to_string(), serde_json::Value::String(path.to_string_lossy().to_string()));
        }
        Err(_) => {
            result.insert("available".to_string(), serde_json::Value::Bool(false));
        }
    }

    Ok(serde_json::Value::Object(result))
}
