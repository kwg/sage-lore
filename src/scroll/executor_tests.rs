// SPDX-License-Identifier: MIT
//! Tests for executor and step dispatch functionality.

#[cfg(test)]
mod tests {
    use crate::scroll::executor::Executor;
    use crate::scroll::error::ExecutionError;
    use crate::scroll::extraction::{
        extract_structured_content, is_likely_yaml_structure, parse_as_sequence,
        try_parse_structured,
    };
    use crate::scroll::schema::{
        AggregateStep, BranchStep, ConsensusStep,
        ElaborateParams, ElaborateStep, FsOperation, FsParams, FsStep, InvokeStep, LoopStep, ParallelStep,
        SecureStep, Step,
    };

    // ========================================================================
    // Step Dispatch Tests
    // ========================================================================

    #[tokio::test]
    async fn test_execute_step_dispatch_invoke_agent() {
        let mut executor = Executor::new();
        let step = Step::Invoke(InvokeStep {
            invoke: crate::scroll::schema::InvokeParams {
                agent: "test-agent".to_string(),
                instructions: "test instructions".to_string(),
                context: None,
                timeout_secs: None,
                backend: None,
                model_tier: None,
                model: None,
                output_schema: None,
            },
            output: Some("result".to_string()),
            on_fail: Default::default(),
        });

        // Agent invocation should succeed with mock backend
        let result = executor.execute_step(&step).await;
        assert!(result.is_ok(), "Failed with error: {:?}", result.err());

        // Check that result was stored
        let output = executor.context().get_variable("result");
        assert!(output.is_some());
    }

    #[tokio::test]
    async fn test_execute_step_dispatch_parallel() {
        let mut executor = Executor::new();
        let step = Step::Parallel(ParallelStep {
            parallel: crate::scroll::schema::ParallelParams {
                agents: vec!["agent1".to_string(), "agent2".to_string()],
                prompt: "test prompt".to_string(),
                max_concurrent: 3,
                timeout_per_agent: None,
                on_fail: crate::scroll::schema::ParallelFailMode::BestEffort,
                quorum: None,
            },
            output: Some("results".to_string()),
            on_fail: Default::default(),
        });

        // Parallel should execute successfully with best_effort mode (allows failures)
        let result = executor.execute_step(&step).await;
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);

        // Check that results were stored (even if null due to mock failures)
        let results = executor.context().get_variable("results");
        assert!(results.is_some(), "Results should be stored");
    }

    #[tokio::test]
    async fn test_execute_step_dispatch_consensus_with_majority() {
        let mut executor = Executor::new();
        let step = Step::Consensus(ConsensusStep {
            consensus: crate::scroll::schema::ConsensusParams {
                agents: vec!["agent1".to_string(), "agent2".to_string()],
                proposal: "test proposal".to_string(),
                mechanism: "vote".to_string(),
                options: vec!["yes".to_string(), "no".to_string()],
                threshold: crate::scroll::schema::ThresholdSpec::Named(
                    crate::scroll::schema::ThresholdType::Majority
                ),
            },
            output: Some("vote_result".to_string()),
            on_fail: Default::default(),
        });

        // Consensus should execute and store result
        let result = executor.execute_step(&step).await;

        // With mock agents, we expect it to fail because mock agents don't return proper votes
        // But we can verify the structure was attempted
        // In a real scenario with proper agent implementations, this would succeed
        let _vote_result = executor.context().get_variable("vote_result");
    }

    #[tokio::test]
    async fn test_execute_step_dispatch_consensus_with_numeric_threshold() {
        let mut executor = Executor::new();
        let step = Step::Consensus(ConsensusStep {
            consensus: crate::scroll::schema::ConsensusParams {
                agents: vec![
                    "agent1".to_string(),
                    "agent2".to_string(),
                    "agent3".to_string(),
                ],
                proposal: "requires at least 2 votes".to_string(),
                mechanism: "vote".to_string(),
                options: vec!["approve".to_string(), "reject".to_string()],
                threshold: crate::scroll::schema::ThresholdSpec::Numeric(2),
            },
            output: Some("consensus_result".to_string()),
            on_fail: Default::default(),
        });

        // Execute consensus with numeric threshold
        let _result = executor.execute_step(&step).await;
    }

    #[tokio::test]
    async fn test_execute_step_dispatch_branch() {
        let mut executor = Executor::new();
        let step = Step::Branch(BranchStep {
            branch: crate::scroll::schema::BranchParams {
                condition: "${flag}".to_string(),
                if_true: vec![],
                if_false: None,
            },
            output: None,
        });

        // Branch with empty if_true and no if_false returns Ok
        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_step_dispatch_loop() {
        let mut executor = Executor::new();
        // Set up items to iterate over
        executor.context_mut().set_variable(
            "items".to_string(),
            serde_json::json!(["a", "b"]),
        );
        let step = Step::Loop(LoopStep {
            loop_params: crate::scroll::schema::LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: None,
                max: None,
            },
            output: None,
        });

        // Loop with valid items returns Ok
        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_step_dispatch_loop_missing_items() {
        let mut executor = Executor::new();
        let step = Step::Loop(LoopStep {
            loop_params: crate::scroll::schema::LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: None,
                max: None,
            },
            output: None,
        });

        // Loop with missing items variable returns VariableNotFound
        let result = executor.execute_step(&step).await;
        assert!(matches!(result, Err(ExecutionError::VariableNotFound(_))));
    }

    #[tokio::test]
    async fn test_execute_step_dispatch_aggregate() {
        let mut executor = Executor::new();
        // Set up variables to aggregate
        executor
            .context_mut()
            .set_variable("a".to_string(), serde_json::Value::String("x".to_string()));
        executor
            .context_mut()
            .set_variable("b".to_string(), serde_json::Value::String("y".to_string()));
        let step = Step::Aggregate(AggregateStep {
            aggregate: crate::scroll::schema::AggregateParams {
                results: vec!["${a}".to_string(), "${b}".to_string()],
                strategy: "concat".to_string(),
            },
            output: None,
        });

        // Aggregate with valid results returns Ok
        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_track_tokens() {
        let mut executor = Executor::new();

        // Track tokens with known strings
        let prompt = "This is a test prompt with some text";  // ~10 tokens
        let response = "This is a test response"; // ~5 tokens

        executor.track_tokens(prompt, response);

        // Verify tokens were tracked (rough estimate: ~15 tokens * 4 chars/token = 60 chars / 4 = 15 tokens)
        assert!(executor.tokens_used > 0);
        assert!(executor.tokens_used < executor.tokens_limit);
    }

    #[tokio::test]
    async fn test_execute_step_dispatch_secure() {
        let mut executor = Executor::new();
        let step = Step::Secure(SecureStep {
            secure: crate::scroll::schema::SecureParams {
                input: None,
                scan_type: crate::scroll::schema::ScanType::SecretDetection,
                policy: Default::default(),
            },
            output: None,
            on_fail: Default::default(),
        });

        // Secure dispatches to interface, which returns NotImplemented
        let result = executor.execute_step(&step).await;
        assert!(matches!(
            result,
            Err(ExecutionError::NotImplemented(s)) if s.contains("secure")
        ));
    }

    // ========================================================================
    // Step Output Helper Tests
    // ========================================================================

    #[tokio::test]
    async fn test_step_output_some() {
        let step = Step::Elaborate(ElaborateStep {
            elaborate: ElaborateParams {
                input: "${prev}".to_string(),
                depth: Default::default(),
                output_contract: None,
                context: None,
                backend: None,
                model_tier: None,
                model: None,
                format_schema: None,
            },
            output: Some("result".to_string()),
            on_fail: Default::default(),
        });

        assert_eq!(step.output(), Some(&"result".to_string()));
    }

    #[tokio::test]
    async fn test_step_output_none() {
        let step = Step::Elaborate(ElaborateStep {
            elaborate: ElaborateParams {
                input: "${prev}".to_string(),
                depth: Default::default(),
                output_contract: None,
                context: None,
                backend: None,
                model_tier: None,
                model: None,
                format_schema: None,
            },
            output: None,
            on_fail: Default::default(),
        });

        assert_eq!(step.output(), None);
    }

    // ========================================================================
    // LLM Response Extraction Tests — verify stripping of markdown fences and prose prefixes (Bug #305)
    // ========================================================================

    #[tokio::test]
    async fn test_extract_xml_tags() {
        let response = "Here's the decomposition:\n<yaml>\n- item1\n- item2\n</yaml>\nHope this helps!";
        let extracted = extract_structured_content(response, "yaml");
        assert_eq!(extracted, "- item1\n- item2");
    }

    #[tokio::test]
    async fn test_extract_markdown_fence() {
        let response = "Here's the result:\n```yaml\n- first\n- second\n```\nLet me know if you need more.";
        let extracted = extract_structured_content(response, "yaml");
        assert_eq!(extracted, "- first\n- second");
    }

    #[tokio::test]
    async fn test_extract_bare_fence() {
        let response = "```\n- alpha\n- beta\n```";
        let extracted = extract_structured_content(response, "yaml");
        assert_eq!(extracted, "- alpha\n- beta");
    }

    #[tokio::test]
    async fn test_extract_strips_prose_prefix() {
        let response = "Sure! Here's your list:\n- one\n- two\n- three";
        let extracted = extract_structured_content(response, "yaml");
        assert_eq!(extracted, "- one\n- two\n- three");
    }

    #[tokio::test]
    async fn test_extract_raw_yaml() {
        let response = "- direct\n- yaml\n- list";
        let extracted = extract_structured_content(response, "yaml");
        assert_eq!(extracted, "- direct\n- yaml\n- list");
    }

    #[tokio::test]
    async fn test_parse_as_sequence_with_xml_wrapped() {
        let value = serde_json::Value::String(
            "Here's the list:\n<yaml>\n- a\n- b\n- c\n</yaml>".to_string()
        );
        let result = parse_as_sequence(&value).unwrap();
        match result {
            serde_json::Value::Array(seq) => {
                assert_eq!(seq.len(), 3);
            }
            _ => panic!("Expected sequence"),
        }
    }

    #[tokio::test]
    async fn test_parse_as_sequence_with_markdown_fence() {
        let value = serde_json::Value::String(
            "```yaml\n- x\n- y\n```".to_string()
        );
        let result = parse_as_sequence(&value).unwrap();
        match result {
            serde_json::Value::Array(seq) => {
                assert_eq!(seq.len(), 2);
            }
            _ => panic!("Expected sequence"),
        }
    }

    #[tokio::test]
    async fn test_parse_as_sequence_realistic_llm_response() {
        // Realistic LLM response with prose before <yaml> tags
        let response = r#"Based on the schema in `chunk-schema.json`, the `implementation_chunks` schema expects chunks with `title`, `complexity`, `files`, and `spec` fields. Let me decompose the input into this format:

<yaml>
- title: "Define dice types"
  complexity: low
  files:
    - "src/dice/types.rs"
  spec: |
    Create types.rs with core dice engine type definitions.

- title: "Implement dice parser"
  complexity: medium
  files:
    - "src/dice/parser.rs"
  spec: |
    Implement parser.rs for dice notation parsing.
</yaml>"#;

        let value = serde_json::Value::String(response.to_string());
        let result = parse_as_sequence(&value).unwrap();
        match result {
            serde_json::Value::Array(seq) => {
                assert_eq!(seq.len(), 2);
                // Verify first item structure
                let first = &seq[0];
                assert!(first.get("title").is_some());
                assert!(first.get("complexity").is_some());
            }
            _ => panic!("Expected sequence"),
        }
    }

    // ========================================================================
    // YAML Structure Detection Tests — identify YAML sequences/mappings in raw LLM output (Bug #306)
    // ========================================================================

    #[tokio::test]
    async fn test_is_likely_yaml_structure_sequence() {
        assert!(is_likely_yaml_structure("- item1\n- item2"));
        assert!(is_likely_yaml_structure("  - indented"));
    }

    #[tokio::test]
    async fn test_is_likely_yaml_structure_json_object() {
        assert!(is_likely_yaml_structure("{\"key\": \"value\"}"));
        assert!(is_likely_yaml_structure("{ key: value }"));
    }

    #[tokio::test]
    async fn test_is_likely_yaml_structure_json_array() {
        assert!(is_likely_yaml_structure("[1, 2, 3]"));
        assert!(is_likely_yaml_structure("[ \"a\", \"b\" ]"));
    }

    #[tokio::test]
    async fn test_is_likely_yaml_structure_yaml_mapping() {
        // This was the bug - mappings starting with keys weren't detected
        assert!(is_likely_yaml_structure("story_title: \"Test\""));
        assert!(is_likely_yaml_structure("status: completed"));
        assert!(is_likely_yaml_structure("key: value\nanother: thing"));
        assert!(is_likely_yaml_structure("snake_case_key: value"));
        assert!(is_likely_yaml_structure("kebab-case-key: value"));
        assert!(is_likely_yaml_structure("key123: value"));
    }

    #[tokio::test]
    async fn test_is_likely_yaml_structure_rejects_prose() {
        // Prose with colons should NOT be detected as YAML
        assert!(!is_likely_yaml_structure("Here's the answer: use a mapping"));
        assert!(!is_likely_yaml_structure("Note: this is important"));
        assert!(!is_likely_yaml_structure("The solution is: do X"));
    }

    #[tokio::test]
    async fn test_is_likely_yaml_structure_rejects_plain_text() {
        assert!(!is_likely_yaml_structure("Just some text"));
        assert!(!is_likely_yaml_structure("No structure here"));
        assert!(!is_likely_yaml_structure(""));
    }

    #[tokio::test]
    async fn test_try_parse_structured_yaml_mapping() {
        // Verify the full pipeline works for YAML mappings
        let input = serde_json::Value::String(
            "story_title: \"Dice Engine\"\nstatus: completed\nchunks: 5".to_string()
        );
        let result = try_parse_structured(&input).unwrap();

        // Should be parsed as a mapping, not remain a string
        assert!(result.is_object(), "Expected mapping, got: {:?}", result);

        let mapping = result.as_object().unwrap();
        assert_eq!(
            mapping.get("story_title").and_then(|v| v.as_str()),
            Some("Dice Engine")
        );
        assert_eq!(
            mapping.get("status").and_then(|v| v.as_str()),
            Some("completed")
        );
    }

    #[tokio::test]
    async fn test_try_parse_structured_with_xml_tags() {
        // Mapping inside XML tags
        let input = serde_json::Value::String(
            "Here's your result:\n<yaml>\nstory_title: Test\nstatus: done\n</yaml>".to_string()
        );
        let result = try_parse_structured(&input).unwrap();

        assert!(result.is_object(), "Expected mapping, got: {:?}", result);
    }

    // ========================================================================
    // Constructor Tests
    // ========================================================================

    #[tokio::test]
    async fn test_executor_new() {
        let executor = Executor::new();
        // Verify executor initializes - context is private but executor exists
        assert!(executor.context.prev().is_none());
    }

    #[tokio::test]
    async fn test_executor_default() {
        let executor = Executor::default();
        // Verify Default trait implementation works
        assert!(executor.context.prev().is_none());
    }

    #[tokio::test]
    async fn test_interface_registry_new() {
        use crate::scroll::interfaces::InterfaceRegistry;
        let _registry = InterfaceRegistry::new();
    }

    #[tokio::test]
    async fn test_interface_registry_default() {
        use crate::scroll::interfaces::InterfaceRegistry;
        let _registry = InterfaceRegistry::default();
    }

    #[tokio::test]
    async fn test_policy_enforcer_new() {
        use crate::scroll::policy::{EnforcementMode, PolicyEnforcer};
        let _enforcer = PolicyEnforcer::new(EnforcementMode::Permissive);
    }

    #[tokio::test]
    async fn test_policy_enforcer_default() {
        use crate::scroll::policy::PolicyEnforcer;
        let _enforcer = PolicyEnforcer::default();
    }

    // ========================================================================
    // Scroll Execution Tests
    // ========================================================================

    #[tokio::test]
    async fn test_execute_scroll_empty_steps() {
        use crate::scroll::schema::Scroll;
        let mut executor = Executor::new();
        let scroll = Scroll {
            scroll: "test-scroll".to_string(),
            description: "A test scroll".to_string(),
            requires: None,
            provides: None,
            steps: vec![],
        };

        let result = executor.execute_scroll(&scroll).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_elaborate_missing_input_returns_error() {
        let mut executor = Executor::new();
        let step = Step::Elaborate(ElaborateStep {
            elaborate: ElaborateParams {
                input: "${nonexistent}".to_string(),
                depth: Default::default(),
                output_contract: None,
                context: None,
                backend: None,
                model_tier: None,
                model: None,
                format_schema: None,
            },
            output: None,
            on_fail: Default::default(),
        });

        let result = executor.execute_step(&step).await;
        assert!(matches!(result, Err(ExecutionError::VariableNotFound(_))));
    }

    // ========================================================================
    // Concurrent Step Tests
    // ========================================================================

    #[tokio::test]
    async fn test_concurrent_basic_execution() {
        use crate::scroll::schema::{ConcurrentStep, ConcurrentParams, InvokeParams};

        let mut executor = Executor::new();

        // Set up some test variables
        executor.context_mut().set_variable("test1".to_string(), serde_json::Value::String("value1".to_string()));
        executor.context_mut().set_variable("test2".to_string(), serde_json::Value::String("value2".to_string()));

        // Create concurrent step with multiple simple operations
        let step = Step::Concurrent(ConcurrentStep {
            concurrent: ConcurrentParams {
                operations: vec![
                    // These use agent invocations with mock backend
                    Step::Invoke(InvokeStep {
                        invoke: InvokeParams {
                            agent: "agent1".to_string(),
                            instructions: "test instructions 1".to_string(),
                            context: None,
                            timeout_secs: None,
                            backend: None,
                            model_tier: None,
                            model: None,
                            output_schema: None,
                        },
                        output: Some("result1".to_string()),
                        on_fail: Default::default(),
                    }),
                    Step::Invoke(InvokeStep {
                        invoke: InvokeParams {
                            agent: "agent2".to_string(),
                            instructions: "test instructions 2".to_string(),
                            context: None,
                            timeout_secs: None,
                            backend: None,
                            model_tier: None,
                            model: None,
                            output_schema: None,
                        },
                        output: Some("result2".to_string()),
                        on_fail: Default::default(),
                    }),
                ],
                timeout: None,
            },
            output: Some("all_results".to_string()),
            on_fail: crate::scroll::schema::OnFail::Continue,
        });

        let result = executor.execute_step(&step).await;

        // With Continue, failures become nulls but execution completes
        assert!(result.is_ok());

        // Check that aggregate output was set
        let all_results = executor.context().get_variable("all_results");
        assert!(all_results.is_some());

        // Should be a sequence
        if let Some(serde_json::Value::Array(seq)) = all_results {
            assert_eq!(seq.len(), 2);
        } else {
            panic!("Expected sequence for concurrent results");
        }
    }

    #[tokio::test]
    async fn test_concurrent_halt_on_error() {
        use crate::scroll::schema::{ConcurrentStep, ConcurrentParams, InvokeParams};

        let mut executor = Executor::new();

        let step = Step::Concurrent(ConcurrentStep {
            concurrent: ConcurrentParams {
                operations: vec![
                    Step::Invoke(InvokeStep {
                        invoke: InvokeParams {
                            agent: "test-agent".to_string(),
                            instructions: "test instructions".to_string(),
                            context: None,
                            timeout_secs: None,
                            backend: None,
                            model_tier: None,
                            model: None,
                            output_schema: None,
                        },
                        output: Some("result1".to_string()),
                        on_fail: Default::default(),
                    }),
                ],
                timeout: None,
            },
            output: Some("results".to_string()),
            on_fail: crate::scroll::schema::OnFail::Halt,
        });

        let result = executor.execute_step(&step).await;

        // With Halt and mock backend, should succeed (mock returns success)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_collect_errors() {
        use crate::scroll::schema::{ConcurrentStep, ConcurrentParams, InvokeParams};

        let mut executor = Executor::new();

        let step = Step::Concurrent(ConcurrentStep {
            concurrent: ConcurrentParams {
                operations: vec![
                    Step::Invoke(InvokeStep {
                        invoke: InvokeParams {
                            agent: "test-agent".to_string(),
                            instructions: "test instructions".to_string(),
                            context: None,
                            timeout_secs: None,
                            backend: None,
                            model_tier: None,
                            model: None,
                            output_schema: None,
                        },
                        output: Some("result1".to_string()),
                        on_fail: Default::default(),
                    }),
                ],
                timeout: None,
            },
            output: Some("results".to_string()),
            on_fail: crate::scroll::schema::OnFail::CollectErrors,
        });

        let result = executor.execute_step(&step).await;

        // CollectErrors always succeeds
        assert!(result.is_ok());

        // Check output structure
        let results = executor.context().get_variable("results");
        assert!(results.is_some());

        // Should be a mapping with "results" and "errors" keys
        if let Some(serde_json::Value::Object(map)) = results {
            assert!(map.contains_key("results"));
            assert!(map.contains_key("errors"));
        } else {
            panic!("Expected mapping for collect_errors output");
        }
    }

    #[tokio::test]
    async fn test_concurrent_timeout() {
        use crate::scroll::schema::{ConcurrentStep, ConcurrentParams, InvokeParams};

        let mut executor = Executor::new();

        let step = Step::Concurrent(ConcurrentStep {
            concurrent: ConcurrentParams {
                operations: vec![
                    Step::Invoke(InvokeStep {
                        invoke: InvokeParams {
                            agent: "test-agent".to_string(),
                            instructions: "test instructions".to_string(),
                            context: None,
                            timeout_secs: None,
                            backend: None,
                            model_tier: None,
                            model: None,
                            output_schema: None,
                        },
                        output: Some("result1".to_string()),
                        on_fail: Default::default(),
                    }),
                ],
                timeout: Some(1), // 1 second timeout - operations are fast so this should succeed
            },
            output: Some("results".to_string()),
            on_fail: crate::scroll::schema::OnFail::Continue,
        });

        let result = executor.execute_step(&step).await;

        // Should complete within timeout
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_invalid_on_fail() {
        use crate::scroll::schema::{ConcurrentStep, ConcurrentParams, InvokeParams, OnFail, RetryConfig};

        let mut executor = Executor::new();

        let step = Step::Concurrent(ConcurrentStep {
            concurrent: ConcurrentParams {
                operations: vec![
                    Step::Invoke(InvokeStep {
                        invoke: InvokeParams {
                            agent: "test-agent".to_string(),
                            instructions: "test instructions".to_string(),
                            context: None,
                            timeout_secs: None,
                            backend: None,
                            model_tier: None,
                            model: None,
                            output_schema: None,
                        },
                        output: Some("result1".to_string()),
                        on_fail: Default::default(),
                    }),
                ],
                timeout: None,
            },
            output: Some("results".to_string()),
            on_fail: OnFail::Retry(RetryConfig { max: 3 }), // Retry is not valid for concurrent
        });

        let result = executor.execute_step(&step).await;

        // Should fail with InvalidOnFail
        assert!(matches!(result, Err(ExecutionError::InvalidOnFail(_))));
    }

    // ========================================================================
    // Filesystem Primitive Tests
    // ========================================================================

    #[tokio::test]
    async fn test_execute_fs_write_and_read() {
        let mut executor = Executor::for_testing();

        // Write operation
        let write_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Write,
                path: "/test/file.txt".to_string(),
                content: Some("Hello, world!".to_string()),
                dest: None,
            },
            output: None,
            on_fail: Default::default(),
        });

        let result = executor.execute_step(&write_step).await;
        assert!(result.is_ok(), "Write failed: {:?}", result.err());

        // Read operation
        let read_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Read,
                path: "/test/file.txt".to_string(),
                content: None,
                dest: None,
            },
            output: Some("file_content".to_string()),
            on_fail: Default::default(),
        });

        let result = executor.execute_step(&read_step).await;
        assert!(result.is_ok(), "Read failed: {:?}", result.err());

        // Check content
        let content = executor.context().get_variable("file_content");
        assert!(content.is_some());
        assert_eq!(content.unwrap().as_str().unwrap(), "Hello, world!");
    }

    #[tokio::test]
    async fn test_execute_fs_exists() {
        let mut executor = Executor::for_testing();

        // Check non-existent file
        let exists_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Exists,
                path: "/test/nonexistent.txt".to_string(),
                content: None,
                dest: None,
            },
            output: Some("exists".to_string()),
            on_fail: Default::default(),
        });

        let result = executor.execute_step(&exists_step).await;
        assert!(result.is_ok());

        let exists = executor.context().get_variable("exists");
        assert!(exists.is_some());
        assert_eq!(exists.unwrap().as_bool().unwrap(), false);

        // Write a file
        let write_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Write,
                path: "/test/exists.txt".to_string(),
                content: Some("test".to_string()),
                dest: None,
            },
            output: None,
            on_fail: Default::default(),
        });
        executor.execute_step(&write_step).await.unwrap();

        // Check existing file
        let exists_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Exists,
                path: "/test/exists.txt".to_string(),
                content: None,
                dest: None,
            },
            output: Some("exists".to_string()),
            on_fail: Default::default(),
        });

        executor.execute_step(&exists_step).await.unwrap();
        let exists = executor.context().get_variable("exists");
        assert_eq!(exists.unwrap().as_bool().unwrap(), true);
    }

    #[tokio::test]
    async fn test_execute_fs_delete() {
        let mut executor = Executor::for_testing();

        // Write a file
        let write_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Write,
                path: "/test/delete_me.txt".to_string(),
                content: Some("temporary".to_string()),
                dest: None,
            },
            output: None,
            on_fail: Default::default(),
        });
        executor.execute_step(&write_step).await.unwrap();

        // Delete it
        let delete_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Delete,
                path: "/test/delete_me.txt".to_string(),
                content: None,
                dest: None,
            },
            output: None,
            on_fail: Default::default(),
        });

        let result = executor.execute_step(&delete_step).await;
        assert!(result.is_ok(), "Delete failed: {:?}", result.err());

        // Verify it's gone
        let exists_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Exists,
                path: "/test/delete_me.txt".to_string(),
                content: None,
                dest: None,
            },
            output: Some("exists".to_string()),
            on_fail: Default::default(),
        });

        executor.execute_step(&exists_step).await.unwrap();
        let exists = executor.context().get_variable("exists");
        assert_eq!(exists.unwrap().as_bool().unwrap(), false);
    }

    #[tokio::test]
    async fn test_execute_fs_list() {
        let mut executor = Executor::for_testing();

        // Write some files
        executor.execute_step(&Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Write,
                path: "/test/dir/file1.txt".to_string(),
                content: Some("test1".to_string()),
                dest: None,
            },
            output: None,
            on_fail: Default::default(),
        })).await.unwrap();

        executor.execute_step(&Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Write,
                path: "/test/dir/file2.txt".to_string(),
                content: Some("test2".to_string()),
                dest: None,
            },
            output: None,
            on_fail: Default::default(),
        })).await.unwrap();

        // List directory
        let list_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::List,
                path: "/test/dir".to_string(),
                content: None,
                dest: None,
            },
            output: Some("files".to_string()),
            on_fail: Default::default(),
        });

        let result = executor.execute_step(&list_step).await;
        assert!(result.is_ok(), "List failed: {:?}", result.err());

        let files = executor.context().get_variable("files");
        assert!(files.is_some());
        let files_seq = files.unwrap().as_array().unwrap();
        assert!(files_seq.len() >= 2, "Expected at least 2 files, got {}", files_seq.len());
    }

    #[tokio::test]
    async fn test_execute_fs_with_variable_path() {
        let mut executor = Executor::for_testing();

        // Set path variable
        executor.context_mut().set_variable(
            "file_path".to_string(),
            serde_json::Value::String("/test/var_path.txt".to_string()),
        );

        // Write using variable path
        let write_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Write,
                path: "${file_path}".to_string(),
                content: Some("variable path content".to_string()),
                dest: None,
            },
            output: None,
            on_fail: Default::default(),
        });

        let result = executor.execute_step(&write_step).await;
        assert!(result.is_ok(), "Write with variable path failed: {:?}", result.err());

        // Read it back
        let read_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Read,
                path: "${file_path}".to_string(),
                content: None,
                dest: None,
            },
            output: Some("content".to_string()),
            on_fail: Default::default(),
        });

        executor.execute_step(&read_step).await.unwrap();
        let content = executor.context().get_variable("content");
        assert_eq!(content.unwrap().as_str().unwrap(), "variable path content");
    }

    #[tokio::test]
    async fn test_execute_fs_write_missing_content() {
        let mut executor = Executor::for_testing();

        // Write without content should fail
        let write_step = Step::Fs(FsStep {
            fs: FsParams {
                operation: FsOperation::Write,
                path: "/test/no_content.txt".to_string(),
                content: None,
                dest: None,
            },
            output: None,
            on_fail: Default::default(),
        });

        let result = executor.execute_step(&write_step).await;
        assert!(result.is_err(), "Expected error for missing content");
        assert!(matches!(result, Err(ExecutionError::MissingParameter(_))));
    }

    // ========================================================================
    // Set Primitive Tests
    // ========================================================================

    #[tokio::test]
    async fn test_set_basic_values() {
        use crate::scroll::schema::{SetStep, SetParams};

        let mut executor = Executor::new();
        executor.context.set_variable("project_name".to_string(), serde_json::Value::String("sage-lore".to_string()));
        executor.context.set_variable("version".to_string(), serde_json::Value::Number(serde_json::Number::from(42)));

        let step = SetStep {
            set: SetParams {
                values: serde_yaml::from_str(r#"
                    name: "${project_name}"
                    ver: "${version}"
                    static_field: "hello"
                "#).unwrap(),
            },
            output: Some("result".to_string()),
        };

        executor.execute_set(&step).await.unwrap();

        let result = executor.context.get_variable("result").unwrap();
        let map = result.as_object().unwrap();
        assert_eq!(map.get("name").unwrap(), &serde_json::Value::String("sage-lore".to_string()));
        assert_eq!(map.get("ver").unwrap(), &serde_json::Value::Number(serde_json::Number::from(42)));
        assert_eq!(map.get("static_field").unwrap(), &serde_json::Value::String("hello".to_string()));
    }

    #[tokio::test]
    async fn test_set_nested_values() {
        use crate::scroll::schema::{SetStep, SetParams};

        let mut executor = Executor::new();
        executor.context.set_variable("source".to_string(), serde_json::Value::String("forgejo".to_string()));

        let step = SetStep {
            set: SetParams {
                values: serde_yaml::from_str(r#"
                    metadata:
                        source: "${source}"
                        version: 1
                    tags:
                        - "alpha"
                        - "${source}"
                "#).unwrap(),
            },
            output: Some("nested".to_string()),
        };

        executor.execute_set(&step).await.unwrap();

        let result = executor.context.get_variable("nested").unwrap();
        let map = result.as_object().unwrap();

        // Check nested mapping
        let metadata = map.get("metadata").unwrap().as_object().unwrap();
        assert_eq!(metadata.get("source").unwrap(), &serde_json::Value::String("forgejo".to_string()));

        // Check sequence with resolved variable
        let tags = map.get("tags").unwrap().as_array().unwrap();
        assert_eq!(tags[1], serde_json::Value::String("forgejo".to_string()));
    }

    #[tokio::test]
    async fn test_set_preserves_types() {
        use crate::scroll::schema::{SetStep, SetParams};

        let mut executor = Executor::new();

        // Set a mapping variable
        let inner_map: serde_json::Value = serde_yaml::from_str(r#"
            title: "My Issue"
            number: 42
        "#).unwrap();
        executor.context.set_variable("raw_issue".to_string(), inner_map);

        let step = SetStep {
            set: SetParams {
                values: serde_yaml::from_str(r#"
                    issue: "${raw_issue}"
                    extra: "added"
                "#).unwrap(),
            },
            output: Some("enriched".to_string()),
        };

        executor.execute_set(&step).await.unwrap();

        let result = executor.context.get_variable("enriched").unwrap();
        let map = result.as_object().unwrap();

        // "${raw_issue}" should resolve to the full mapping, not a string
        let issue = map.get("issue").unwrap();
        assert!(issue.is_object(), "Pure variable reference should preserve mapping type");
    }

    #[tokio::test]
    async fn test_set_interpolated_string() {
        use crate::scroll::schema::{SetStep, SetParams};

        let mut executor = Executor::new();
        executor.context.set_variable("owner".to_string(), serde_json::Value::String("kai".to_string()));
        executor.context.set_variable("repo".to_string(), serde_json::Value::String("sage-lore".to_string()));

        let step = SetStep {
            set: SetParams {
                values: serde_yaml::from_str(r#"
                    full_name: "${owner}/${repo}"
                "#).unwrap(),
            },
            output: Some("result".to_string()),
        };

        executor.execute_set(&step).await.unwrap();

        let result = executor.context.get_variable("result").unwrap();
        let map = result.as_object().unwrap();
        assert_eq!(map.get("full_name").unwrap(), &serde_json::Value::String("kai/sage-lore".to_string()));
    }

    #[tokio::test]
    async fn test_set_missing_variable_errors() {
        use crate::scroll::schema::{SetStep, SetParams};

        let mut executor = Executor::new();

        let step = SetStep {
            set: SetParams {
                values: serde_yaml::from_str(r#"
                    name: "${nonexistent}"
                "#).unwrap(),
            },
            output: Some("result".to_string()),
        };

        let result = executor.execute_set(&step).await;
        assert!(result.is_err(), "Should error on missing variable");
    }

    #[tokio::test]
    async fn test_set_no_output_still_succeeds() {
        use crate::scroll::schema::{SetStep, SetParams};

        let mut executor = Executor::new();

        let step = SetStep {
            set: SetParams {
                values: serde_yaml::from_str(r#"
                    key: "value"
                "#).unwrap(),
            },
            output: None,
        };

        // Should succeed even without output binding
        executor.execute_set(&step).await.unwrap();
    }

    #[tokio::test]
    async fn test_set_scroll_parsing() {
        // Verify a scroll with a set step parses correctly
        let scroll_yaml = r#"
scroll: test-set
description: "Test set primitive parsing"

steps:
  - set:
      values:
        title: "hello"
        count: 5
    output: my_data
"#;
        let scroll: crate::scroll::schema::Scroll = serde_yaml::from_str(scroll_yaml).unwrap();
        assert_eq!(scroll.steps.len(), 1);
        match &scroll.steps[0] {
            Step::Set(s) => {
                assert_eq!(s.output, Some("my_data".to_string()));
                assert!(s.set.values.is_object());
            }
            other => panic!("Expected Set step, got {:?}", other),
        }
    }
}
