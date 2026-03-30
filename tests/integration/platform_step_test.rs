// SPDX-License-Identifier: MIT
//! Integration tests for platform step execution through the executor.

use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::schema::{OnFail, PlatformOperation, PlatformParams, PlatformStep, Step};
use std::env;

#[tokio::test]
async fn test_platform_env_step_execution() {
    // Set up a test environment variable
    env::set_var("SAGE_INTEGRATION_TEST_VAR", "integration_test_value");

    let step = Step::Platform(PlatformStep {
        platform: PlatformParams {
            operation: PlatformOperation::Env,
            var: Some("SAGE_INTEGRATION_TEST_VAR".to_string()),
            command: None,
            number: None,
            payload: None,
            labels: None,
            body: None,
            head: None,
            base: None,
            title: None,
            description: None,
            strategy: None,
            state: None,
            milestone: None,
            assignee: None,
        },
        output: Some("env_value".to_string()),
        on_fail: OnFail::Halt,
    });

    let mut executor = Executor::new();

    let result = executor.execute_step(&step).await;
    assert!(result.is_ok());

    // Verify the output was bound
    let env_value = executor.context().get_variable("env_value");
    assert!(env_value.is_some());
    assert_eq!(env_value.unwrap().as_str(), Some("integration_test_value"));

    // Clean up
    env::remove_var("SAGE_INTEGRATION_TEST_VAR");
}

#[tokio::test]
async fn test_platform_info_step_execution() {
    let step = Step::Platform(PlatformStep {
        platform: PlatformParams {
            operation: PlatformOperation::Info,
            var: None,
            command: None,
            number: None,
            payload: None,
            labels: None,
            body: None,
            head: None,
            base: None,
            title: None,
            description: None,
            strategy: None,
            state: None,
            milestone: None,
            assignee: None,
        },
        output: Some("platform_info".to_string()),
        on_fail: OnFail::Halt,
    });

    let mut executor = Executor::new();

    let result = executor.execute_step(&step).await;
    assert!(result.is_ok());

    // Verify the output was bound
    let platform_info = executor.context().get_variable("platform_info");
    assert!(platform_info.is_some());

    let info = platform_info.unwrap();
    let map = info.as_object().expect("Platform info should be a mapping");

    // Verify expected fields
    assert!(map.contains_key("os"));
    assert!(map.contains_key("arch"));
    assert!(map.contains_key("family"));
}

#[tokio::test]
async fn test_platform_check_step_execution() {
    let step = Step::Platform(PlatformStep {
        platform: PlatformParams {
            operation: PlatformOperation::Check,
            var: None,
            command: Some("cargo".to_string()),
            number: None,
            payload: None,
            labels: None,
            body: None,
            head: None,
            base: None,
            title: None,
            description: None,
            strategy: None,
            state: None,
            milestone: None,
            assignee: None,
        },
        output: Some("cargo_check".to_string()),
        on_fail: OnFail::Halt,
    });

    let mut executor = Executor::new();

    let result = executor.execute_step(&step).await;
    assert!(result.is_ok());

    // Verify the output was bound
    let cargo_check = executor.context().get_variable("cargo_check");
    assert!(cargo_check.is_some());

    let check = cargo_check.unwrap();
    let map = check.as_object().expect("Check result should be a mapping");

    // Cargo should be available since we're running with cargo test
    assert!(map.contains_key("available"));
    let available = map.get("available")
        .unwrap()
        .as_bool()
        .unwrap();
    assert!(available);
}

#[tokio::test]
async fn test_platform_env_missing_var_with_on_fail_halt() {
    let step = Step::Platform(PlatformStep {
        platform: PlatformParams {
            operation: PlatformOperation::Env,
            var: Some("SAGE_NONEXISTENT_VAR_XYZ".to_string()),
            command: None,
            number: None,
            payload: None,
            labels: None,
            body: None,
            head: None,
            base: None,
            title: None,
            description: None,
            strategy: None,
            state: None,
            milestone: None,
            assignee: None,
        },
        output: Some("missing_var".to_string()),
        on_fail: OnFail::Halt,
    });

    let mut executor = Executor::new();

    let result = executor.execute_step(&step).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_platform_env_missing_var_with_on_fail_continue() {
    let step = Step::Platform(PlatformStep {
        platform: PlatformParams {
            operation: PlatformOperation::Env,
            var: Some("SAGE_NONEXISTENT_VAR_XYZ".to_string()),
            command: None,
            number: None,
            payload: None,
            labels: None,
            body: None,
            head: None,
            base: None,
            title: None,
            description: None,
            strategy: None,
            state: None,
            milestone: None,
            assignee: None,
        },
        output: Some("missing_var".to_string()),
        on_fail: OnFail::Continue,
    });

    let mut executor = Executor::new();

    let result = executor.execute_step(&step).await;
    // Should succeed with OnFail::Continue
    assert!(result.is_ok());

    // Output should be null
    let missing_var = executor.context().get_variable("missing_var");
    assert!(missing_var.is_some());
    assert!(missing_var.unwrap().is_null());
}

#[tokio::test]
async fn test_platform_check_unavailable_command() {
    let step = Step::Platform(PlatformStep {
        platform: PlatformParams {
            operation: PlatformOperation::Check,
            var: None,
            command: Some("nonexistent_command_abc123".to_string()),
            number: None,
            payload: None,
            labels: None,
            body: None,
            head: None,
            base: None,
            title: None,
            description: None,
            strategy: None,
            state: None,
            milestone: None,
            assignee: None,
        },
        output: Some("command_check".to_string()),
        on_fail: OnFail::Halt,
    });

    let mut executor = Executor::new();

    let result = executor.execute_step(&step).await;
    assert!(result.is_ok());

    // Verify the output indicates unavailable
    let command_check = executor.context().get_variable("command_check");
    assert!(command_check.is_some());

    let check = command_check.unwrap();
    let map = check.as_object().unwrap();

    let available = map.get("available")
        .unwrap()
        .as_bool()
        .unwrap();
    assert!(!available);
}
