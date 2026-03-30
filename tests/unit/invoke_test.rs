//! Unit tests for LlmBackend trait and implementations.

use sage_lore::primitives::invoke::{
    ClaudeCliBackend, LlmBackend, LlmError, LlmRequest, LlmResponse, MockLlmBackend, MockResponseKey,
};

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a basic request for testing.
fn sample_request() -> LlmRequest {
    LlmRequest {
        prompt: "What is 2+2?".to_string(),
        system: Some("You are a helpful assistant.".to_string()),
        max_tokens: Some(100),
        temperature: Some(0.7),
        timeout_secs: Some(30),
        model_tier: None,
            format_schema: None,
            model: None,
    }
}

/// Create a sample response for testing.
fn sample_response() -> LlmResponse {
    LlmResponse {
        text: "The answer is 4.".to_string(),
        tokens_used: Some(10),
        model: "claude-3-opus".to_string(),
        truncated: false,
    }
}

// ============================================================================
// MockLlmBackend Construction Tests
// ============================================================================

#[tokio::test]
async fn test_mock_backend_new() {
    let mock = MockLlmBackend::new();
    assert_eq!(mock.total_calls(), 0);
    assert!(mock.calls().is_empty());
}

#[tokio::test]
async fn test_mock_backend_default() {
    let mock = MockLlmBackend::default();
    assert_eq!(mock.total_calls(), 0);
}

#[tokio::test]
async fn test_mock_backend_debug() {
    let mock = MockLlmBackend::new();
    let debug = format!("{:?}", mock);
    assert!(debug.contains("MockLlmBackend"));
}

// ============================================================================
// LlmRequest Tests
// ============================================================================

#[tokio::test]
async fn test_request_minimal() {
    let req = LlmRequest {
        prompt: "Hello".to_string(),
        system: None,
        max_tokens: None,
        temperature: None,
        timeout_secs: None,
        model_tier: None,
            format_schema: None,
            model: None,
    };
    assert_eq!(req.prompt, "Hello");
    assert!(req.system.is_none());
}

#[tokio::test]
async fn test_request_full() {
    let req = sample_request();
    assert_eq!(req.prompt, "What is 2+2?");
    assert_eq!(req.system, Some("You are a helpful assistant.".to_string()));
    assert_eq!(req.max_tokens, Some(100));
    assert_eq!(req.temperature, Some(0.7));
    assert_eq!(req.timeout_secs, Some(30));
}

#[tokio::test]
async fn test_request_clone() {
    let req1 = sample_request();
    let req2 = req1.clone();
    assert_eq!(req1.prompt, req2.prompt);
    assert_eq!(req1.system, req2.system);
}

// ============================================================================
// LlmResponse Tests
// ============================================================================

#[tokio::test]
async fn test_response_fields() {
    let resp = sample_response();
    assert_eq!(resp.text, "The answer is 4.");
    assert_eq!(resp.tokens_used, Some(10));
    assert_eq!(resp.model, "claude-3-opus");
    assert!(!resp.truncated);
}

#[tokio::test]
async fn test_response_truncated() {
    let resp = LlmResponse {
        text: "Partial response...".to_string(),
        tokens_used: Some(4096),
        model: "claude-3-opus".to_string(),
        truncated: true,
    };
    assert!(resp.truncated);
}

// ============================================================================
// MockLlmBackend Generate Tests
// ============================================================================

#[tokio::test]
async fn test_generate_with_configured_response() {
    let expected = sample_response();
    let mock = MockLlmBackend::new()
        .with_response(MockResponseKey::Generate, Ok(expected.clone()));

    let req = sample_request();
    let result = mock.generate(req.clone()).await.unwrap();

    assert_eq!(result.text, expected.text);
    assert_eq!(result.model, expected.model);
    assert!(mock.was_called_with_prompt("What is 2+2?"));
}

#[tokio::test]
async fn test_generate_with_default_response() {
    let default_response = sample_response();
    let mock = MockLlmBackend::new()
        .with_default_response(default_response.clone());

    let req = LlmRequest {
        prompt: "Any prompt".to_string(),
        system: None,
        max_tokens: None,
        temperature: None,
        timeout_secs: None,
        model_tier: None,
            format_schema: None,
            model: None,
    };

    let result = mock.generate(req).await.unwrap();
    assert_eq!(result.text, default_response.text);
}

#[tokio::test]
async fn test_generate_error_response() {
    let mock = MockLlmBackend::new()
        .with_response(MockResponseKey::Generate, Err(LlmError::Timeout));

    let req = sample_request();
    let result = mock.generate(req).await;
    assert!(matches!(result, Err(LlmError::Timeout)));
}

#[tokio::test]
async fn test_generate_no_configured_response() {
    let mock = MockLlmBackend::new();

    let req = sample_request();
    let result = mock.generate(req).await;
    // With smart response generation, even unconfigured mocks return a response
    assert!(result.is_ok(), "Smart response generation should always succeed");
    assert!(result.unwrap().text.len() > 0, "Should generate non-empty response");
}

#[tokio::test]
async fn test_generate_canned_responses_from_file() {
    // MockLlmBackend can load canned responses from a file
    let canned = vec![
        ("What is 2+2?".to_string(), "4".to_string()),
        ("Hello".to_string(), "Hi there!".to_string()),
    ];

    let mock = MockLlmBackend::new()
        .with_canned_responses(canned);

    let req1 = LlmRequest {
        prompt: "What is 2+2?".to_string(),
        system: None,
        max_tokens: None,
        temperature: None,
        timeout_secs: None,
        model_tier: None,
            format_schema: None,
            model: None,
    };
    let result1 = mock.generate(req1).await.unwrap();
    assert_eq!(result1.text, "4");

    let req2 = LlmRequest {
        prompt: "Hello".to_string(),
        system: None,
        max_tokens: None,
        temperature: None,
        timeout_secs: None,
        model_tier: None,
            format_schema: None,
            model: None,
    };
    let result2 = mock.generate(req2).await.unwrap();
    assert_eq!(result2.text, "Hi there!");
}

// ============================================================================
// Call Recording Tests
// ============================================================================

#[tokio::test]
async fn test_call_recording() {
    let mock = MockLlmBackend::new()
        .with_default_response(sample_response());

    let req1 = LlmRequest {
        prompt: "First prompt".to_string(),
        system: None,
        max_tokens: None,
        temperature: None,
        timeout_secs: None,
        model_tier: None,
            format_schema: None,
            model: None,
    };
    mock.generate(req1).await.unwrap();

    let req2 = LlmRequest {
        prompt: "Second prompt".to_string(),
        system: Some("System".to_string()),
        max_tokens: Some(50),
        temperature: None,
        timeout_secs: None,
        model_tier: None,
            format_schema: None,
            model: None,
    };
    mock.generate(req2).await.unwrap();

    assert_eq!(mock.total_calls(), 2);

    let calls = mock.calls();
    assert_eq!(calls[0].prompt, "First prompt");
    assert_eq!(calls[1].prompt, "Second prompt");
    assert_eq!(calls[1].system, Some("System".to_string()));
}

#[tokio::test]
async fn test_was_called_with_prompt() {
    let mock = MockLlmBackend::new()
        .with_default_response(sample_response());

    let req = LlmRequest {
        prompt: "Specific prompt text".to_string(),
        system: None,
        max_tokens: None,
        temperature: None,
        timeout_secs: None,
        model_tier: None,
            format_schema: None,
            model: None,
    };
    mock.generate(req).await.unwrap();

    assert!(mock.was_called_with_prompt("Specific prompt text"));
    assert!(!mock.was_called_with_prompt("Different prompt"));
}

#[tokio::test]
async fn test_clear_calls() {
    let mock = MockLlmBackend::new()
        .with_default_response(sample_response());

    let req = sample_request();
    mock.generate(req.clone()).await.unwrap();
    mock.generate(req).await.unwrap();
    assert_eq!(mock.total_calls(), 2);

    mock.clear_calls();
    assert_eq!(mock.total_calls(), 0);
}

// ============================================================================
// LlmError Tests
// ============================================================================

#[tokio::test]
async fn test_error_display() {
    let err = LlmError::Timeout;
    assert!(err.to_string().to_lowercase().contains("timed out") || err.to_string().to_lowercase().contains("timeout"));

    let err = LlmError::CliNotFound;
    assert!(err.to_string().to_lowercase().contains("not found") || err.to_string().to_lowercase().contains("cli"));

    let err = LlmError::ExecutionFailed("exit code 1".to_string());
    assert!(err.to_string().contains("exit code 1"));

    let err = LlmError::ParseError("invalid json".to_string());
    assert!(err.to_string().contains("invalid json"));
}

#[tokio::test]
async fn test_error_clone() {
    let errors = vec![
        LlmError::Timeout,
        LlmError::CliNotFound,
        LlmError::NotConfigured,
        LlmError::ExecutionFailed("test".to_string()),
        LlmError::ParseError("test".to_string()),
    ];

    for err in errors {
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }
}

// ============================================================================
// ClaudeCliBackend Tests (with mock execution)
// ============================================================================

#[tokio::test]
async fn test_claude_cli_backend_new() {
    // Just test construction - actual execution requires claude CLI
    let backend = ClaudeCliBackend::new();
    let debug = format!("{:?}", backend);
    assert!(debug.contains("ClaudeCliBackend"));
}

#[tokio::test]
async fn test_claude_cli_backend_with_model() {
    let backend = ClaudeCliBackend::new()
        .with_model("claude-3-sonnet");
    let debug = format!("{:?}", backend);
    assert!(debug.contains("claude-3-sonnet"));
}

// Note: Full ClaudeCliBackend integration tests require the claude CLI
// and would be in a separate integration test file.

// ============================================================================
// Trait Object Safety Tests
// ============================================================================

#[tokio::test]
async fn test_llm_backend_object_safety() {
    // Verify LlmBackend can be used as a trait object
    let mock = MockLlmBackend::new()
        .with_default_response(sample_response());

    let backend: Box<dyn LlmBackend> = Box::new(mock);

    let req = sample_request();
    let result = backend.generate(req).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_llm_backend_send_sync() {
    // Verify LlmBackend implementations are Send + Sync
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<MockLlmBackend>();
    assert_send_sync::<ClaudeCliBackend>();
}
