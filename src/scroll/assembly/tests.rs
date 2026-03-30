// SPDX-License-Identifier: MIT
//! Tests for the Scroll Assembly grammar.
//!
//! S1 tests verify that the pest grammar can parse all test scrolls
//! and correctly rejects malformed input. AST construction is S2.

use crate::scroll::assembly::ast::*;
use crate::scroll::assembly::dispatch;
use crate::scroll::assembly::grammar::{Rule, ScrollAssemblyParser};
use crate::scroll::assembly::parser::{self, Severity};
use crate::scroll::assembly::typechecker;
use pest::Parser;

// ============================================================================
// Helper
// ============================================================================

fn parse_scroll(input: &str) -> Result<(), String> {
    ScrollAssemblyParser::parse(Rule::scroll_file, input)
        .map(|_| ())
        .map_err(|e| format!("{}", e))
}

fn assert_parses(name: &str, input: &str) {
    match parse_scroll(input) {
        Ok(()) => {}
        Err(e) => panic!("Failed to parse {name}:\n{e}"),
    }
}

fn assert_fails(name: &str, input: &str) {
    if parse_scroll(input).is_ok() {
        panic!("{name} should have failed to parse but succeeded");
    }
}

// ============================================================================
// Corpus Tests — each test scroll file must parse
// ============================================================================

#[test]
fn parse_test_basic() {
    assert_parses("test_basic", include_str!("../../../tests/scroll_corpus/test_basic.scroll"));
}

#[test]
fn parse_test_types() {
    assert_parses("test_types", include_str!("../../../tests/scroll_corpus/test_types.scroll"));
}

#[test]
fn parse_test_binding() {
    assert_parses("test_binding", include_str!("../../../tests/scroll_corpus/test_binding.scroll"));
}

#[test]
fn parse_test_strings() {
    assert_parses("test_strings", include_str!("../../../tests/scroll_corpus/test_strings.scroll"));
}

#[test]
fn parse_test_control_flow() {
    assert_parses("test_control_flow", include_str!("../../../tests/scroll_corpus/test_control_flow.scroll"));
}

#[test]
fn parse_test_expressions() {
    assert_parses("test_expressions", include_str!("../../../tests/scroll_corpus/test_expressions.scroll"));
}

#[test]
fn parse_test_error_handling() {
    assert_parses("test_error_handling", include_str!("../../../tests/scroll_corpus/test_error_handling.scroll"));
}

#[test]
fn parse_test_primitives() {
    assert_parses("test_primitives", include_str!("../../../tests/scroll_corpus/test_primitives.scroll"));
}

#[test]
fn parse_test_concurrent() {
    assert_parses("test_concurrent", include_str!("../../../tests/scroll_corpus/test_concurrent.scroll"));
}

#[test]
fn parse_test_scoping() {
    assert_parses("test_scoping", include_str!("../../../tests/scroll_corpus/test_scoping.scroll"));
}

#[test]
fn parse_test_scroll_structure() {
    assert_parses("test_scroll_structure", include_str!("../../../tests/scroll_corpus/test_scroll_structure.scroll"));
}

#[test]
fn parse_test_operators() {
    assert_parses("test_operators", include_str!("../../../tests/scroll_corpus/test_operators.scroll"));
}

#[test]
fn parse_test_map_merge() {
    assert_parses("test_map_merge", include_str!("../../../tests/scroll_corpus/test_map_merge.scroll"));
}

#[test]
fn parse_test_run() {
    assert_parses("test_run", include_str!("../../../tests/scroll_corpus/test_run.scroll"));
}

#[test]
fn parse_test_full_example() {
    assert_parses("test_full_example", include_str!("../../../tests/scroll_corpus/test_full_example.scroll"));
}

#[test]
fn parse_test_match_complex() {
    assert_parses("test_match_complex", include_str!("../../../tests/scroll_corpus/test_match_complex.scroll"));
}

#[test]
fn parse_test_inline_provide() {
    assert_parses("test_inline_provide", include_str!("../../../tests/scroll_corpus/test_inline_provide.scroll"));
}

// ============================================================================
// Negative Tests — malformed input must fail
// ============================================================================

#[test]
fn reject_missing_semicolon() {
    assert_fails("missing_semicolon", include_str!("../../../tests/scroll_corpus/errors/missing_semicolon.scroll"));
}

#[test]
fn reject_unclosed_brace() {
    assert_fails("unclosed_brace", include_str!("../../../tests/scroll_corpus/errors/unclosed_brace.scroll"));
}

#[test]
fn reject_bad_type_def() {
    assert_fails("bad_type_def", include_str!("../../../tests/scroll_corpus/errors/bad_type_def.scroll"));
}

#[test]
fn reject_missing_type_annotation() {
    assert_fails("missing_type_annotation", include_str!("../../../tests/scroll_corpus/errors/missing_type_annotation.scroll"));
}

#[test]
fn reject_keyword_as_identifier() {
    assert_fails("keyword_as_identifier", include_str!("../../../tests/scroll_corpus/errors/keyword_as_identifier.scroll"));
}

// ============================================================================
// Unit Tests — specific grammar rules
// ============================================================================

#[test]
fn parse_type_ref_primitive() {
    let cases = ["str", "int", "float", "bool", "map"];
    for case in cases {
        ScrollAssemblyParser::parse(Rule::type_ref, case)
            .unwrap_or_else(|e| panic!("Failed to parse type_ref '{case}': {e}"));
    }
}

#[test]
fn parse_type_ref_array() {
    let cases = ["str[]", "int[]", "Story[]"];
    for case in cases {
        ScrollAssemblyParser::parse(Rule::type_ref, case)
            .unwrap_or_else(|e| panic!("Failed to parse type_ref '{case}': {e}"));
    }
}

#[test]
fn parse_type_ref_nullable() {
    let cases = ["str?", "int?", "Story?", "str[]?"];
    for case in cases {
        ScrollAssemblyParser::parse(Rule::type_ref, case)
            .unwrap_or_else(|e| panic!("Failed to parse type_ref '{case}': {e}"));
    }
}

#[test]
fn parse_identifier_accepts_valid() {
    let cases = ["foo", "bar_baz", "x1", "_private", "camelCase"];
    for case in cases {
        ScrollAssemblyParser::parse(Rule::identifier, case)
            .unwrap_or_else(|e| panic!("Failed to parse identifier '{case}': {e}"));
    }
}

#[test]
fn parse_identifier_rejects_keywords() {
    let keywords = ["set", "type", "scroll", "if", "else", "for", "in",
                     "while", "match", "break", "concurrent", "true", "false",
                     "null", "continue", "retry", "fallback", "description",
                     "str", "int", "float", "bool", "map"];
    for kw in keywords {
        assert!(
            ScrollAssemblyParser::parse(Rule::identifier, kw).is_err(),
            "Keyword '{kw}' should not parse as identifier"
        );
    }
}

#[test]
fn parse_identifier_allows_keyword_prefixes() {
    // Words that START with a keyword but are not keywords themselves
    let cases = ["setup", "format", "internal", "matching", "break_point",
                 "for_each", "set_value", "type_name", "scroll_path"];
    for case in cases {
        ScrollAssemblyParser::parse(Rule::identifier, case)
            .unwrap_or_else(|e| panic!("Failed to parse identifier '{case}': {e}"));
    }
}

#[test]
fn parse_type_name() {
    let cases = ["Story", "File", "ChunkExtraction", "A", "MyType123"];
    for case in cases {
        ScrollAssemblyParser::parse(Rule::type_name, case)
            .unwrap_or_else(|e| panic!("Failed to parse type_name '{case}': {e}"));
    }
}

#[test]
fn parse_string_lit_simple() {
    let input = r#""hello world""#;
    ScrollAssemblyParser::parse(Rule::string_lit, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_string_lit_interpolation() {
    let input = r#""Hello {name}, you have {count} items""#;
    ScrollAssemblyParser::parse(Rule::string_lit, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_string_lit_escape() {
    let input = r#""line one\nline two\ttab""#;
    ScrollAssemblyParser::parse(Rule::string_lit, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_raw_string() {
    let input = r#"`literal {braces} and \no escapes`"#;
    ScrollAssemblyParser::parse(Rule::raw_string_lit, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_binary_ops() {
    let cases = [
        "1 + 2",
        "a - b",
        "x == y",
        "x != y",
        "a > b",
        "a < b",
        "a >= b",
        "a <= b",
        "x && y",
        "x || y",
        "a ++ b",
    ];
    for case in cases {
        ScrollAssemblyParser::parse(Rule::expression, case)
            .unwrap_or_else(|e| panic!("Failed to parse expression '{case}': {e}"));
    }
}

#[test]
fn parse_expression_field_access() {
    let input = "issue.title";
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_method_call() {
    let input = "platform.get_issue(number: 42)";
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_call_with_config() {
    let input = r#"invoke(agent: "dev", instructions: "test") { schema: Review, tier: premium }"#;
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_null_coalesce() {
    let input = "maybe_value ?? default_value";
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_struct_literal() {
    let input = r#"Story { number: 1, title: "test" }"#;
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_array_literal() {
    let input = r#"["a", "b", "c"]"#;
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_if_as_expr() {
    let input = r#"if x > 0 { "positive" } else { "non-positive" }"#;
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_unary_not() {
    let input = "!active";
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_ternary() {
    let input = r#"count > 0 ? "positive" : "non-positive""#;
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_ternary_nested() {
    let input = r#"a > b ? a > c ? "a wins" : "c wins" : "b wins""#;
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_expression_mixed_precedence() {
    let input = "a + b == c && d || e";
    ScrollAssemblyParser::parse(Rule::expression, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_minimal_scroll() {
    let input = r#"scroll "minimal" {
    description: "Minimal test";
    require x: int;
    provide y: int;
    set y: int = x;
}"#;
    assert_parses("minimal", input);
}

#[test]
fn parse_enum_type() {
    let input = "type Status { open, closed, merged }";
    ScrollAssemblyParser::parse(Rule::type_def, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

#[test]
fn parse_struct_type() {
    let input = "type Story { number: int, title: str, tags: str[] }";
    ScrollAssemblyParser::parse(Rule::type_def, input)
        .unwrap_or_else(|e| panic!("Failed: {e}"));
}

// ============================================================================
// S2 Parser Tests — pest pairs → AST
// ============================================================================

fn parse_to_ast(input: &str) -> ScrollFile {
    parser::parse(input, "test.scroll").unwrap_or_else(|diags| {
        let msgs: Vec<_> = diags.iter().map(|d| d.to_string()).collect();
        panic!("Parse failed:\n{}", msgs.join("\n"));
    })
}

#[test]
fn ast_minimal_scroll() {
    let ast = parse_to_ast(r#"scroll "minimal" {
    description: "A test";
    require x: int;
    provide y: int;
    set y: int = x;
}"#);
    assert_eq!(ast.scroll.name, "minimal");
    assert_eq!(ast.scroll.description.as_deref(), Some("A test"));
    assert_eq!(ast.scroll.requires.len(), 1);
    assert_eq!(ast.scroll.requires[0].name, "x");
    assert_eq!(ast.scroll.provides.len(), 1);
    assert_eq!(ast.scroll.provides[0].name, "y");
    assert_eq!(ast.scroll.body.statements.len(), 1);
    assert!(matches!(&ast.scroll.body.statements[0], Statement::SetDecl(_)));
}

#[test]
fn ast_type_defs() {
    let ast = parse_to_ast(r#"
type Story { number: int, title: str }
type Status { open, closed, merged }
scroll "test" {
    description: "Types test";
    require x: int;
    provide y: int;
    set y: int = 0;
}"#);
    assert_eq!(ast.type_defs.len(), 2);
    match &ast.type_defs[0] {
        TypeDef::Struct(s) => {
            assert_eq!(s.name, "Story");
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.fields[0].name, "number");
            assert_eq!(s.fields[1].name, "title");
        }
        _ => panic!("Expected struct"),
    }
    match &ast.type_defs[1] {
        TypeDef::Enum(e) => {
            assert_eq!(e.name, "Status");
            assert_eq!(e.variants, vec!["open", "closed", "merged"]);
        }
        _ => panic!("Expected enum"),
    }
}

#[test]
fn ast_set_decl() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Set test";
    require x: int;
    provide y: int;
    set count: int = 42;
    set name: str = "hello";
    set y: int = count;
}"#);
    assert_eq!(ast.scroll.body.statements.len(), 3);
    if let Statement::SetDecl(sd) = &ast.scroll.body.statements[0] {
        assert_eq!(sd.name, "count");
        assert!(matches!(sd.type_ref.base, TypeBase::Primitive(PrimitiveType::Int)));
        assert!(matches!(sd.value.kind, ExprKind::IntLit(42)));
    } else {
        panic!("Expected SetDecl");
    }
}

#[test]
fn ast_binding() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Binding test";
    require n: int;
    provide result: map;
    platform.get_issue(number: n) -> result: map;
}"#);
    assert_eq!(ast.scroll.body.statements.len(), 1);
    if let Statement::Binding(b) = &ast.scroll.body.statements[0] {
        assert_eq!(b.name, "result");
        assert!(matches!(b.type_ref.base, TypeBase::Primitive(PrimitiveType::Map)));
        if let ExprKind::Call { target, args, .. } = &b.source.kind {
            if let ExprKind::FieldAccess { object, field } = &target.kind {
                assert!(matches!(&object.kind, ExprKind::Identifier(name) if name == "platform"));
                assert_eq!(field, "get_issue");
            } else {
                panic!("Expected field access on call target");
            }
            assert_eq!(args.len(), 1);
        } else {
            panic!("Expected Call");
        }
    } else {
        panic!("Expected Binding");
    }
}

#[test]
fn ast_assignment() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Assignment test";
    require x: int;
    provide y: int;
    set y: int = 0;
    y = x + 1;
    y += 5;
}"#);
    assert_eq!(ast.scroll.body.statements.len(), 3);
    if let Statement::Assignment(a) = &ast.scroll.body.statements[1] {
        assert_eq!(a.target, "y");
        assert_eq!(a.op, AssignOp::Assign);
    } else {
        panic!("Expected Assignment");
    }
    if let Statement::Assignment(a) = &ast.scroll.body.statements[2] {
        assert_eq!(a.target, "y");
        assert_eq!(a.op, AssignOp::AddAssign);
    } else {
        panic!("Expected AddAssign");
    }
}

#[test]
fn ast_error_chain() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Error chain test";
    require n: int;
    provide result: map;
    invoke(agent: "dev") { schema: Result } -> result: map | retry(3) | continue;
}"#);
    if let Statement::Binding(b) = &ast.scroll.body.statements[0] {
        assert_eq!(b.error_chain.len(), 2);
        assert!(matches!(b.error_chain[0], ErrorHandler::Retry(3)));
        assert!(matches!(b.error_chain[1], ErrorHandler::Continue));
    } else {
        panic!("Expected Binding");
    }
}

#[test]
fn ast_if_expression() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "If test";
    require x: int;
    provide label: str;
    set label: str = if x > 0 { "positive" } else { "non-positive" };
}"#);
    if let Statement::SetDecl(sd) = &ast.scroll.body.statements[0] {
        assert!(matches!(sd.value.kind, ExprKind::If { .. }));
    } else {
        panic!("Expected SetDecl with if expression");
    }
}

#[test]
fn ast_for_expression() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "For test";
    require items: int[];
    provide results: int[];
    set results: int[] = for item in items {
        item
    };
}"#);
    if let Statement::SetDecl(sd) = &ast.scroll.body.statements[0] {
        if let ExprKind::For { binding, .. } = &sd.value.kind {
            assert_eq!(binding, "item");
        } else {
            panic!("Expected For expression");
        }
    } else {
        panic!("Expected SetDecl");
    }
}

#[test]
fn ast_match_expression() {
    let ast = parse_to_ast(r#"
type Status { open, closed }
scroll "test" {
    description: "Match test";
    require status: Status;
    provide label: str;
    set label: str = match status {
        Status.open => "active",
        Status.closed => "done",
    };
}"#);
    if let Statement::SetDecl(sd) = &ast.scroll.body.statements[0] {
        if let ExprKind::Match { arms, .. } = &sd.value.kind {
            assert_eq!(arms.len(), 2);
        } else {
            panic!("Expected Match expression");
        }
    } else {
        panic!("Expected SetDecl");
    }
}

#[test]
fn ast_string_interpolation() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "String test";
    require name: str;
    provide msg: str;
    set msg: str = "Hello {name}!";
}"#);
    if let Statement::SetDecl(sd) = &ast.scroll.body.statements[0] {
        if let ExprKind::StringLit(segments) = &sd.value.kind {
            assert_eq!(segments.len(), 3); // "Hello ", interpolation(name), "!"
            assert!(matches!(&segments[0], StringSegment::Literal(s) if s == "Hello "));
            assert!(matches!(&segments[1], StringSegment::Interpolation(_)));
            assert!(matches!(&segments[2], StringSegment::Literal(s) if s == "!"));
        } else {
            panic!("Expected StringLit");
        }
    } else {
        panic!("Expected SetDecl");
    }
}

#[test]
fn ast_inline_provide() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Inline provide test";
    require x: int;
    provide result: Result {
        value: str,
        count: int,
    };
    set result: Result = Result { value: "ok", count: x };
}"#);
    assert_eq!(ast.scroll.provides.len(), 1);
    assert!(ast.scroll.provides[0].inline_struct.is_some());
    let fields = ast.scroll.provides[0].inline_struct.as_ref().unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name, "value");
    assert_eq!(fields[1].name, "count");
}

#[test]
fn ast_concurrent_block() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Concurrent test";
    require a: int;
    require b: int;
    provide x: map;
    concurrent {
        platform.get_issue(number: a) -> issue_a: map;
        platform.get_issue(number: b) -> issue_b: map;
    };
    set x: map = { done: true };
}"#);
    assert!(ast.scroll.body.statements.len() >= 2);
    assert!(matches!(&ast.scroll.body.statements[0], Statement::BlockExpr(e) if matches!(e.kind, ExprKind::ConcurrentBlock { .. })));
}

#[test]
fn ast_tail_expression() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Tail expr test";
    require x: int;
    provide y: int;
    set y: int = if x > 0 {
        set temp: int = x + 1;
        temp
    } else {
        0
    };
}"#);
    if let Statement::SetDecl(sd) = &ast.scroll.body.statements[0] {
        if let ExprKind::If { then_body, .. } = &sd.value.kind {
            assert_eq!(then_body.statements.len(), 1); // set temp
            assert!(then_body.tail_expr.is_some()); // temp
        } else {
            panic!("Expected If");
        }
    } else {
        panic!("Expected SetDecl");
    }
}

#[test]
fn ast_all_corpus_files_parse_to_ast() {
    let files = [
        ("test_basic", include_str!("../../../tests/scroll_corpus/test_basic.scroll")),
        ("test_types", include_str!("../../../tests/scroll_corpus/test_types.scroll")),
        ("test_binding", include_str!("../../../tests/scroll_corpus/test_binding.scroll")),
        ("test_strings", include_str!("../../../tests/scroll_corpus/test_strings.scroll")),
        ("test_control_flow", include_str!("../../../tests/scroll_corpus/test_control_flow.scroll")),
        ("test_expressions", include_str!("../../../tests/scroll_corpus/test_expressions.scroll")),
        ("test_error_handling", include_str!("../../../tests/scroll_corpus/test_error_handling.scroll")),
        ("test_primitives", include_str!("../../../tests/scroll_corpus/test_primitives.scroll")),
        ("test_concurrent", include_str!("../../../tests/scroll_corpus/test_concurrent.scroll")),
        ("test_scoping", include_str!("../../../tests/scroll_corpus/test_scoping.scroll")),
        ("test_scroll_structure", include_str!("../../../tests/scroll_corpus/test_scroll_structure.scroll")),
        ("test_operators", include_str!("../../../tests/scroll_corpus/test_operators.scroll")),
        ("test_map_merge", include_str!("../../../tests/scroll_corpus/test_map_merge.scroll")),
        ("test_run", include_str!("../../../tests/scroll_corpus/test_run.scroll")),
        ("test_full_example", include_str!("../../../tests/scroll_corpus/test_full_example.scroll")),
        ("test_match_complex", include_str!("../../../tests/scroll_corpus/test_match_complex.scroll")),
        ("test_inline_provide", include_str!("../../../tests/scroll_corpus/test_inline_provide.scroll")),
    ];
    for (name, source) in files {
        parser::parse(source, &format!("{name}.scroll")).unwrap_or_else(|diags| {
            let msgs: Vec<_> = diags.iter().map(|d| d.to_string()).collect();
            panic!("Failed to parse {name} to AST:\n{}", msgs.join("\n"));
        });
    }
}

#[test]
fn ast_parse_error_has_diagnostics() {
    let input = r#"scroll "bad" { set x = 5; }"#;
    let result = parser::parse(input, "bad.scroll");
    assert!(result.is_err());
    let diags = result.unwrap_err();
    assert!(!diags.is_empty());
    assert_eq!(diags[0].file, "bad.scroll");
    assert!(diags[0].line > 0);
}

// ============================================================================
// S3 Type Checker Tests
// ============================================================================

fn check_scroll(input: &str) -> Vec<parser::Diagnostic> {
    let ast = parse_to_ast(input);
    typechecker::check(&ast, "test.scroll")
}

fn check_errors(input: &str) -> Vec<parser::Diagnostic> {
    check_scroll(input).into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

fn check_warnings(input: &str) -> Vec<parser::Diagnostic> {
    check_scroll(input).into_iter()
        .filter(|d| d.severity == Severity::Warning)
        .collect()
}

#[test]
fn tc_clean_scroll_no_errors() {
    let errs = check_errors(r#"
type Story { number: int, title: str }
scroll "test" {
    description: "Clean scroll";
    require n: int;
    provide story: Story;
    platform.get_issue(number: n) -> raw: map;
    story = Story { number: raw.number, title: raw.title };
}"#);
    assert!(errs.is_empty(), "Expected no errors, got: {:?}", errs);
}

#[test]
fn tc_undefined_variable() {
    let warns = check_warnings(r#"scroll "test" {
    description: "Undefined var";
    require x: int;
    provide y: int;
    set y: int = z;
}"#);
    assert!(warns.iter().any(|w| w.message.contains("possibly undefined variable: 'z'")),
        "Expected undefined var warning, got: {:?}", warns);
}

#[test]
fn tc_unknown_type_in_set() {
    let errs = check_errors(r#"scroll "test" {
    description: "Unknown type";
    require x: int;
    provide y: int;
    set val: UnknownType = x;
}"#);
    assert!(errs.iter().any(|e| e.message.contains("unknown type: 'UnknownType'")),
        "Expected unknown type error, got: {:?}", errs);
}

#[test]
fn tc_unknown_type_in_struct_lit() {
    let errs = check_errors(r#"scroll "test" {
    description: "Unknown struct type";
    require x: int;
    provide y: int;
    set y: int = 0;
    set val: map = Missing { field: x };
}"#);
    assert!(errs.iter().any(|e| e.message.contains("unknown type: 'Missing'")),
        "Expected unknown type error, got: {:?}", errs);
}

#[test]
fn tc_duplicate_type() {
    let errs = check_errors(r#"
type Foo { x: int }
type Foo { y: str }
scroll "test" {
    description: "Duplicate type";
    require x: int;
    provide y: int;
    set y: int = 0;
}"#);
    assert!(errs.iter().any(|e| e.message.contains("duplicate type definition: 'Foo'")),
        "Expected duplicate type error, got: {:?}", errs);
}

#[test]
fn tc_match_exhaustiveness() {
    let errs = check_errors(r#"
type Status { open, closed, merged }
scroll "test" {
    description: "Non-exhaustive match";
    require status: Status;
    provide label: str;
    label = match status {
        Status.open => "active",
        Status.closed => "done",
    };
}"#);
    assert!(errs.iter().any(|e| e.message.contains("non-exhaustive match") && e.message.contains("merged")),
        "Expected non-exhaustive match error, got: {:?}", errs);
}

#[test]
fn tc_match_exhaustive_passes() {
    let errs = check_errors(r#"
type Status { open, closed, merged }
scroll "test" {
    description: "Exhaustive match";
    require status: Status;
    provide label: str;
    label = match status {
        Status.open => "active",
        Status.closed => "done",
        Status.merged => "merged",
    };
}"#);
    assert!(errs.is_empty(), "Expected no errors, got: {:?}", errs);
}

#[test]
fn tc_block_scoping() {
    // Variable declared inside if should not be visible outside
    let warns = check_warnings(r#"scroll "test" {
    description: "Scope test";
    require x: int;
    provide y: int;
    if x > 0 {
        set inner: int = 5;
    };
    y = inner;
}"#);
    assert!(warns.iter().any(|w| w.message.contains("possibly undefined variable: 'inner'")),
        "Expected undefined var warning for scoped variable, got: {:?}", warns);
}

#[test]
fn tc_struct_field_checking() {
    let errs = check_errors(r#"
type Story { number: int, title: str }
scroll "test" {
    description: "Field check";
    require x: int;
    provide s: Story;
    set s: Story = Story { number: x, title: "hello", bogus_field: true };
}"#);
    assert!(errs.iter().any(|e| e.message.contains("unknown field 'bogus_field'")),
        "Expected unknown field error, got: {:?}", errs);
}

#[test]
fn tc_missing_struct_field_warns() {
    let warns = check_warnings(r#"
type Story { number: int, title: str }
scroll "test" {
    description: "Missing field";
    require x: int;
    provide s: Story;
    set s: Story = Story { number: x };
}"#);
    assert!(warns.iter().any(|w| w.message.contains("missing field 'title'")),
        "Expected missing field warning, got: {:?}", warns);
}

#[test]
fn tc_inline_provide_type() {
    let errs = check_errors(r#"scroll "test" {
    description: "Inline provide";
    require x: int;
    provide result: Result {
        value: str,
        count: int,
    };
    result = Result { value: "ok", count: x };
}"#);
    assert!(errs.is_empty(), "Expected no errors, got: {:?}", errs);
}

#[test]
fn tc_all_corpus_type_check_subset() {
    let files = [
        ("test_basic", include_str!("../../../tests/scroll_corpus/test_basic.scroll")),
        ("test_types", include_str!("../../../tests/scroll_corpus/test_types.scroll")),
        ("test_full_example", include_str!("../../../tests/scroll_corpus/test_full_example.scroll")),
        ("test_inline_provide", include_str!("../../../tests/scroll_corpus/test_inline_provide.scroll")),
    ];
    for (name, source) in files {
        let ast = parser::parse(source, &format!("{name}.scroll")).unwrap();
        let errs: Vec<_> = typechecker::check(&ast, &format!("{name}.scroll"))
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errs.is_empty(), "Type check errors in {name}: {:?}", errs);
    }
}

// ============================================================================
// S4 Dispatch Tests — AST execution
// ============================================================================

#[tokio::test]
async fn dispatch_set_and_provide() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Set and provide";
    require x: int;
    provide y: int;
    set y: int = x + 1;
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("x".to_string(), serde_json::json!(5));
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await.unwrap();
    assert_eq!(outputs.get("y"), Some(&serde_json::json!(6)));
}

#[tokio::test]
async fn dispatch_if_expression() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "If expression";
    require x: int;
    provide label: str;
    label = if x > 0 { "positive" } else { "negative" };
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("x".to_string(), serde_json::json!(5));
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await.unwrap();
    assert_eq!(outputs.get("label"), Some(&serde_json::json!("positive")));
}

#[tokio::test]
async fn dispatch_for_expression() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "For expression";
    require items: int[];
    provide results: int[];
    set results: int[] = for item in items {
        item + 1
    };
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("items".to_string(), serde_json::json!([1, 2, 3]));
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await.unwrap();
    assert_eq!(outputs.get("results"), Some(&serde_json::json!([2, 3, 4])));
}

#[tokio::test]
async fn dispatch_while_with_break() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "While with break";
    require limit: int;
    provide count: int;
    set count: int = 0;
    while count < limit {
        count += 1;
        if count == 3 { break; };
    };
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("limit".to_string(), serde_json::json!(10));
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await.unwrap();
    assert_eq!(outputs.get("count"), Some(&serde_json::json!(3)));
}

#[tokio::test]
async fn dispatch_string_interpolation() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "String interpolation";
    require name: str;
    provide greeting: str;
    set greeting: str = "Hello {name}!";
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("name".to_string(), serde_json::json!("Kai"));
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await.unwrap();
    assert_eq!(outputs.get("greeting"), Some(&serde_json::json!("Hello Kai!")));
}

#[tokio::test]
async fn dispatch_map_merge() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Map merge";
    require extra: map;
    provide result: map;
    set base: map = { x: 1, y: 2 };
    set result: map = base + extra;
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("extra".to_string(), serde_json::json!({"y": 99, "z": 3}));
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await.unwrap();
    let result = outputs.get("result").unwrap();
    assert_eq!(result["x"], 1);
    assert_eq!(result["y"], 99);
    assert_eq!(result["z"], 3);
}

#[tokio::test]
async fn dispatch_null_coalesce() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Null coalesce";
    require maybe: str?;
    provide result: str;
    set result: str = maybe ?? "default";
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("maybe".to_string(), serde_json::Value::Null);
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await.unwrap();
    assert_eq!(outputs.get("result"), Some(&serde_json::json!("default")));
}

#[tokio::test]
async fn dispatch_array_append() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Array append";
    require x: str;
    provide items: str[];
    set items: str[] = ["first"];
    items ++= x;
    items ++= "third";
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("x".to_string(), serde_json::json!("second"));
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await.unwrap();
    assert_eq!(outputs.get("items"), Some(&serde_json::json!(["first", "second", "third"])));
}

#[tokio::test]
async fn dispatch_match_expression() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Match expression";
    require mode: str;
    provide timeout: int;
    set timeout: int = match mode {
        "fast" => 60,
        "slow" => 600,
    };
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let mut inputs = std::collections::HashMap::new();
    inputs.insert("mode".to_string(), serde_json::json!("fast"));
    let outputs = dispatch::execute(&ast, &mut executor, inputs).await.unwrap();
    assert_eq!(outputs.get("timeout"), Some(&serde_json::json!(60)));
}

#[tokio::test]
async fn dispatch_missing_require_errors() {
    let ast = parse_to_ast(r#"scroll "test" {
    description: "Missing require";
    require x: int;
    provide y: int;
    set y: int = x;
}"#);
    let mut executor = crate::scroll::executor::Executor::for_testing();
    let inputs = std::collections::HashMap::new();
    let result = dispatch::execute(&ast, &mut executor, inputs).await;
    assert!(result.is_err());
}
