//! Agent Card and Capability definitions

use serde::{Deserialize, Serialize};

/// Transport type for agent communication
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// PTY (pseudo-terminal) for TUI apps
    Pty,
    /// Standard I/O for local processes
    Stdio,
    /// WebSocket for remote communication
    WebSocket,
    /// HTTP fallback
    Http,
}

/// Agent capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// Unique capability ID (e.g., "translation")
    pub id: String,
    /// Human-readable name (e.g., "Translation")
    pub name: String,
    /// Optional tags for filtering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl Capability {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            tags: None,
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }
}

/// Agent Card - describes an agent's identity and capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    /// Unique agent ID (e.g., "claude-code@localhost")
    pub id: String,
    /// Protocol version
    pub protocol: String,
    /// Human-readable name
    pub name: String,
    /// Agent capabilities
    pub capabilities: Vec<Capability>,
    /// Connection endpoint
    pub endpoint: String,
    /// Transport type
    pub transport: Transport,
}

impl AgentCard {
    /// Create a new agent card
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        endpoint: impl Into<String>,
        transport: Transport,
    ) -> Self {
        Self {
            id: id.into(),
            protocol: "ACP/1.0".to_string(),
            name: name.into(),
            capabilities: Vec::new(),
            endpoint: endpoint.into(),
            transport,
        }
    }

    /// Add a capability
    pub fn with_capability(mut self, capability: Capability) -> Self {
        self.capabilities.push(capability);
        self
    }

    /// Add multiple capabilities
    pub fn with_capabilities(mut self, capabilities: Vec<Capability>) -> Self {
        self.capabilities.extend(capabilities);
        self
    }

    /// Check if agent has a specific capability
    pub fn has_capability(&self, capability_id: &str) -> bool {
        self.capabilities.iter().any(|c| c.id == capability_id)
    }

    /// Check if agent has any of the specified capabilities
    pub fn has_any_capability(&self, capability_ids: &[&str]) -> bool {
        capability_ids
            .iter()
            .any(|id| self.capabilities.iter().any(|c| c.id == *id))
    }

    /// Check if agent has all of the specified capabilities
    pub fn has_all_capabilities(&self, capability_ids: &[&str]) -> bool {
        capability_ids
            .iter()
            .all(|id| self.capabilities.iter().any(|c| c.id == *id))
    }

    /// Check if agent has capability with specific tag
    pub fn has_capability_with_tag(&self, capability_id: &str, tag: &str) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.id == capability_id && c.tags.as_ref().map_or(false, |t| t.contains(&tag.to_string())))
    }

    /// Create a Claude Code agent card
    pub fn claude_code(instance_id: &str) -> Self {
        Self::new(
            format!("claude-code-{}", instance_id),
            format!("Claude Code ({})", instance_id),
            format!("acp://claude-code@localhost/{}", instance_id),
            Transport::Pty,
        )
        .with_capabilities(vec![
            Capability::new("translation", "Translation").with_tags(vec!["multilingual".into()]),
            Capability::new("code-generation", "Code Generation"),
            Capability::new("code-review", "Code Review"),
            Capability::new("analysis", "Analysis"),
            Capability::new("writing", "Writing"),
            Capability::new("summarization", "Summarization"),
        ])
    }

    /// Create a generic Codex agent card
    pub fn codex(instance_id: &str) -> Self {
        Self::new(
            format!("codex-{}", instance_id),
            format!("Codex ({})", instance_id),
            format!("acp://codex@localhost/{}", instance_id),
            Transport::Pty,
        )
        .with_capabilities(vec![
            Capability::new("code-generation", "Code Generation"),
            Capability::new("code-review", "Code Review"),
            Capability::new("debugging", "Debugging"),
        ])
    }
}

/// Discovery query for finding agents
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscoveryQuery {
    /// Filter by capability IDs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    /// Filter by capability tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Filter by transport type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<Transport>,
}

impl DiscoveryQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn with_transport(mut self, transport: Transport) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Check if an agent card matches this query
    pub fn matches(&self, card: &AgentCard) -> bool {
        // Check transport
        if let Some(ref transport) = self.transport {
            if card.transport != *transport {
                return false;
            }
        }

        // Check capabilities (agent must have ALL requested capabilities)
        if let Some(ref required_caps) = self.capabilities {
            for cap in required_caps {
                if !card.has_capability(cap) {
                    return false;
                }
            }
        }

        // Check tags (agent must have capability with matching tag)
        if let Some(ref required_tags) = self.tags {
            for tag in required_tags {
                let has_tag = card
                    .capabilities
                    .iter()
                    .any(|c| c.tags.as_ref().map_or(false, |t| t.contains(tag)));
                if !has_tag {
                    return false;
                }
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_creation() {
        let card = AgentCard::claude_code("main");
        assert_eq!(card.id, "claude-code-main");
        assert_eq!(card.protocol, "ACP/1.0");
        assert!(card.has_capability("translation"));
        assert!(card.has_capability("code-generation"));
    }

    #[test]
    fn test_capability_matching() {
        let card = AgentCard::claude_code("test");

        // Test has_capability
        assert!(card.has_capability("translation"));
        assert!(!card.has_capability("nonexistent"));

        // Test has_any_capability
        assert!(card.has_any_capability(&["translation", "nonexistent"]));
        assert!(!card.has_any_capability(&["nonexistent1", "nonexistent2"]));

        // Test has_all_capabilities
        assert!(card.has_all_capabilities(&["translation", "analysis"]));
        assert!(!card.has_all_capabilities(&["translation", "nonexistent"]));
    }

    #[test]
    fn test_discovery_query() {
        let card = AgentCard::claude_code("test");

        // Query by capability
        let query = DiscoveryQuery::new().with_capabilities(vec!["translation".into()]);
        assert!(query.matches(&card));

        // Query by non-existent capability
        let query = DiscoveryQuery::new().with_capabilities(vec!["nonexistent".into()]);
        assert!(!query.matches(&card));

        // Query by multiple capabilities (all must match)
        let query =
            DiscoveryQuery::new().with_capabilities(vec!["translation".into(), "analysis".into()]);
        assert!(query.matches(&card));

        // Query with partial match should fail
        let query = DiscoveryQuery::new()
            .with_capabilities(vec!["translation".into(), "nonexistent".into()]);
        assert!(!query.matches(&card));
    }
}
