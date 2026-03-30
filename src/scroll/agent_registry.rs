// SPDX-License-Identifier: MIT
//! Agent registry for loading agent definitions from .sage-method/agents/.
//!
//! Agents are defined in markdown files with XML format:
//! ```xml
//! <agent id="..." name="..." title="...">
//!   <persona>
//!     <role>...</role>
//!     <identity>...</identity>
//!     ...
//!   </persona>
//! </agent>
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Extract the <persona> section from agent XML.
///
/// Returns the full persona XML block as the system prompt.
fn extract_persona(xml_content: &str) -> Option<String> {
    // Find <persona> opening tag
    let persona_start = xml_content.find("<persona>")?;
    let after_opening = &xml_content[persona_start..];

    // Find </persona> closing tag
    let persona_end = after_opening.find("</persona>")?;
    let persona_content = &after_opening[..persona_end + "</persona>".len()];

    Some(persona_content.to_string())
}

/// Extract XML content from markdown code fence.
fn extract_xml_from_markdown(content: &str) -> Option<String> {
    // Find ```xml fence
    let xml_start = content.find("```xml")?;
    let after_fence = &content[xml_start + "```xml".len()..];

    // Find closing ```
    let xml_end = after_fence.find("```")?;
    let xml_content = after_fence[..xml_end].trim();

    Some(xml_content.to_string())
}

/// Agent registry that loads agent definitions from .sage-method/agents/.
pub struct AgentRegistry {
    /// Maps agent name to system prompt (persona XML)
    agents: HashMap<String, String>,
}

impl AgentRegistry {
    /// Create a new empty agent registry.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Load all agents from agents/**/*.md files.
    ///
    /// Searches in three-tier hierarchy (all tiers merged, project wins):
    /// 1. Global: SAGE_LORE_DATADIR/agents/ or SAGE_LORE_HOME/agents/
    /// 2. User: ~/.config/sage-lore/agents/
    /// 3. Project: <base_path>/agents/ or <base_path>/.sage-method/agents/
    ///
    /// Agent names are derived from the filename (without .md extension).
    /// If the same agent name exists in multiple tiers, the most specific wins.
    pub fn load_from_directory(&mut self, base_path: &str) -> Result<(), String> {
        let base = PathBuf::from(base_path);

        // Build candidate directories: global → user → project (project loaded last = wins)
        let mut candidates: Vec<PathBuf> = Vec::new();

        // Global tier: SAGE_LORE_HOME or compile-time SAGE_LORE_DATADIR
        if let Ok(home) = std::env::var("SAGE_LORE_HOME") {
            candidates.push(PathBuf::from(home).join("agents"));
        }
        if let Some(datadir) = option_env!("SAGE_LORE_DATADIR") {
            candidates.push(PathBuf::from(datadir).join("agents"));
        }

        // User tier: XDG config
        if let Some(config_dir) = dirs::config_dir() {
            candidates.push(config_dir.join("sage-lore/agents"));
        }

        // Project tier (most specific, wins on conflict)
        candidates.push(base.join("agents"));
        candidates.push(base.join(".sage-method/agents"));

        let found_any = candidates.iter().any(|d| d.exists());
        if !found_any {
            return Err(format!("No agent directory found in: {:?}", candidates));
        }

        // Load from all tiers — later entries override earlier (project wins)
        for agents_dir in &candidates {
            if agents_dir.exists() {
                self.load_agents_from(agents_dir)?;
            }
        }
        return Ok(());
    }

    /// Load agents from a single directory.
    fn load_agents_from(&mut self, agents_dir: &PathBuf) -> Result<(), String> {

        if !agents_dir.exists() {
            return Err(format!("Agent directory not found: {}", agents_dir.display()));
        }

        // Use glob to find all .md files recursively
        let pattern = format!("{}/**/*.md", agents_dir.display());
        let entries = glob::glob(&pattern)
            .map_err(|e| format!("Failed to read glob pattern: {}", e))?;

        for entry in entries {
            let path = entry.map_err(|e| format!("Failed to read path: {}", e))?;

            // Skip if not a file
            if !path.is_file() {
                continue;
            }

            // Read file content
            let content = fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read agent file {}: {}", path.display(), e))?;

            // Extract XML from markdown
            let xml = match extract_xml_from_markdown(&content) {
                Some(xml) => xml,
                None => {
                    tracing::warn!(path = %path.display(), "No XML code fence found in agent file");
                    continue;
                }
            };

            // Extract persona from XML
            let persona = match extract_persona(&xml) {
                Some(persona) => persona,
                None => {
                    tracing::warn!(path = %path.display(), "No <persona> section found in agent XML");
                    continue;
                }
            };

            // Derive agent name from filename (without .md extension)
            let agent_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("Invalid agent filename: {}", path.display()))?
                .to_string();

            tracing::debug!(
                agent = %agent_name,
                path = %path.display(),
                "Loaded agent definition"
            );

            self.agents.insert(agent_name, persona);
        }

        tracing::info!(count = self.agents.len(), "Loaded agents from registry");
        Ok(())
    }

    /// Get the system prompt for an agent by name.
    pub fn get_system_prompt(&self, agent_name: &str) -> Option<&str> {
        self.agents.get(agent_name).map(|s| s.as_str())
    }

    /// Check if an agent exists in the registry.
    pub fn has_agent(&self, agent_name: &str) -> bool {
        self.agents.contains_key(agent_name)
    }

    /// Get all registered agent names.
    pub fn agent_names(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_persona() {
        let xml = r#"<agent id="test" name="Test">
  <persona>
    <role>Developer</role>
    <identity>Expert</identity>
  </persona>
  <menu>...</menu>
</agent>"#;

        let persona = extract_persona(xml).expect("Should extract persona");
        assert!(persona.contains("<persona>"));
        assert!(persona.contains("<role>Developer</role>"));
        assert!(persona.contains("</persona>"));
        assert!(!persona.contains("<menu>"));
    }

    #[test]
    fn test_extract_xml_from_markdown() {
        let markdown = r#"---
name: "test"
---

```xml
<agent id="test">
  <persona>...</persona>
</agent>
```

More content here.
"#;

        let xml = extract_xml_from_markdown(markdown).expect("Should extract XML");
        assert!(xml.contains("<agent id=\"test\">"));
        assert!(xml.contains("</agent>"));
        assert!(!xml.contains("```"));
    }

    #[test]
    fn test_agent_registry_empty() {
        let registry = AgentRegistry::new();
        assert!(!registry.has_agent("test"));
        assert_eq!(registry.agent_names().len(), 0);
    }
}
