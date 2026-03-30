// SPDX-License-Identifier: MIT
//! Tests for platform primitive operations (env, info, check).

use sage_lore::scroll::platform;
use serde_json::Value;

#[test]
fn test_platform_env_get_existing_var() {
    // Set a test environment variable
    std::env::set_var("SAGE_TEST_VAR", "test_value");

    let mut params = serde_json::Map::new();
    params.insert(
        "var".to_string(),
        Value::String("SAGE_TEST_VAR".to_string()),
    );

    let result = platform::execute("env", &Value::Object(params));

    assert!(result.is_ok());
    let value = result.unwrap();
    assert_eq!(value.as_str(), Some("test_value"));

    // Clean up
    std::env::remove_var("SAGE_TEST_VAR");
}

#[test]
fn test_platform_env_missing_var() {
    let mut params = serde_json::Map::new();
    params.insert(
        "var".to_string(),
        Value::String("SAGE_NONEXISTENT_VAR_12345".to_string()),
    );

    let result = platform::execute("env", &Value::Object(params));

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_platform_env_missing_param() {
    let params = serde_json::Map::new();

    let result = platform::execute("env", &Value::Object(params));

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("var required"));
}

#[test]
fn test_platform_info() {
    let params = serde_json::Map::new();

    let result = platform::execute("info", &Value::Object(params));

    assert!(result.is_ok());
    let info = result.unwrap();

    // Check that we have the expected fields
    let map = info.as_object().expect("Info should be a mapping");

    assert!(map.contains_key("os"));
    assert!(map.contains_key("arch"));
    assert!(map.contains_key("family"));
    assert!(map.contains_key("dll_extension"));
    assert!(map.contains_key("exe_extension"));

    // Verify the values are strings
    assert!(map.get("os").unwrap().is_string());
    assert!(map.get("arch").unwrap().is_string());
    assert!(map.get("family").unwrap().is_string());
}

#[test]
fn test_platform_info_os_values() {
    let params = serde_json::Map::new();
    let result = platform::execute("info", &Value::Object(params)).unwrap();
    let map = result.as_object().unwrap();

    let os = map.get("os").unwrap().as_str().unwrap();
    // OS should be one of the known values
    assert!(
        os == "linux" || os == "macos" || os == "windows" ||
        os == "freebsd" || os == "openbsd" || os == "netbsd"
    );
}

#[test]
fn test_platform_check_available_command() {
    // Test with a command that should exist on most systems
    let mut params = serde_json::Map::new();
    params.insert(
        "command".to_string(),
        Value::String("sh".to_string()),
    );

    let result = platform::execute("check", &Value::Object(params));

    assert!(result.is_ok());
    let check = result.unwrap();
    let map = check.as_object().expect("Check result should be a mapping");

    assert!(map.contains_key("available"));
    let available = map.get("available")
        .unwrap()
        .as_bool()
        .unwrap();

    // sh should be available on Unix-like systems
    if cfg!(unix) {
        assert!(available);
        assert!(map.contains_key("path"));
    }
}

#[test]
fn test_platform_check_unavailable_command() {
    let mut params = serde_json::Map::new();
    params.insert(
        "command".to_string(),
        Value::String("nonexistent_command_xyz123".to_string()),
    );

    let result = platform::execute("check", &Value::Object(params));

    assert!(result.is_ok());
    let check = result.unwrap();
    let map = check.as_object().unwrap();

    let available = map.get("available")
        .unwrap()
        .as_bool()
        .unwrap();

    assert!(!available);
    // Path should not be present for unavailable commands
    assert!(!map.contains_key("path"));
}

#[test]
fn test_platform_check_missing_param() {
    let params = serde_json::Map::new();

    let result = platform::execute("check", &Value::Object(params));

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("command required"));
}

#[test]
fn test_platform_unknown_operation() {
    let params = serde_json::Map::new();

    let result = platform::execute("unknown_op", &Value::Object(params));

    assert!(result.is_err());
    let err = result.unwrap_err();
    // Check that it's a NotImplemented error
    assert!(err.to_string().contains("platform.unknown_op") || err.to_string().contains("not implemented"));
}

#[test]
fn test_platform_env_with_path_var() {
    // PATH should exist on all platforms
    let mut params = serde_json::Map::new();
    params.insert(
        "var".to_string(),
        Value::String("PATH".to_string()),
    );

    let result = platform::execute("env", &Value::Object(params));

    assert!(result.is_ok());
    let value = result.unwrap();
    assert!(value.is_string());
    // PATH should not be empty
    assert!(!value.as_str().unwrap().is_empty());
}

#[test]
fn test_platform_check_cargo() {
    // Since we're running tests with cargo, it should be available
    let mut params = serde_json::Map::new();
    params.insert(
        "command".to_string(),
        Value::String("cargo".to_string()),
    );

    let result = platform::execute("check", &Value::Object(params));

    assert!(result.is_ok());
    let check = result.unwrap();
    let map = check.as_object().unwrap();

    let available = map.get("available")
        .unwrap()
        .as_bool()
        .unwrap();

    assert!(available);

    let path = map.get("path")
        .unwrap()
        .as_str()
        .unwrap();

    assert!(path.contains("cargo"));
}
