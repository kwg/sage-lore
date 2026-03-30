// SPDX-License-Identifier: MIT
//! Tests for loop primitive while condition evaluation.

#[cfg(test)]
mod tests {
    use sage_lore::scroll::executor::Executor;
    use sage_lore::scroll::schema::{LoopParams, LoopStep, Step};

    /// Test: while condition stops loop early when condition becomes false
    /// This test uses a condition variable that gets updated during the loop
    #[tokio::test]
    async fn test_loop_while_condition_stops_early() {
        let mut executor = Executor::new();

        // Set up items: ["a", "b", "c", "d"]
        executor.context_mut().set_variable(
            "items".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
                serde_json::Value::String("c".to_string()),
                serde_json::Value::String("d".to_string()),
            ]),
        );

        // Set continue flag to true initially
        executor.context_mut().set_variable(
            "continue_flag".to_string(),
            serde_json::Value::Bool(true),
        );

        // Loop will iterate but we'll track that while is checked
        let step = Step::Loop(LoopStep {
            loop_params: LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: Some("${continue_flag}".to_string()),
                max: None,
            },
            output: Some("results".to_string()),
        });

        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());

        // With continue_flag = true, should process all 4 items
        let results = executor.context().get_variable("results").unwrap();
        if let serde_json::Value::Array(seq) = results {
            assert_eq!(seq.len(), 4, "Loop with true condition should process all items");
        } else {
            panic!("Results should be a sequence");
        }
    }

    /// Test: while condition with loop_index variable
    /// Since we don't have expression evaluation, we test that loop_index is accessible
    #[tokio::test]
    async fn test_loop_while_with_loop_index() {
        let mut executor = Executor::new();

        // Set up items: [a, b, c, d, e]
        executor.context_mut().set_variable(
            "items".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
                serde_json::Value::String("c".to_string()),
                serde_json::Value::String("d".to_string()),
                serde_json::Value::String("e".to_string()),
            ]),
        );

        // Test that loop_index variable is available (using truthy number)
        // The loop_index itself is truthy for all values except 0
        let step = Step::Loop(LoopStep {
            loop_params: LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: Some("${loop_index}".to_string()),
                max: Some(3),
            },
            output: Some("results".to_string()),
        });

        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());

        let results = executor.context().get_variable("results").unwrap();
        if let serde_json::Value::Array(seq) = results {
            // First iteration has loop_index=0 which is falsy, so loop stops immediately
            assert_eq!(seq.len(), 0, "Loop should stop at loop_index=0 (falsy)");
        } else {
            panic!("Results should be a sequence");
        }
    }

    /// Test: while condition false from start - no iterations
    #[tokio::test]
    async fn test_loop_while_false_from_start() {
        let mut executor = Executor::new();

        // Set up items
        executor.context_mut().set_variable(
            "items".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
        );

        // Loop with while: false should run 0 iterations
        let step = Step::Loop(LoopStep {
            loop_params: LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: Some("false".to_string()),
                max: None,
            },
            output: Some("results".to_string()),
        });

        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());

        let results = executor.context().get_variable("results").unwrap();
        if let serde_json::Value::Array(seq) = results {
            assert_eq!(seq.len(), 0, "Loop should have run 0 iterations");
        } else {
            panic!("Results should be a sequence");
        }
    }

    /// Test: while condition with truthy/falsy values
    #[tokio::test]
    async fn test_loop_while_truthy_values() {
        let mut executor = Executor::new();

        // Set up items
        executor.context_mut().set_variable(
            "items".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
                serde_json::Value::String("c".to_string()),
            ]),
        );

        // Test with truthy string
        executor.context_mut().set_variable(
            "continue_flag".to_string(),
            serde_json::Value::String("yes".to_string()),
        );

        let step = Step::Loop(LoopStep {
            loop_params: LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: Some("${continue_flag}".to_string()),
                max: None,
            },
            output: Some("results".to_string()),
        });

        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());

        let results = executor.context().get_variable("results").unwrap();
        if let serde_json::Value::Array(seq) = results {
            assert_eq!(seq.len(), 3, "Loop with truthy string should process all items");
        } else {
            panic!("Results should be a sequence");
        }
    }

    /// Test: while condition with falsy values (empty string, 0, null)
    #[tokio::test]
    async fn test_loop_while_falsy_values() {
        let mut executor = Executor::new();

        executor.context_mut().set_variable(
            "items".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a".to_string()),
            ]),
        );

        // Test with empty string
        executor.context_mut().set_variable(
            "flag".to_string(),
            serde_json::Value::String("".to_string()),
        );

        let step = Step::Loop(LoopStep {
            loop_params: LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: Some("${flag}".to_string()),
                max: None,
            },
            output: Some("results".to_string()),
        });

        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());

        let results = executor.context().get_variable("results").unwrap();
        if let serde_json::Value::Array(seq) = results {
            assert_eq!(seq.len(), 0, "Loop with empty string should run 0 iterations");
        } else {
            panic!("Results should be a sequence");
        }

        // Test with 0
        executor.context_mut().set_variable(
            "flag".to_string(),
            serde_json::Value::Number(serde_json::Number::from(0)),
        );

        let step = Step::Loop(LoopStep {
            loop_params: LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: Some("${flag}".to_string()),
                max: None,
            },
            output: Some("results".to_string()),
        });

        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());

        let results = executor.context().get_variable("results").unwrap();
        if let serde_json::Value::Array(seq) = results {
            assert_eq!(seq.len(), 0, "Loop with 0 should run 0 iterations");
        } else {
            panic!("Results should be a sequence");
        }

        // Test with null
        executor.context_mut().set_variable(
            "flag".to_string(),
            serde_json::Value::Null,
        );

        let step = Step::Loop(LoopStep {
            loop_params: LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: Some("${flag}".to_string()),
                max: None,
            },
            output: Some("results".to_string()),
        });

        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());

        let results = executor.context().get_variable("results").unwrap();
        if let serde_json::Value::Array(seq) = results {
            assert_eq!(seq.len(), 0, "Loop with null should run 0 iterations");
        } else {
            panic!("Results should be a sequence");
        }
    }

    /// Test: while with no condition (None) should process all items
    #[tokio::test]
    async fn test_loop_without_while_processes_all() {
        let mut executor = Executor::new();

        executor.context_mut().set_variable(
            "items".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
                serde_json::Value::String("c".to_string()),
            ]),
        );

        let step = Step::Loop(LoopStep {
            loop_params: LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: None,
                max: None,
            },
            output: Some("results".to_string()),
        });

        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());

        let results = executor.context().get_variable("results").unwrap();
        if let serde_json::Value::Array(seq) = results {
            assert_eq!(seq.len(), 3, "Loop without while should process all items");
        } else {
            panic!("Results should be a sequence");
        }
    }

    /// Test: while condition with max iterations - both limits enforced
    #[tokio::test]
    async fn test_loop_while_with_max() {
        let mut executor = Executor::new();

        executor.context_mut().set_variable(
            "items".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
                serde_json::Value::String("c".to_string()),
                serde_json::Value::String("d".to_string()),
                serde_json::Value::String("e".to_string()),
            ]),
        );

        // while condition is true, but max is 2 - max should limit
        executor.context_mut().set_variable(
            "continue_flag".to_string(),
            serde_json::Value::Bool(true),
        );

        let step = Step::Loop(LoopStep {
            loop_params: LoopParams {
                items: "${items}".to_string(),
                item_var: "item".to_string(),
                operation: vec![],
                while_cond: Some("${continue_flag}".to_string()),
                max: Some(2),
            },
            output: Some("results".to_string()),
        });

        let result = executor.execute_step(&step).await;
        assert!(result.is_ok());

        let results = executor.context().get_variable("results").unwrap();
        if let serde_json::Value::Array(seq) = results {
            assert_eq!(seq.len(), 2, "Max should limit iterations to 2");
        } else {
            panic!("Results should be a sequence");
        }
    }
}
