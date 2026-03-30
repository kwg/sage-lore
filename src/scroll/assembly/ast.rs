// SPDX-License-Identifier: MIT
//! AST types for the Scroll Assembly language.
//!
//! These types represent the parsed structure of a `.scroll` file.
//! The parser (S2) converts pest parse output into these types.
//! The type checker (S3) validates them before execution.

use std::fmt;

/// Source location for error reporting.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

// ============================================================================
// Top-Level
// ============================================================================

/// A complete `.scroll` file: type definitions followed by a scroll block.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollFile {
    pub type_defs: Vec<TypeDef>,
    pub scroll: ScrollBlock,
}

// ============================================================================
// Type Definitions
// ============================================================================

/// A type definition: either a struct or an enum.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeDef {
    Struct(StructDef),
    Enum(EnumDef),
}

/// Named struct type: `type Story { number: int, title: str }`
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<StructField>,
    pub span: Span,
}

/// A single field in a struct definition.
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub type_ref: TypeRef,
    pub span: Span,
}

/// Enum type: `type Complexity { low, medium, high }`
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<String>,
    pub span: Span,
}

// ============================================================================
// Type References
// ============================================================================

/// A reference to a type, used in declarations and annotations.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub base: TypeBase,
    pub is_array: bool,
    pub is_nullable: bool,
    pub span: Span,
}

/// The base of a type reference.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeBase {
    /// Primitive: str, int, float, bool, map
    Primitive(PrimitiveType),
    /// Named type: Story, Review, etc.
    Named(String),
}

/// Built-in primitive types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    Str,
    Int,
    Float,
    Bool,
    Map,
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveType::Str => write!(f, "str"),
            PrimitiveType::Int => write!(f, "int"),
            PrimitiveType::Float => write!(f, "float"),
            PrimitiveType::Bool => write!(f, "bool"),
            PrimitiveType::Map => write!(f, "map"),
        }
    }
}

// ============================================================================
// Scroll Block
// ============================================================================

/// The main scroll block: `scroll "name" { ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollBlock {
    pub name: String,
    pub description: Option<String>,
    pub requires: Vec<RequireDecl>,
    pub provides: Vec<ProvideDecl>,
    pub body: BlockBody,
    pub span: Span,
}

/// A `require` declaration in the scroll header.
#[derive(Debug, Clone, PartialEq)]
pub struct RequireDecl {
    pub name: String,
    pub type_ref: TypeRef,
    pub default: Option<Expr>,
    pub inline_struct: Option<Vec<StructField>>,
    pub span: Span,
}

/// A `provide` declaration in the scroll header.
#[derive(Debug, Clone, PartialEq)]
pub struct ProvideDecl {
    pub name: String,
    pub type_ref: TypeRef,
    pub inline_struct: Option<Vec<StructField>>,
    pub span: Span,
}

// ============================================================================
// Block Body
// ============================================================================

/// The body of a block: statements plus an optional trailing expression (return value).
#[derive(Debug, Clone, PartialEq)]
pub struct BlockBody {
    pub statements: Vec<Statement>,
    pub tail_expr: Option<Box<Expr>>,
}

// ============================================================================
// Statements
// ============================================================================

/// A statement in a block body (terminated by `;`).
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// `set name: Type = expr;`
    SetDecl(SetDecl),
    /// `expr -> name: Type | handlers;`
    Binding(BindingStmt),
    /// `name = expr;` or `name += expr;` etc.
    Assignment(Assignment),
    /// `break;`
    Break(Span),
    /// Block expression used as statement: `if ... { } else { };`
    BlockExpr(Expr),
    /// Expression statement: `platform.close_issue(number: n);`
    ExprStmt(Expr),
}

/// Variable declaration: `set count: int = 0;`
#[derive(Debug, Clone, PartialEq)]
pub struct SetDecl {
    pub name: String,
    pub type_ref: TypeRef,
    pub value: Expr,
    pub span: Span,
}

/// Output binding: `platform.get_issue(number: n) -> raw_issue: IssueResponse;`
#[derive(Debug, Clone, PartialEq)]
pub struct BindingStmt {
    pub source: Expr,
    pub name: String,
    pub type_ref: TypeRef,
    pub error_chain: Vec<ErrorHandler>,
    pub span: Span,
}

/// Assignment: `count = count + 1;` or `count += 1;`
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub target: String,
    pub op: AssignOp,
    pub value: Expr,
    pub span: Span,
}

/// Assignment operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    /// `=`
    Assign,
    /// `+=`
    AddAssign,
    /// `-=`
    SubAssign,
    /// `++=`
    AppendAssign,
}

// ============================================================================
// Error Handling
// ============================================================================

/// An error handler in a binding chain: `| continue`, `| retry(3)`, `| fallback { ... }`
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorHandler {
    Continue,
    Retry(u32),
    Fallback(BlockBody),
}

// ============================================================================
// Expressions
// ============================================================================

/// An expression node.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// The kind of expression.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    // --- Literals ---
    /// Integer literal: `42`
    IntLit(i64),
    /// Float literal: `3.14`
    FloatLit(f64),
    /// String literal with interpolation segments
    StringLit(Vec<StringSegment>),
    /// Raw string literal: `` `no interpolation` ``
    RawStringLit(String),
    /// Boolean literal: `true`, `false`
    BoolLit(bool),
    /// Null literal
    NullLit,

    // --- Identifiers ---
    /// Variable reference: `count`
    Identifier(String),

    // --- Collections ---
    /// Array literal: `[1, 2, 3]`
    ArrayLit(Vec<Expr>),
    /// Struct literal: `Story { number: 1, title: "x" }`
    StructLit {
        type_name: String,
        fields: Vec<ConfigField>,
    },
    /// Map literal: `{ key: "value" }`
    MapLit(Vec<ConfigField>),

    // --- Operations ---
    /// Binary operation: `a + b`, `a == b`, `a && b`, etc.
    BinaryOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    /// Unary operation: `!x`
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    /// Ternary: `cond ? true_val : false_val`
    Ternary {
        condition: Box<Expr>,
        true_val: Box<Expr>,
        false_val: Box<Expr>,
    },
    /// Null coalescing: `a ?? b`
    NullCoalesce {
        left: Box<Expr>,
        right: Box<Expr>,
    },

    // --- Access ---
    /// Field access: `issue.title`
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
    /// Function/method call: `platform.get_issue(number: 1)`
    Call {
        target: Box<Expr>,
        args: Vec<CallArg>,
        config: Option<Vec<ConfigField>>,
    },

    // --- Block Expressions ---
    /// If expression: `if cond { ... } else { ... }`
    If {
        condition: Box<Expr>,
        then_body: BlockBody,
        else_body: Option<ElseClause>,
    },
    /// Match expression: `match expr { ... }`
    Match {
        target: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// For expression: `for item in collection { ... }`
    For {
        binding: String,
        iterable: Box<Expr>,
        body: BlockBody,
    },
    /// While loop: `while cond { ... }`
    While {
        condition: Box<Expr>,
        body: BlockBody,
    },
    /// Concurrent block: `concurrent { ... }`
    ConcurrentBlock {
        body: BlockBody,
    },
    /// Concurrent for: `concurrent for item in collection { ... }`
    ConcurrentFor {
        binding: String,
        iterable: Box<Expr>,
        body: BlockBody,
    },
}

/// A segment of an interpolated string.
#[derive(Debug, Clone, PartialEq)]
pub enum StringSegment {
    /// Literal text
    Literal(String),
    /// Interpolated expression: `{expr}`
    Interpolation(Box<Expr>),
    /// Escape sequence: `\n`, `\"`, etc.
    Escape(char),
}

/// A field in a struct literal, map literal, or config block.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigField {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

/// A call argument (named or positional).
#[derive(Debug, Clone, PartialEq)]
pub enum CallArg {
    Named { name: String, value: Expr },
    Positional(Expr),
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    // Collection
    Concat,
    // Comparison
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    // Logical
    And,
    Or,
    // Map
    MapMerge, // same as Add, distinguished by type checker
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
}

/// Else clause (either a block or an else-if).
#[derive(Debug, Clone, PartialEq)]
pub enum ElseClause {
    ElseBlock(BlockBody),
    ElseIf(Box<Expr>), // contains an ExprKind::If
}

/// A match arm: `pattern => body`
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Expr,
    pub body: MatchArmBody,
    pub span: Span,
}

/// Match arm body: either a block or a single expression.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchArmBody {
    Block(BlockBody),
    Expr(Expr),
}
