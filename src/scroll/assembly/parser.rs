// SPDX-License-Identifier: MIT
//! Parser: converts pest parse output into AST nodes.
//!
//! Entry point: `parse(source, filename) -> Result<ScrollFile, Vec<Diagnostic>>`
//! Walks the pest `Pairs` tree produced by the PEG grammar and constructs
//! the typed AST defined in `ast.rs`.

use super::ast::*;
use super::grammar::{Rule, ScrollAssemblyParser};
use pest::iterators::Pair;
use pest::Parser;

// ============================================================================
// Diagnostics
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub severity: Severity,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{}:{}:{}: {}: {}", self.file, self.line, self.col, sev, self.message)
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Parse a `.scroll` source string into an AST.
/// Parse a `.scroll` source string into an AST.
///
/// Internal parser panics (from unwrap() on pest grammar children) are caught
/// and converted to diagnostics rather than crashing the process (MF1, #185).
pub fn parse(source: &str, filename: &str) -> Result<ScrollFile, Vec<Diagnostic>> {
    let pairs = ScrollAssemblyParser::parse(Rule::scroll_file, source).map_err(|e| {
        let (line, col) = match e.line_col {
            pest::error::LineColLocation::Pos((l, c)) => (l, c),
            pest::error::LineColLocation::Span((l, c), _) => (l, c),
        };
        vec![Diagnostic {
            file: filename.to_string(),
            line,
            col,
            severity: Severity::Error,
            message: format!("{e}"),
        }]
    })?;

    // Wrap AST construction in catch_unwind to convert any internal panics
    // (from grammar/parser divergence) into proper diagnostics instead of crashes.
    let filename_owned = filename.to_string();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ctx = ParseCtx { filename: filename_owned.clone() };
        let file_pair = pairs.into_iter().next().expect("scroll_file rule must match");
        ctx.parse_scroll_file(file_pair)
    }));

    match result {
        Ok(ast_result) => ast_result,
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "internal parser error (unknown panic)".to_string()
            };
            Err(vec![Diagnostic {
                file: filename.to_string(),
                line: 0,
                col: 0,
                severity: Severity::Error,
                message: format!("internal parser error: {msg}"),
            }])
        }
    }
}

// ============================================================================
// Parse Context
// ============================================================================

struct ParseCtx {
    filename: String,
}

impl ParseCtx {
    fn span_from(&self, pair: &Pair<Rule>) -> Span {
        let s = pair.as_span();
        let (line, col) = s.start_pos().line_col();
        Span { start: s.start(), end: s.end(), line, col }
    }

    fn err(&self, pair: &Pair<Rule>, msg: impl Into<String>) -> Vec<Diagnostic> {
        let span = self.span_from(pair);
        vec![Diagnostic {
            file: self.filename.clone(),
            line: span.line,
            col: span.col,
            severity: Severity::Error,
            message: msg.into(),
        }]
    }

    // ========================================================================
    // Top-Level
    // ========================================================================

    fn parse_scroll_file(&self, pair: Pair<Rule>) -> Result<ScrollFile, Vec<Diagnostic>> {
        let mut type_defs = Vec::new();
        let mut scroll = None;

        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::type_def => type_defs.push(self.parse_type_def(inner)?),
                Rule::scroll_block => scroll = Some(self.parse_scroll_block(inner)?),
                Rule::EOI => {}
                _ => {}
            }
        }

        Ok(ScrollFile {
            type_defs,
            scroll: scroll.expect("scroll_block required"),
        })
    }

    // ========================================================================
    // Type Definitions
    // ========================================================================

    fn parse_type_def(&self, pair: Pair<Rule>) -> Result<TypeDef, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();
        let name = inner.next().unwrap().as_str().to_string(); // type_name
        let body = inner.next().unwrap(); // type_body

        let body_inner = body.into_inner().next().unwrap();
        match body_inner.as_rule() {
            Rule::struct_fields => {
                let fields = self.parse_struct_fields(body_inner)?;
                Ok(TypeDef::Struct(StructDef { name, fields, span }))
            }
            Rule::enum_variants => {
                let variants = body_inner
                    .into_inner()
                    .map(|p| p.as_str().to_string())
                    .collect();
                Ok(TypeDef::Enum(EnumDef { name, variants, span }))
            }
            _ => unreachable!("type_body must be struct_fields or enum_variants"),
        }
    }

    fn parse_struct_fields(&self, pair: Pair<Rule>) -> Result<Vec<StructField>, Vec<Diagnostic>> {
        pair.into_inner()
            .map(|field_pair| {
                let span = self.span_from(&field_pair);
                let mut inner = field_pair.into_inner();
                let name = inner.next().unwrap().as_str().to_string();
                let type_ref = self.parse_type_ref(inner.next().unwrap())?;
                Ok(StructField { name, type_ref, span })
            })
            .collect()
    }

    // ========================================================================
    // Type References
    // ========================================================================

    fn parse_type_ref(&self, pair: Pair<Rule>) -> Result<TypeRef, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut is_array = false;
        let mut is_nullable = false;
        let mut base = None;

        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::prim_type => {
                    base = Some(TypeBase::Primitive(match inner.as_str() {
                        "str" => PrimitiveType::Str,
                        "int" => PrimitiveType::Int,
                        "float" => PrimitiveType::Float,
                        "bool" => PrimitiveType::Bool,
                        "map" => PrimitiveType::Map,
                        _ => unreachable!(),
                    }));
                }
                Rule::type_name => {
                    base = Some(TypeBase::Named(inner.as_str().to_string()));
                }
                Rule::array_suffix => is_array = true,
                Rule::nullable_suffix => is_nullable = true,
                _ => {}
            }
        }

        Ok(TypeRef {
            base: base.expect("type_ref must have a base type"),
            is_array,
            is_nullable,
            span,
        })
    }

    // ========================================================================
    // Scroll Block
    // ========================================================================

    fn parse_scroll_block(&self, pair: Pair<Rule>) -> Result<ScrollBlock, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let name_pair = inner.next().unwrap(); // string_lit
        let name = self.extract_string_value(&name_pair);

        let mut description = None;
        let mut requires = Vec::new();
        let mut provides = Vec::new();
        let mut body = BlockBody { statements: vec![], tail_expr: None };

        for child in inner {
            match child.as_rule() {
                Rule::description_decl => {
                    let str_pair = child.into_inner().next().unwrap();
                    description = Some(self.extract_string_value(&str_pair));
                }
                Rule::require_decl => requires.push(self.parse_require_decl(child)?),
                Rule::provide_decl => provides.push(self.parse_provide_decl(child)?),
                Rule::block_body => body = self.parse_block_body(child)?,
                _ => {}
            }
        }

        Ok(ScrollBlock { name, description, requires, provides, body, span })
    }

    fn parse_require_decl(&self, pair: Pair<Rule>) -> Result<RequireDecl, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();
        let name = inner.next().unwrap().as_str().to_string(); // identifier

        let provide_type_pair = inner.next().unwrap(); // provide_type
        let (type_ref, inline_struct) = self.parse_provide_type(provide_type_pair)?;

        let default = if let Some(expr_pair) = inner.next() {
            Some(self.parse_expression(expr_pair)?)
        } else {
            None
        };

        Ok(RequireDecl { name, type_ref, default, inline_struct, span })
    }

    fn parse_provide_decl(&self, pair: Pair<Rule>) -> Result<ProvideDecl, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();
        let name = inner.next().unwrap().as_str().to_string(); // identifier

        let provide_type_pair = inner.next().unwrap(); // provide_type
        let (type_ref, inline_struct) = self.parse_provide_type(provide_type_pair)?;

        Ok(ProvideDecl { name, type_ref, inline_struct, span })
    }

    fn parse_provide_type(
        &self,
        pair: Pair<Rule>,
    ) -> Result<(TypeRef, Option<Vec<StructField>>), Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let children: Vec<_> = pair.into_inner().collect();

        // Check if first child is type_name followed by inline_struct_def
        if let Some(first) = children.first() {
            if first.as_rule() == Rule::type_name {
                let type_name = first.as_str().to_string();
                let inline = if children.len() > 1 && children[1].as_rule() == Rule::inline_struct_def
                {
                    let struct_pair = children[1].clone();
                    let fields_pair = struct_pair.into_inner().next().unwrap();
                    Some(self.parse_struct_fields(fields_pair)?)
                } else {
                    None
                };
                let type_ref = TypeRef {
                    base: TypeBase::Named(type_name),
                    is_array: false,
                    is_nullable: false,
                    span,
                };
                return Ok((type_ref, inline));
            }
        }

        // Otherwise it's a plain type_ref
        let type_ref_pair = children.into_iter().next().unwrap();
        let type_ref = self.parse_type_ref(type_ref_pair)?;
        Ok((type_ref, None))
    }

    // ========================================================================
    // Block Body
    // ========================================================================

    fn parse_block_body(&self, pair: Pair<Rule>) -> Result<BlockBody, Vec<Diagnostic>> {
        let mut statements = Vec::new();
        let mut tail_expr = None;

        let children: Vec<_> = pair.into_inner().collect();
        let len = children.len();

        for (i, child) in children.into_iter().enumerate() {
            let is_last = i == len - 1;
            match child.as_rule() {
                Rule::set_decl => statements.push(self.parse_set_decl(child)?),
                Rule::binding_stmt => statements.push(self.parse_binding_stmt(child)?),
                Rule::assignment => statements.push(self.parse_assignment(child)?),
                Rule::break_stmt => {
                    let span = self.span_from(&child);
                    statements.push(Statement::Break(span));
                }
                Rule::block_expr => {
                    let expr = self.parse_block_expr(child)?;
                    statements.push(Statement::BlockExpr(expr));
                }
                // Expression: if it's the last item (tail_expr), it's the return value
                _ if is_last && is_expression_rule(child.as_rule()) => {
                    tail_expr = Some(Box::new(self.parse_expression(child)?));
                }
                _ if is_expression_rule(child.as_rule()) => {
                    let expr = self.parse_expression(child)?;
                    statements.push(Statement::ExprStmt(expr));
                }
                _ => {}
            }
        }

        Ok(BlockBody { statements, tail_expr })
    }

    // ========================================================================
    // Statements
    // ========================================================================

    fn parse_set_decl(&self, pair: Pair<Rule>) -> Result<Statement, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();
        let name = inner.next().unwrap().as_str().to_string();
        let type_ref = self.parse_type_ref(inner.next().unwrap())?;
        let value = self.parse_expression(inner.next().unwrap())?;
        Ok(Statement::SetDecl(SetDecl { name, type_ref, value, span }))
    }

    fn parse_binding_stmt(&self, pair: Pair<Rule>) -> Result<Statement, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let source_pair = inner.next().unwrap();
        let source = self.parse_expression(source_pair)?;

        let name = inner.next().unwrap().as_str().to_string(); // identifier
        let type_ref = self.parse_type_ref(inner.next().unwrap())?;

        let mut error_chain = Vec::new();
        for remaining in inner {
            if remaining.as_rule() == Rule::error_chain {
                error_chain = self.parse_error_chain(remaining)?;
            }
        }

        Ok(Statement::Binding(BindingStmt { source, name, type_ref, error_chain, span }))
    }

    fn parse_assignment(&self, pair: Pair<Rule>) -> Result<Statement, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();
        let target = inner.next().unwrap().as_str().to_string();
        let op_pair = inner.next().unwrap();
        let op = match op_pair.as_str() {
            "=" => AssignOp::Assign,
            "+=" => AssignOp::AddAssign,
            "-=" => AssignOp::SubAssign,
            "++=" => AssignOp::AppendAssign,
            other => unreachable!("unknown assign op: '{other}'"),
        };
        let value = self.parse_expression(inner.next().unwrap())?;
        Ok(Statement::Assignment(Assignment { target, op, value, span }))
    }

    // ========================================================================
    // Error Chain
    // ========================================================================

    fn parse_error_chain(&self, pair: Pair<Rule>) -> Result<Vec<ErrorHandler>, Vec<Diagnostic>> {
        pair.into_inner()
            .map(|handler_pair| {
                let inner = handler_pair.into_inner().next().unwrap();
                match inner.as_rule() {
                    Rule::continue_handler => Ok(ErrorHandler::Continue),
                    Rule::retry_handler => {
                        let count_str = inner.into_inner().next().unwrap().as_str();
                        let count: u32 = count_str
                            .parse()
                            .expect("retry count must be integer");
                        Ok(ErrorHandler::Retry(count))
                    }
                    Rule::fallback_handler => {
                        let body_pair = inner.into_inner().next().unwrap();
                        let body = self.parse_block_body(body_pair)?;
                        Ok(ErrorHandler::Fallback(body))
                    }
                    _ => unreachable!(),
                }
            })
            .collect()
    }

    // ========================================================================
    // Block Expressions
    // ========================================================================

    fn parse_block_expr(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let inner = pair.into_inner().next().unwrap();
        match inner.as_rule() {
            Rule::if_expr => self.parse_if_expr(inner),
            Rule::match_expr => self.parse_match_expr(inner),
            Rule::for_expr => self.parse_for_expr(inner),
            Rule::while_expr => self.parse_while_expr(inner),
            Rule::concurrent_block => self.parse_concurrent_block(inner),
            Rule::concurrent_for => self.parse_concurrent_for(inner),
            _ => unreachable!("unexpected block_expr variant: {:?}", inner.as_rule()),
        }
    }

    fn parse_if_expr(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let condition = Box::new(self.parse_expression(inner.next().unwrap())?);
        let then_body = self.parse_block_body(inner.next().unwrap())?;

        let else_body = if let Some(else_pair) = inner.next() {
            Some(self.parse_else_clause(else_pair)?)
        } else {
            None
        };

        Ok(Expr {
            kind: ExprKind::If { condition, then_body, else_body },
            span,
        })
    }

    fn parse_else_clause(&self, pair: Pair<Rule>) -> Result<ElseClause, Vec<Diagnostic>> {
        // else_clause = { "else" ~ (if_expr | "{" ~ block_body ~ "}") }
        // Inner pairs: if_expr OR block_body
        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::if_expr => {
                    let expr = self.parse_if_expr(inner)?;
                    return Ok(ElseClause::ElseIf(Box::new(expr)));
                }
                Rule::block_body => {
                    let body = self.parse_block_body(inner)?;
                    return Ok(ElseClause::ElseBlock(body));
                }
                _ => continue,
            }
        }
        unreachable!("else_clause must contain if_expr or block_body")
    }

    fn parse_match_expr(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let target = Box::new(self.parse_expression(inner.next().unwrap())?);
        let arms: Result<Vec<_>, _> = inner.map(|arm| self.parse_match_arm(arm)).collect();

        Ok(Expr {
            kind: ExprKind::Match { target, arms: arms? },
            span,
        })
    }

    fn parse_match_arm(&self, pair: Pair<Rule>) -> Result<MatchArm, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let pattern = self.parse_expression(inner.next().unwrap())?;
        let body_pair = inner.next().unwrap(); // arm_body

        let body_inner = body_pair.into_inner().next().unwrap();
        let body = match body_inner.as_rule() {
            Rule::arm_block => {
                let block_body = body_inner.into_inner().next().unwrap();
                MatchArmBody::Block(self.parse_block_body(block_body)?)
            }
            _ => MatchArmBody::Expr(self.parse_expression(body_inner)?),
        };

        Ok(MatchArm { pattern, body, span })
    }

    fn parse_for_expr(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let binding = inner.next().unwrap().as_str().to_string();
        let iterable = Box::new(self.parse_expression(inner.next().unwrap())?);
        let body = self.parse_block_body(inner.next().unwrap())?;

        Ok(Expr {
            kind: ExprKind::For { binding, iterable, body },
            span,
        })
    }

    fn parse_while_expr(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let condition = Box::new(self.parse_expression(inner.next().unwrap())?);
        let body = self.parse_block_body(inner.next().unwrap())?;

        Ok(Expr {
            kind: ExprKind::While { condition, body },
            span,
        })
    }

    fn parse_concurrent_block(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let body = self.parse_block_body(pair.into_inner().next().unwrap())?;
        Ok(Expr {
            kind: ExprKind::ConcurrentBlock { body },
            span,
        })
    }

    fn parse_concurrent_for(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let binding = inner.next().unwrap().as_str().to_string();
        let iterable = Box::new(self.parse_expression(inner.next().unwrap())?);
        let body = self.parse_block_body(inner.next().unwrap())?;

        Ok(Expr {
            kind: ExprKind::ConcurrentFor { binding, iterable, body },
            span,
        })
    }

    // ========================================================================
    // Expressions
    // ========================================================================

    /// Parse any expression — the main entry point for expression parsing.
    fn parse_expression(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        match pair.as_rule() {
            Rule::expression => {
                let inner = pair.into_inner().next().unwrap();
                self.parse_expression(inner)
            }
            Rule::ternary => self.parse_ternary(pair),
            Rule::null_coalesce => self.parse_null_coalesce_chain(pair),
            Rule::logical_or => self.parse_binary_left(pair, Rule::logical_and),
            Rule::logical_and => self.parse_binary_left(pair, Rule::equality),
            Rule::equality => self.parse_binary_left(pair, Rule::comparison),
            Rule::comparison => self.parse_binary_left(pair, Rule::addition),
            Rule::addition => self.parse_binary_left(pair, Rule::concatenation),
            Rule::concatenation => self.parse_binary_left(pair, Rule::unary),
            Rule::unary => self.parse_unary(pair),
            Rule::postfix => self.parse_postfix(pair),

            // Primary expressions (reached directly from postfix -> primary)
            Rule::integer_lit => self.parse_integer_lit(pair),
            Rule::float_lit => self.parse_float_lit(pair),
            Rule::string_lit => self.parse_string_lit(pair),
            Rule::raw_string_lit => self.parse_raw_string_lit(pair),
            Rule::bool_lit => self.parse_bool_lit(pair),
            Rule::null_lit => self.parse_null_lit(pair),
            Rule::identifier => self.parse_identifier_expr(pair),
            Rule::array_lit => self.parse_array_lit(pair),
            Rule::struct_lit => self.parse_struct_lit(pair),
            Rule::map_lit => self.parse_map_lit(pair),

            // Block expressions as primaries
            Rule::if_expr => self.parse_if_expr(pair),
            Rule::match_expr => self.parse_match_expr(pair),
            Rule::for_expr => self.parse_for_expr(pair),
            Rule::while_expr => self.parse_while_expr(pair),
            Rule::concurrent_block => self.parse_concurrent_block(pair),
            Rule::concurrent_for => self.parse_concurrent_for(pair),

            other => {
                let span = self.span_from(&pair);
                Err(vec![Diagnostic {
                    file: self.filename.clone(),
                    line: span.line,
                    col: span.col,
                    severity: Severity::Error,
                    message: format!("unexpected rule in expression: {other:?}"),
                }])
            }
        }
    }

    fn parse_ternary(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();
        let first = inner.next().unwrap();
        let condition = self.parse_expression(first)?;

        if let Some(true_pair) = inner.next() {
            let true_val = self.parse_expression(true_pair)?;
            let false_pair = inner.next().unwrap();
            let false_val = self.parse_expression(false_pair)?;
            Ok(Expr {
                kind: ExprKind::Ternary {
                    condition: Box::new(condition),
                    true_val: Box::new(true_val),
                    false_val: Box::new(false_val),
                },
                span,
            })
        } else {
            Ok(condition)
        }
    }

    /// Parse a left-associative binary operator chain.
    /// The pair contains: [operand, op, operand, op, operand, ...]
    fn parse_binary_left(&self, pair: Pair<Rule>, _operand_rule: Rule) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let first = inner.next().unwrap();
        let mut left = self.parse_expression(first)?;

        while let Some(op_pair) = inner.next() {
            let op = self.parse_bin_op(&op_pair);
            let right_pair = inner.next().unwrap();
            let right = self.parse_expression(right_pair)?;
            left = Expr {
                kind: ExprKind::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span: span.clone(),
            };
        }

        Ok(left)
    }

    /// Parse null-coalescing chain (a ?? b ?? c).
    fn parse_null_coalesce_chain(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();
        let first = inner.next().unwrap();
        let mut left = self.parse_expression(first)?;

        while let Some(op_or_operand) = inner.next() {
            if op_or_operand.as_rule() == Rule::null_coalesce_op {
                let right_pair = inner.next().unwrap();
                let right = self.parse_expression(right_pair)?;
                left = Expr {
                    kind: ExprKind::NullCoalesce {
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    span: span.clone(),
                };
            } else {
                // Single operand passthrough
                let right = self.parse_expression(op_or_operand)?;
                left = Expr {
                    kind: ExprKind::NullCoalesce {
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    span: span.clone(),
                };
            }
        }

        Ok(left)
    }

    fn parse_bin_op(&self, pair: &Pair<Rule>) -> BinOp {
        match pair.as_rule() {
            Rule::eq_op => match pair.as_str() {
                "==" => BinOp::Eq,
                "!=" => BinOp::NotEq,
                _ => unreachable!(),
            },
            Rule::cmp_op => match pair.as_str() {
                ">" => BinOp::Gt,
                "<" => BinOp::Lt,
                ">=" => BinOp::GtEq,
                "<=" => BinOp::LtEq,
                _ => unreachable!(),
            },
            Rule::add_op => match pair.as_str() {
                "+" => BinOp::Add,
                "-" => BinOp::Sub,
                _ => unreachable!("unknown add_op: '{}'", pair.as_str()),
            },
            Rule::concat_op => BinOp::Concat,
            Rule::and_op => BinOp::And,
            Rule::or_op => BinOp::Or,
            Rule::null_coalesce_op => BinOp::Add, // shouldn't reach here, handled by parse_null_coalesce_chain
            _ => unreachable!("unknown bin op: {:?} '{}'", pair.as_rule(), pair.as_str()),
        }
    }

    fn parse_unary(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let children: Vec<_> = pair.into_inner().collect();

        if children.len() == 1 {
            // No unary operator, just the operand
            return self.parse_expression(children.into_iter().next().unwrap());
        }

        // Has unary ! operator — last child is the operand
        let operand = self.parse_expression(children.into_iter().last().unwrap())?;
        Ok(Expr {
            kind: ExprKind::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(operand),
            },
            span,
        })
    }

    fn parse_postfix(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();

        let primary = inner.next().unwrap();
        let mut expr = self.parse_expression(primary)?;

        for postfix_part in inner {
            match postfix_part.as_rule() {
                Rule::field_access => {
                    let field = postfix_part.into_inner().next().unwrap().as_str().to_string();
                    expr = Expr {
                        kind: ExprKind::FieldAccess {
                            object: Box::new(expr),
                            field,
                        },
                        span: span.clone(),
                    };
                }
                Rule::call_expr => {
                    let (args, config) = self.parse_call_expr(postfix_part)?;
                    expr = Expr {
                        kind: ExprKind::Call {
                            target: Box::new(expr),
                            args,
                            config,
                        },
                        span: span.clone(),
                    };
                }
                _ => {}
            }
        }

        Ok(expr)
    }

    #[allow(clippy::type_complexity)]
    fn parse_call_expr(
        &self,
        pair: Pair<Rule>,
    ) -> Result<(Vec<CallArg>, Option<Vec<ConfigField>>), Vec<Diagnostic>> {
        let mut args = Vec::new();
        let mut config = None;

        for child in pair.into_inner() {
            match child.as_rule() {
                Rule::call_args => {
                    args = self.parse_call_args(child)?;
                }
                Rule::config_block => {
                    config = Some(self.parse_config_block(child)?);
                }
                _ => {}
            }
        }

        Ok((args, config))
    }

    fn parse_call_args(&self, pair: Pair<Rule>) -> Result<Vec<CallArg>, Vec<Diagnostic>> {
        pair.into_inner()
            .map(|arg_pair| self.parse_call_arg(arg_pair))
            .collect()
    }

    fn parse_call_arg(&self, pair: Pair<Rule>) -> Result<CallArg, Vec<Diagnostic>> {
        let mut inner = pair.into_inner();
        let first = inner.next().unwrap();

        // Check if this is a named_arg (ident: expr) or just an expression
        if first.as_rule() == Rule::named_arg {
            let mut named_inner = first.into_inner();
            let name = named_inner.next().unwrap().as_str().to_string();
            let value = self.parse_expression(named_inner.next().unwrap())?;
            Ok(CallArg::Named { name, value })
        } else {
            let value = self.parse_expression(first)?;
            Ok(CallArg::Positional(value))
        }
    }

    fn parse_config_block(&self, pair: Pair<Rule>) -> Result<Vec<ConfigField>, Vec<Diagnostic>> {
        let mut fields = Vec::new();
        for child in pair.into_inner() {
            if child.as_rule() == Rule::config_fields {
                for field_pair in child.into_inner() {
                    fields.push(self.parse_config_field(field_pair)?);
                }
            }
        }
        Ok(fields)
    }

    fn parse_config_field(&self, pair: Pair<Rule>) -> Result<ConfigField, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();
        let name = inner.next().unwrap().as_str().to_string();
        let value = self.parse_expression(inner.next().unwrap())?;
        Ok(ConfigField { name, value, span })
    }

    // ========================================================================
    // Primary Expressions
    // ========================================================================

    fn parse_integer_lit(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let val: i64 = pair.as_str().parse().map_err(|e| self.err(&pair, format!("invalid integer: {e}")))?;
        Ok(Expr { kind: ExprKind::IntLit(val), span })
    }

    fn parse_float_lit(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let val: f64 = pair.as_str().parse().map_err(|e| self.err(&pair, format!("invalid float: {e}")))?;
        Ok(Expr { kind: ExprKind::FloatLit(val), span })
    }

    fn parse_string_lit(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut segments = Vec::new();
        let mut current_text = String::new();

        for inner in pair.into_inner() {
            match inner.as_rule() {
                Rule::string_char => {
                    current_text.push_str(inner.as_str());
                }
                Rule::escape_seq => {
                    // Flush current text
                    if !current_text.is_empty() {
                        segments.push(StringSegment::Literal(std::mem::take(&mut current_text)));
                    }
                    let ch = match inner.as_str() {
                        "\\n" => '\n',
                        "\\r" => '\r',
                        "\\t" => '\t',
                        "\\\"" => '"',
                        "\\\\" => '\\',
                        "\\{" => '{',
                        "\\}" => '}',
                        "\\0" => '\0',
                        other => other.chars().last().unwrap(),
                    };
                    segments.push(StringSegment::Escape(ch));
                }
                Rule::interpolation => {
                    // Flush current text
                    if !current_text.is_empty() {
                        segments.push(StringSegment::Literal(std::mem::take(&mut current_text)));
                    }
                    let expr_pair = inner.into_inner().next().unwrap();
                    let expr = self.parse_expression(expr_pair)?;
                    segments.push(StringSegment::Interpolation(Box::new(expr)));
                }
                _ => {}
            }
        }

        if !current_text.is_empty() {
            segments.push(StringSegment::Literal(current_text));
        }

        Ok(Expr { kind: ExprKind::StringLit(segments), span })
    }

    fn parse_raw_string_lit(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let raw = pair.as_str();
        // Strip backtick delimiters
        let content = &raw[1..raw.len() - 1];
        Ok(Expr { kind: ExprKind::RawStringLit(content.to_string()), span })
    }

    fn parse_bool_lit(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let val = pair.as_str() == "true";
        Ok(Expr { kind: ExprKind::BoolLit(val), span })
    }

    fn parse_null_lit(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        Ok(Expr { kind: ExprKind::NullLit, span })
    }

    fn parse_identifier_expr(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        Ok(Expr {
            kind: ExprKind::Identifier(pair.as_str().to_string()),
            span,
        })
    }

    fn parse_array_lit(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let elements: Result<Vec<_>, _> = pair
            .into_inner()
            .map(|p| self.parse_expression(p))
            .collect();
        Ok(Expr { kind: ExprKind::ArrayLit(elements?), span })
    }

    fn parse_struct_lit(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut inner = pair.into_inner();
        let type_name = inner.next().unwrap().as_str().to_string();

        let mut fields = Vec::new();
        for child in inner {
            if child.as_rule() == Rule::config_fields {
                for field_pair in child.into_inner() {
                    fields.push(self.parse_config_field(field_pair)?);
                }
            }
        }

        Ok(Expr { kind: ExprKind::StructLit { type_name, fields }, span })
    }

    fn parse_map_lit(&self, pair: Pair<Rule>) -> Result<Expr, Vec<Diagnostic>> {
        let span = self.span_from(&pair);
        let mut fields = Vec::new();
        for child in pair.into_inner() {
            if child.as_rule() == Rule::config_fields {
                for field_pair in child.into_inner() {
                    fields.push(self.parse_config_field(field_pair)?);
                }
            }
        }
        Ok(Expr { kind: ExprKind::MapLit(fields), span })
    }

    // ========================================================================
    // String Helpers
    // ========================================================================

    /// Extract the string value from a string_lit pair (strips quotes, resolves escapes).
    fn extract_string_value(&self, pair: &Pair<Rule>) -> String {
        let mut result = String::new();
        for inner in pair.clone().into_inner() {
            match inner.as_rule() {
                Rule::string_char => result.push_str(inner.as_str()),
                Rule::escape_seq => {
                    let ch = match inner.as_str() {
                        "\\n" => '\n',
                        "\\r" => '\r',
                        "\\t" => '\t',
                        "\\\"" => '"',
                        "\\\\" => '\\',
                        "\\{" => '{',
                        "\\}" => '}',
                        "\\0" => '\0',
                        other => other.chars().last().unwrap(),
                    };
                    result.push(ch);
                }
                Rule::interpolation => {
                    // For simple value extraction (description, scroll name),
                    // interpolation shouldn't appear. Include raw text as fallback.
                    result.push_str(inner.as_str());
                }
                _ => {}
            }
        }
        result
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn is_expression_rule(rule: Rule) -> bool {
    matches!(
        rule,
        Rule::expression
            | Rule::ternary
            | Rule::null_coalesce
            | Rule::logical_or
            | Rule::logical_and
            | Rule::equality
            | Rule::comparison
            | Rule::addition
            | Rule::concatenation
            | Rule::unary
            | Rule::postfix
            | Rule::integer_lit
            | Rule::float_lit
            | Rule::string_lit
            | Rule::raw_string_lit
            | Rule::bool_lit
            | Rule::null_lit
            | Rule::identifier
            | Rule::array_lit
            | Rule::struct_lit
            | Rule::map_lit
            | Rule::if_expr
            | Rule::match_expr
            | Rule::for_expr
            | Rule::while_expr
            | Rule::concurrent_block
            | Rule::concurrent_for
    )
}
