// SPDX-License-Identifier: MIT
use sage_lore::cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    cli::run().await
}
