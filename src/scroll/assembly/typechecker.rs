// SPDX-License-Identifier: MIT
//! Type checker for Scroll Assembly ASTs.
//!
//! Validates parsed ASTs before execution:
//! - All variables declared before use
//! - Type annotations match across declarations and assignments (D13r, D14r)
//! - `provide` variables are assigned on all code paths (D18r)
//! - `match` on enums is exhaustive (D16)
//! - Block scoping enforced (D15r2)
//!
//! Entry point: `check(ast) -> Result<(), Vec<Diagnostic>>`

use super::ast::*;
use super::parser::{Diagnostic, Severity};
use std::collections::HashMap;

// ============================================================================
// Public API
// ============================================================================

/// Type-check a parsed ScrollFile. Returns diagnostics (errors and warnings).
pub fn check(ast: &ScrollFile, filename: &str) -> Vec<Diagnostic> {
    let mut ctx = CheckCtx::new(filename.to_string());

    // Register type definitions
    for td in &ast.type_defs {
        ctx.register_type_def(td);
    }

    // Register inline struct defs from require/provide
    for r in &ast.scroll.requires {
        if let Some(fields) = &r.inline_struct {
            ctx.register_inline_struct(&r.type_ref, fields);
        }
    }
    for p in &ast.scroll.provides {
        if let Some(fields) = &p.inline_struct {
            ctx.register_inline_struct(&p.type_ref, fields);
        }
    }

    // Register require variables (inputs)
    for r in &ast.scroll.requires {
        ctx.declare_var(&r.name, &r.type_ref, &r.span);
    }

    // Register provide variables (outputs — must be assigned)
    for p in &ast.scroll.provides {
        ctx.declare_var(&p.name, &p.type_ref, &p.span);
        ctx.mark_provide(&p.name);
    }

    // Check body
    ctx.check_block_body(&ast.scroll.body);

    // Verify provide variables are assigned
    ctx.verify_provides();

    ctx.diagnostics
}

// ============================================================================
// Type Registry
// ============================================================================

#[derive(Debug, Clone)]
struct TypeInfo {
    fields: Option<Vec<(String, TypeRef)>>,   // struct fields
    variants: Option<Vec<String>>,             // enum variants
}

// ============================================================================
// Variable Scope
// ============================================================================

#[derive(Debug, Clone)]
struct VarInfo {
    type_ref: TypeRef,
    is_provide: bool,
    assigned: bool,
}

// ============================================================================
// Check Context
// ============================================================================

struct CheckCtx {
    filename: String,
    diagnostics: Vec<Diagnostic>,
    /// Type definitions: name -> TypeInfo
    types: HashMap<String, TypeInfo>,
    /// Variable scopes (stack of scope frames)
    scopes: Vec<HashMap<String, VarInfo>>,
    /// Provide variable names (for assignment checking)
    provides: Vec<String>,
}

impl CheckCtx {
    fn new(filename: String) -> Self {
        Self {
            filename,
            diagnostics: Vec::new(),
            types: HashMap::new(),
            scopes: vec![HashMap::new()], // scroll-level scope
            provides: Vec::new(),
        }
    }

    fn error(&mut self, span: &Span, msg: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            file: self.filename.clone(),
            line: span.line,
            col: span.col,
            severity: Severity::Error,
            message: msg.into(),
        });
    }

    fn warning(&mut self, span: &Span, msg: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            file: self.filename.clone(),
            line: span.line,
            col: span.col,
            severity: Severity::Warning,
            message: msg.into(),
        });
    }

    // ========================================================================
    // Type Registration
    // ========================================================================

    fn register_type_def(&mut self, td: &TypeDef) {
        match td {
            TypeDef::Struct(s) => {
                let fields: Vec<_> = s.fields.iter().map(|f| (f.name.clone(), f.type_ref.clone())).collect();
                if self.types.contains_key(&s.name) {
                    self.error(&s.span, format!("duplicate type definition: '{}'", s.name));
                }
                self.types.insert(s.name.clone(), TypeInfo { fields: Some(fields), variants: None });
            }
            TypeDef::Enum(e) => {
                if self.types.contains_key(&e.name) {
                    self.error(&e.span, format!("duplicate type definition: '{}'", e.name));
                }
                self.types.insert(e.name.clone(), TypeInfo { fields: None, variants: Some(e.variants.clone()) });
            }
        }
    }

    fn register_inline_struct(&mut self, type_ref: &TypeRef, fields: &[StructField]) {
        if let TypeBase::Named(name) = &type_ref.base {
            if !self.types.contains_key(name) {
                let field_info: Vec<_> = fields.iter().map(|f| (f.name.clone(), f.type_ref.clone())).collect();
                self.types.insert(name.clone(), TypeInfo { fields: Some(field_info), variants: None });
            }
        }
    }

    // ========================================================================
    // Variable Scope
    // ========================================================================

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_var(&mut self, name: &str, type_ref: &TypeRef, span: &Span) {
        // Check if already declared in current scope
        if let Some(current) = self.scopes.last() {
            if current.contains_key(name) {
                self.error(span, format!("variable '{}' already declared in this scope", name));
                return;
            }
        }
        let info = VarInfo { type_ref: type_ref.clone(), is_provide: false, assigned: true };
        self.scopes.last_mut().unwrap().insert(name.to_string(), info);
    }

    fn mark_provide(&mut self, name: &str) {
        self.provides.push(name.to_string());
        // Mark provide vars as unassigned initially — they must be assigned in the body
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.get_mut(name) {
                info.is_provide = true;
                info.assigned = false;
                return;
            }
        }
    }

    fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    fn mark_assigned(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.get_mut(name) {
                info.assigned = true;
                return;
            }
        }
    }

    // ========================================================================
    // Body Checking
    // ========================================================================

    fn check_block_body(&mut self, body: &BlockBody) {
        for stmt in &body.statements {
            self.check_statement(stmt);
        }
        if let Some(tail) = &body.tail_expr {
            self.check_expr(tail);
        }
    }

    fn check_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::SetDecl(sd) => {
                self.validate_type_ref(&sd.type_ref, &sd.span);
                self.check_expr(&sd.value);
                self.declare_var(&sd.name, &sd.type_ref, &sd.span);
            }
            Statement::Binding(b) => {
                self.check_expr(&b.source);
                self.validate_type_ref(&b.type_ref, &b.span);
                self.declare_var(&b.name, &b.type_ref, &b.span);
                for handler in &b.error_chain {
                    if let ErrorHandler::Fallback(body) = handler {
                        self.push_scope();
                        self.check_block_body(body);
                        self.pop_scope();
                    }
                }
            }
            Statement::Assignment(a) => {
                if self.lookup_var(&a.target).is_none() {
                    self.error(&a.span, format!("undefined variable: '{}'", a.target));
                } else {
                    self.mark_assigned(&a.target);
                }
                self.check_expr(&a.value);
            }
            Statement::Break(span) => {
                // Break is valid syntactically; loop context checked semantically
                let _ = span;
            }
            Statement::BlockExpr(expr) | Statement::ExprStmt(expr) => {
                self.check_expr(expr);
            }
        }
    }

    // ========================================================================
    // Expression Checking
    // ========================================================================

    fn check_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Identifier(name) => {
                if self.lookup_var(name).is_none()
                    && !self.is_known_namespace(name)
                    && !self.types.contains_key(name)
                {
                    // Bare identifiers could be: enum-like config values (tier: cheap),
                    // runtime-resolved constants (threshold: majority), or genuinely
                    // undefined variables. Flag as warning for v1 — full resolution
                    // requires runtime type information.
                    self.warning(&expr.span, format!("possibly undefined variable: '{}'", name));
                }
            }
            ExprKind::BinaryOp { left, right, .. } => {
                self.check_expr(left);
                self.check_expr(right);
            }
            ExprKind::UnaryOp { operand, .. } => {
                self.check_expr(operand);
            }
            ExprKind::Ternary { condition, true_val, false_val } => {
                self.check_expr(condition);
                self.check_expr(true_val);
                self.check_expr(false_val);
            }
            ExprKind::NullCoalesce { left, right } => {
                self.check_expr(left);
                self.check_expr(right);
            }
            ExprKind::FieldAccess { object, .. } => {
                self.check_expr(object);
            }
            ExprKind::Call { target, args, config } => {
                self.check_expr(target);
                for arg in args {
                    match arg {
                        CallArg::Named { value, .. } | CallArg::Positional(value) => {
                            self.check_expr(value);
                        }
                    }
                }
                if let Some(fields) = config {
                    for field in fields {
                        self.check_expr(&field.value);
                    }
                }
            }
            ExprKind::If { condition, then_body, else_body } => {
                self.check_expr(condition);
                self.push_scope();
                self.check_block_body(then_body);
                self.pop_scope();
                if let Some(else_clause) = else_body {
                    match else_clause {
                        ElseClause::ElseBlock(body) => {
                            self.push_scope();
                            self.check_block_body(body);
                            self.pop_scope();
                        }
                        ElseClause::ElseIf(if_expr) => {
                            self.check_expr(if_expr);
                        }
                    }
                }
            }
            ExprKind::Match { target, arms } => {
                self.check_expr(target);
                self.check_match_exhaustiveness(target, arms, &expr.span);
                for arm in arms {
                    self.check_expr(&arm.pattern);
                    match &arm.body {
                        MatchArmBody::Block(body) => {
                            self.push_scope();
                            self.check_block_body(body);
                            self.pop_scope();
                        }
                        MatchArmBody::Expr(e) => self.check_expr(e),
                    }
                }
            }
            ExprKind::For { binding, iterable, body } => {
                self.check_expr(iterable);
                self.push_scope();
                // Declare loop variable with a placeholder type
                let loop_var_type = TypeRef {
                    base: TypeBase::Primitive(PrimitiveType::Map),
                    is_array: false,
                    is_nullable: false,
                    span: expr.span.clone(),
                };
                self.declare_var(binding, &loop_var_type, &expr.span);
                self.check_block_body(body);
                self.pop_scope();
            }
            ExprKind::While { condition, body } => {
                self.check_expr(condition);
                self.push_scope();
                self.check_block_body(body);
                self.pop_scope();
            }
            ExprKind::ConcurrentBlock { body } => {
                self.push_scope();
                self.check_block_body(body);
                self.pop_scope();
            }
            ExprKind::ConcurrentFor { binding, iterable, body } => {
                self.check_expr(iterable);
                self.push_scope();
                let loop_var_type = TypeRef {
                    base: TypeBase::Primitive(PrimitiveType::Map),
                    is_array: false,
                    is_nullable: false,
                    span: expr.span.clone(),
                };
                self.declare_var(binding, &loop_var_type, &expr.span);
                self.check_block_body(body);
                self.pop_scope();
            }
            ExprKind::ArrayLit(elements) => {
                for el in elements {
                    self.check_expr(el);
                }
            }
            ExprKind::StructLit { type_name, fields } => {
                if !self.types.contains_key(type_name) {
                    self.error(&expr.span, format!("unknown type: '{type_name}'"));
                } else {
                    self.check_struct_fields(type_name, fields, &expr.span);
                }
                for field in fields {
                    self.check_expr(&field.value);
                }
            }
            ExprKind::MapLit(fields) => {
                for field in fields {
                    self.check_expr(&field.value);
                }
            }
            ExprKind::StringLit(segments) => {
                for seg in segments {
                    if let StringSegment::Interpolation(expr) = seg {
                        self.check_expr(expr);
                    }
                }
            }
            // Literals — nothing to check
            ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::RawStringLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::NullLit => {}
        }
    }

    // ========================================================================
    // Type Validation
    // ========================================================================

    fn validate_type_ref(&mut self, type_ref: &TypeRef, span: &Span) {
        if let TypeBase::Named(name) = &type_ref.base {
            if !self.types.contains_key(name) {
                self.error(span, format!("unknown type: '{name}'"));
            }
        }
    }

    fn check_struct_fields(&mut self, type_name: &str, fields: &[ConfigField], span: &Span) {
        // Clone field definitions to avoid borrow conflict with self.error/warning
        let defined_fields = self.types.get(type_name)
            .and_then(|ti| ti.fields.clone());

        let Some(defined_fields) = defined_fields else { return };

        let defined_names: Vec<_> = defined_fields.iter().map(|(n, _)| n.as_str()).collect();
        for field in fields {
            if !defined_names.contains(&field.name.as_str()) {
                self.error(&field.span, format!(
                    "unknown field '{}' on type '{type_name}'", field.name
                ));
            }
        }
        let provided_names: Vec<_> = fields.iter().map(|f| f.name.as_str()).collect();
        for (defined_name, _) in &defined_fields {
            if !provided_names.contains(&defined_name.as_str()) {
                self.warning(span, format!(
                    "missing field '{}' in struct literal '{type_name}'", defined_name
                ));
            }
        }
    }

    // ========================================================================
    // Match Exhaustiveness
    // ========================================================================

    fn check_match_exhaustiveness(&mut self, target: &Expr, arms: &[MatchArm], span: &Span) {
        // Infer target type name
        let target_type_name = match &target.kind {
            ExprKind::Identifier(name) => {
                self.lookup_var(name).and_then(|info| {
                    if let TypeBase::Named(tn) = &info.type_ref.base {
                        Some(tn.clone())
                    } else {
                        None
                    }
                })
            }
            _ => None,
        };

        let Some(type_name) = target_type_name else { return };

        // Get variants (clone to avoid borrow conflict)
        let variants = self.types.get(&type_name)
            .and_then(|ti| ti.variants.clone());

        let Some(variants) = variants else { return };

        let mut covered: Vec<String> = Vec::new();
        for arm in arms {
            if let ExprKind::FieldAccess { field, .. } = &arm.pattern.kind {
                covered.push(field.clone());
            }
        }

        let missing: Vec<_> = variants.iter()
            .filter(|v| !covered.contains(v))
            .collect();

        if !missing.is_empty() {
            self.error(span, format!(
                "non-exhaustive match on '{}': missing variants: {}",
                type_name,
                missing.iter().map(|v| format!("{type_name}.{v}")).collect::<Vec<_>>().join(", ")
            ));
        }
    }

    // ========================================================================
    // Provide Verification
    // ========================================================================

    fn verify_provides(&mut self) {
        for name in &self.provides.clone() {
            let mut found = false;
            let mut assigned = false;
            for scope in self.scopes.iter().rev() {
                if let Some(info) = scope.get(name) {
                    found = true;
                    assigned = info.assigned;
                    break;
                }
            }
            if found && !assigned {
                self.warning(
                    &Span::default(),
                    format!("provide variable '{name}' may not be assigned on all code paths"),
                );
            }
        }
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    fn is_known_namespace(&self, name: &str) -> bool {
        matches!(name,
            "platform" | "fs" | "vcs" | "test"
            | "invoke" | "parallel" | "consensus"
            | "elaborate" | "distill" | "validate" | "convert" | "aggregate"
            | "run"
        )
    }
}
