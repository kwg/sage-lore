// SPDX-License-Identifier: MIT
pub mod auth;
pub mod init;
pub mod lsp;
pub mod run;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "sage")]
#[command(about = "SAGE Method - Security-Aware Generation Engine")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage authentication
    Auth(auth::AuthArgs),
    /// Initialize sage-lore config (user or project)
    Init(init::InitArgs),
    /// Execute a scroll
    Run(run::RunArgs),
    /// Start the Scroll Assembly LSP server (stdio)
    Lsp,
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file if present (silently ignore if missing)
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    match cli.command {
        Command::Auth(args) => auth::handle_auth(args),
        Command::Init(args) => init::handle_init(args),
        Command::Run(args) => run::handle_run(args).await,
        Command::Lsp => lsp::handle_lsp().await,
    }
}
