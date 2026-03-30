// SPDX-License-Identifier: MIT
//! LSP server for Scroll Assembly language.
//!
//! Provides diagnostics, completion, and hover for `.scroll` files.
//! Reuses the parser and type checker from `scroll::assembly`.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::scroll::assembly::parser::{self, Severity};
use crate::scroll::assembly::typechecker;

// ============================================================================
// LSP Backend
// ============================================================================

struct ScrollLsp {
    client: Client,
}

impl ScrollLsp {
    fn new(client: Client) -> Self {
        Self { client }
    }

    /// Parse and type-check a document, returning diagnostics.
    async fn diagnose(&self, uri: &Url, text: &str) {
        let filename = uri.path();
        let mut diagnostics = Vec::new();

        // Parse
        match parser::parse(text, filename) {
            Ok(ast) => {
                // Type-check
                let tc_diags = typechecker::check(&ast, filename);
                for d in tc_diags {
                    diagnostics.push(to_lsp_diagnostic(&d));
                }
            }
            Err(parse_diags) => {
                for d in parse_diags {
                    diagnostics.push(to_lsp_diagnostic(&d));
                }
            }
        }

        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }
}

fn to_lsp_diagnostic(d: &parser::Diagnostic) -> Diagnostic {
    let severity = match d.severity {
        Severity::Error => Some(DiagnosticSeverity::ERROR),
        Severity::Warning => Some(DiagnosticSeverity::WARNING),
    };
    let line = if d.line > 0 { d.line - 1 } else { 0 }; // LSP is 0-indexed
    let col = if d.col > 0 { d.col - 1 } else { 0 };
    Diagnostic {
        range: Range {
            start: Position { line: line as u32, character: col as u32 },
            end: Position { line: line as u32, character: (col + 1) as u32 },
        },
        severity,
        source: Some("sage-scroll".to_string()),
        message: d.message.clone(),
        ..Default::default()
    }
}

// ============================================================================
// Keyword / primitive data for completion and hover
// ============================================================================

struct DocItem {
    label: &'static str,
    kind: CompletionItemKind,
    detail: &'static str,
    docs: &'static str,
}

const COMPLETIONS: &[DocItem] = &[
    // Keywords
    DocItem { label: "scroll", kind: CompletionItemKind::KEYWORD, detail: "Scroll block", docs: "scroll \"name\" { ... }\nDefines a scroll with require/provide interface and body." },
    DocItem { label: "type", kind: CompletionItemKind::KEYWORD, detail: "Type definition", docs: "type Name { field: type, ... }\nDefines a struct or enum type at file scope." },
    DocItem { label: "require", kind: CompletionItemKind::KEYWORD, detail: "Input declaration", docs: "require name: type;\nDeclares a required input variable for the scroll." },
    DocItem { label: "provide", kind: CompletionItemKind::KEYWORD, detail: "Output declaration", docs: "provide name: type;\nDeclares a promised output variable. Must be assigned before scroll ends." },
    DocItem { label: "set", kind: CompletionItemKind::KEYWORD, detail: "Variable declaration", docs: "set name: type = expr;\nDeclares a new typed variable. Type is locked at declaration." },
    DocItem { label: "if", kind: CompletionItemKind::KEYWORD, detail: "Conditional", docs: "if condition { ... } else { ... }\nBlocks are expressions: set x: type = if cond { a } else { b };" },
    DocItem { label: "for", kind: CompletionItemKind::KEYWORD, detail: "Loop / expression", docs: "for item in collection { ... }\nAs expression, returns array: set results: T[] = for x in xs { x };" },
    DocItem { label: "while", kind: CompletionItemKind::KEYWORD, detail: "While loop", docs: "while condition { ... }\nLoop with break support." },
    DocItem { label: "match", kind: CompletionItemKind::KEYWORD, detail: "Pattern match", docs: "match target { Pattern.variant => value, ... }\nExhaustiveness checked for enum types." },
    DocItem { label: "break", kind: CompletionItemKind::KEYWORD, detail: "Break loop", docs: "break;\nExit the innermost for/while loop." },
    DocItem { label: "concurrent", kind: CompletionItemKind::KEYWORD, detail: "Parallel execution", docs: "concurrent { ... } or concurrent for x in xs { ... }\nRun operations in parallel." },

    // Error handling
    DocItem { label: "continue", kind: CompletionItemKind::KEYWORD, detail: "Error: null on failure", docs: "-> result: Type | continue;\nOn failure, set result to null and continue." },
    DocItem { label: "retry", kind: CompletionItemKind::KEYWORD, detail: "Error: retry N times", docs: "-> result: Type | retry(3);\nRetry the operation up to N times before failing." },
    DocItem { label: "fallback", kind: CompletionItemKind::KEYWORD, detail: "Error: fallback block", docs: "-> result: Type | fallback { ... };\nExecute fallback block on failure." },

    // Primitives
    DocItem { label: "invoke", kind: CompletionItemKind::FUNCTION, detail: "Agent call", docs: "invoke(agent: \"name\", instructions: \"...\") { schema: Type, tier: cheap|standard|premium } -> result: Type;\nInvoke an LLM agent." },
    DocItem { label: "parallel", kind: CompletionItemKind::FUNCTION, detail: "Fan-out same prompt", docs: "parallel(agents: [...], instructions: \"...\") { schema: Type, require: quorum(N) } -> results: Type[];\nSame prompt to multiple agents with quorum." },
    DocItem { label: "consensus", kind: CompletionItemKind::FUNCTION, detail: "Voting", docs: "consensus(mechanism: \"vote\", threshold: majority) { agents: [...], proposal: \"...\", options: [...] } -> vote: VoteResult;\nStructured voting across agents." },
    DocItem { label: "run", kind: CompletionItemKind::FUNCTION, detail: "Sub-scroll", docs: "run(scroll_path: \"path\", args: { key: value }) -> result: map;\nExecute another scroll." },
    DocItem { label: "elaborate", kind: CompletionItemKind::FUNCTION, detail: "Expand content", docs: "elaborate(input: text, depth: thorough) -> result: str;\nExpand/enrich content via LLM." },
    DocItem { label: "distill", kind: CompletionItemKind::FUNCTION, detail: "Summarize", docs: "distill(input: text, intensity: aggressive) -> result: str;\nSummarize/compress content via LLM." },
    DocItem { label: "validate", kind: CompletionItemKind::FUNCTION, detail: "Semantic check", docs: "validate(input: data, reference: spec) { criteria: [...], mode: strict } -> result: map;\nValidate data against criteria via LLM." },
    DocItem { label: "convert", kind: CompletionItemKind::FUNCTION, detail: "Format conversion", docs: "convert(input: data, to: \"json\") -> result: map;\nParse/convert between formats. Supports schema: { format: \"json\", schema: {...} }." },
    DocItem { label: "aggregate", kind: CompletionItemKind::FUNCTION, detail: "Merge results", docs: "aggregate(results: [a, b], strategy: merge) -> combined: map;\nMerge multiple results into one." },

    // Namespaces
    DocItem { label: "platform", kind: CompletionItemKind::MODULE, detail: "Forgejo API", docs: "platform.get_issue(number: N)\nplatform.create_issue(title: ..., body: ...)\nplatform.close_issue(number: N)\nplatform.list_issues(state: \"open\")" },
    DocItem { label: "fs", kind: CompletionItemKind::MODULE, detail: "Filesystem", docs: "fs.read(path: \"...\") -> content: str\nfs.write(path: \"...\", content: \"...\")\nfs.exists(path: \"...\") -> exists: bool\nfs.list(path: \"...\") -> entries: str[]\nfs.mkdir(path: \"...\")" },
    DocItem { label: "vcs", kind: CompletionItemKind::MODULE, detail: "Version control", docs: "vcs.status() -> status: map\nvcs.commit(message: \"...\")\nvcs.branch(name: \"...\")\nvcs.diff(scope: \"staged\") -> changes: str\nvcs.push(set_upstream: true)" },
    DocItem { label: "test", kind: CompletionItemKind::MODULE, detail: "Test runner", docs: "test.run() -> results: map\ntest.run(filter: \"test_name\") -> results: map\ntest.verify(tool: \"...\", input: code) -> check: map" },

    // Types
    DocItem { label: "str", kind: CompletionItemKind::TYPE_PARAMETER, detail: "String type", docs: "String type. Supports interpolation: \"text {var} more\"" },
    DocItem { label: "int", kind: CompletionItemKind::TYPE_PARAMETER, detail: "Integer type", docs: "Integer type. 64-bit signed." },
    DocItem { label: "float", kind: CompletionItemKind::TYPE_PARAMETER, detail: "Float type", docs: "Floating point type. 64-bit." },
    DocItem { label: "bool", kind: CompletionItemKind::TYPE_PARAMETER, detail: "Boolean type", docs: "Boolean type. true or false." },
    DocItem { label: "map", kind: CompletionItemKind::TYPE_PARAMETER, detail: "Map type", docs: "Untyped key-value map (JSON object)." },
];

// ============================================================================
// LSP Protocol Implementation
// ============================================================================

#[tower_lsp::async_trait]
impl LanguageServer for ScrollLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Scroll Assembly LSP initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.diagnose(&params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.diagnose(&params.text_document.uri, &change.text)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Clear diagnostics on close
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let _ = params; // Position-aware completion would need document state
        let items: Vec<CompletionItem> = COMPLETIONS
            .iter()
            .map(|c| CompletionItem {
                label: c.label.to_string(),
                kind: Some(c.kind),
                detail: Some(c.detail.to_string()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```scroll\n{}\n```", c.docs),
                })),
                ..Default::default()
            })
            .collect();

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let _ = params; // Would need document state for position-aware hover
        // For now, return None (no hover info without word-at-position)
        Ok(None)
    }
}

// ============================================================================
// Entry Point
// ============================================================================

pub async fn handle_lsp() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(ScrollLsp::new);
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
