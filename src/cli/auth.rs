// SPDX-License-Identifier: MIT
use clap::{Args, Subcommand};
use std::io::{self, Write};

use crate::auth::{AuthStore, ForgeType};

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Log in to a git forge
    Login {
        /// Host of the forge (e.g., forgejo.example.com)
        #[arg(short = 'H', long)]
        host: String,

        /// Type of forge (forgejo, github, gitlab, gitea)
        #[arg(short, long)]
        forge_type: String,

        /// URL of the forge (e.g., http://forgejo.example.com)
        #[arg(short = 'U', long)]
        url: String,

        /// Username (optional, will prompt if not provided)
        #[arg(short, long)]
        username: Option<String>,

        /// API token (optional, will prompt if not provided)
        #[arg(short, long)]
        token: Option<String>,
    },
    /// Log out from a git forge
    Logout {
        /// Host of the forge
        #[arg(short = 'H', long)]
        host: String,
    },
}

pub fn handle_auth(args: AuthArgs) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        AuthCommand::Login {
            host,
            forge_type,
            url,
            username,
            token,
        } => {
            let username = username.unwrap_or_else(|| {
                print!("Username: ");
                io::stdout().flush().ok();
                let mut input = String::new();
                io::stdin().read_line(&mut input).ok();
                input.trim().to_string()
            });

            let token = token.unwrap_or_else(|| {
                print!("API Token: ");
                io::stdout().flush().ok();
                let mut input = String::new();
                io::stdin().read_line(&mut input).ok();
                input.trim().to_string()
            });

            let forge_type_enum = match forge_type.to_lowercase().as_str() {
                "forgejo" => ForgeType::Forgejo,
                "github" => ForgeType::GitHub,
                "gitlab" => ForgeType::GitLab,
                "gitea" => ForgeType::Gitea,
                _ => return Err(format!("Unknown forge type: {}", forge_type).into()),
            };

            let store = AuthStore::new()?;
            store.login(&host, forge_type_enum, &url, &username, &token)?;
            println!("Successfully logged in to {} as {}", host, username);
            Ok(())
        }
        AuthCommand::Logout { host } => {
            let store = AuthStore::new()?;
            store.logout(&host)?;
            println!("Successfully logged out from {}", host);
            Ok(())
        }
    }
}
