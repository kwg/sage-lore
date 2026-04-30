// SPDX-License-Identifier: MIT
//! End-to-end test for the `string.*` builtin namespace dispatched through
//! the Scroll Assembly pipeline (#190).

use sage_lore::scroll::assembly::{dispatch, parser, typechecker};
use sage_lore::scroll::executor::Executor;
use serde_json::json;
use std::collections::HashMap;

#[tokio::test]
async fn string_primitives_through_assembly_pipeline() {
    let source = std::fs::read_to_string("tests/fixtures/string_primitive.scroll")
        .expect("read fixture");
    let ast = parser::parse(&source, "string_primitive.scroll")
        .expect("parse fixture");

    let diags = typechecker::check(&ast, "string_primitive.scroll");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.severity, parser::Severity::Error))
        .collect();
    assert!(errors.is_empty(), "type-check errors: {errors:?}");

    let mut executor = Executor::for_testing();
    let mut inputs = HashMap::new();
    inputs.insert("code".to_string(), json!("alpha\nbeta\ngamma"));

    let outputs = dispatch::execute(&ast, &mut executor, inputs)
        .await
        .expect("scroll executes");

    assert_eq!(outputs.get("first"), Some(&json!("alpha")));
    assert_eq!(outputs.get("parts"), Some(&json!(["alpha", "beta", "gamma"])));
    assert_eq!(outputs.get("upper_first"), Some(&json!("ALPHA")));
}
