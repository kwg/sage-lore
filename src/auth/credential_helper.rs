// SPDX-License-Identifier: MIT
use std::io::{self, BufRead};

use super::store::AuthStore;
use super::types::AuthError;

pub fn credential_helper_main() -> Result<(), AuthError> {
    let stdin = io::stdin();
    let mut protocol = String::new();
    let mut host = String::new();

    // Read protocol and host from stdin
    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        if let Some(p) = line.strip_prefix("protocol=") {
            protocol = p.to_string();
        }
        if let Some(h) = line.strip_prefix("host=") {
            host = h.to_string();
        }
    }

    // Silence unused variable warning - protocol is read but not used yet
    let _ = protocol;

    // Get token from auth store
    let token = AuthStore::get_token(&host)?;

    // Output in git credential helper format
    println!("username=oauth");
    println!("password={}", token);

    Ok(())
}
