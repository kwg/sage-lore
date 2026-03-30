// SPDX-License-Identifier: MIT
//! Scroll execution CLI command.

use std::io::IsTerminal;
use std::path::PathBuf;

use clap::Args;

use crate::scroll::assembly::{dispatch, parser as asm_parser};
use crate::scroll::executor::Executor;

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Path to the scroll YAML file
    pub scroll: PathBuf,

    /// Project root directory (defaults to current directory)
    #[arg(short, long, default_value = ".")]
    pub project: PathBuf,

    /// Variables as key=value (inline) or key=path (YAML/JSON file)
    /// Examples: --var epic_number=1 --var config=settings.json
    #[arg(long = "var", value_parser = parse_var)]
    pub vars: Vec<(String, serde_json::Value)>,

    /// Verbose output — show scroll execution progress on stderr.
    /// Use -v for step-level progress, -vv for debug detail.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Print the value of a context variable to stdout after execution.
    /// Use this to capture scroll output programmatically.
    /// Example: --output formatted_response
    #[arg(short, long)]
    pub output: Option<String>,
}

/// Parse key=value where value is either a file path (YAML/JSON) or an inline YAML literal.
/// Tries file first, falls back to parsing the raw value as YAML.
fn parse_var(s: &str) -> Result<(String, serde_json::Value), String> {
    let (key, value) = s
        .split_once('=')
        .ok_or_else(|| format!("Expected key=value format, got: {}", s))?;

    if key.is_empty() {
        return Err("Variable name cannot be empty".to_string());
    }

    // Try reading as a file path first
    if let Ok(content) = std::fs::read_to_string(value) {
        if let Ok(parsed) = serde_yaml::from_str::<serde_json::Value>(&content) {
            return Ok((key.to_string(), parsed));
        }
    }

    // For the Assembly parser, try to parse as a typed value:
    // numbers stay as numbers, bools as bools. This is needed because
    // the Assembly type system requires explicit types (D13r), and
    // primitives like platform.get_issue expect number params as i64.
    if let Ok(n) = value.parse::<i64>() {
        Ok((key.to_string(), serde_json::json!(n)))
    } else if let Ok(f) = value.parse::<f64>() {
        Ok((key.to_string(), serde_json::json!(f)))
    } else if value == "true" {
        Ok((key.to_string(), serde_json::json!(true)))
    } else if value == "false" {
        Ok((key.to_string(), serde_json::json!(false)))
    } else if value == "null" {
        Ok((key.to_string(), serde_json::Value::Null))
    } else {
        Ok((key.to_string(), serde_json::Value::String(value.to_string())))
    }
}

/// Initialize the tracing subscriber based on verbosity level.
fn init_tracing(verbose: u8) {
    use tracing_subscriber::EnvFilter;

    // RUST_LOG takes priority if set
    let filter = if std::env::var("RUST_LOG").is_ok() {
        EnvFilter::from_default_env()
    } else {
        match verbose {
            0 => EnvFilter::new("warn"),
            1 => EnvFilter::new("sage_lore=info"),
            _ => EnvFilter::new("sage_lore=debug"),
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}

pub async fn handle_run(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing before anything else
    init_tracing(args.verbose);

    // Resolve scroll path via search path (project → user → global → cwd fallback)
    let resolver = crate::config::PathResolver::discover(&args.project);
    let scroll_file = resolver.resolve_scroll(&args.scroll.to_string_lossy())
        .or_else(|| {
            // Direct file path fallback
            let p = args.scroll.clone();
            if p.exists() { Some(p) } else { None }
        })
        .ok_or_else(|| format!("Scroll not found: {}", args.scroll.display()))?;

    let scroll_content = std::fs::read_to_string(&scroll_file)
        .map_err(|e| format!("Failed to read scroll {}: {}", scroll_file.display(), e))?;

    let scroll_path = scroll_file.to_string_lossy().to_string();
    let ast = asm_parser::parse(&scroll_content, &scroll_path)
        .map_err(|diags| {
            let msgs: Vec<_> = diags.iter().map(|d| d.to_string()).collect();
            format!("Failed to parse scroll: {}", msgs.join("; "))
        })?;

    // Create executor
    let mut executor = Executor::from_project(&args.project)
        .map_err(|e| format!("Failed to initialize executor: {}", e))?;

    // Build inputs from CLI variables
    let mut inputs = std::collections::HashMap::new();
    for (key, value) in args.vars {
        tracing::debug!(var = %key, "Injecting CLI variable");
        inputs.insert(key, value);
    }

    // Execute scroll via Assembly dispatch
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await
        .map_err(|e| format!("Scroll execution failed: {}", e))?;

    // If --output is set, print the variable value to stdout
    if let Some(ref var_name) = args.output {
        if let Some(value) = outputs.get(var_name) {
            match value {
                serde_json::Value::String(s) => print!("{}", s),
                other => print!("{}", serde_json::to_string_pretty(other).unwrap_or_default()),
            }
        } else if let Some(value) = executor.context().get_variable(var_name) {
            match value {
                serde_json::Value::String(s) => print!("{}", s),
                other => print!("{}", serde_json::to_string_pretty(other).unwrap_or_default()),
            }
        } else {
            eprintln!("Warning: output variable '{}' not found in context", var_name);
        }
    } else {
        println!("Scroll '{}' executed successfully", ast.scroll.name);
    }
    Ok(())
}
