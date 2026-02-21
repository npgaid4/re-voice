//! Agent Card and Skill definitions
//!
//! A2A Protocol Compliant - Agent-to-Agent Communication
//! Based on: https://github.com/google/A2A

use serde::{Deserialize, Serialize};

// ============================================================================
// Protocol Version
// ============================================================================

/// A2A Protocol version
pub const A2A_PROTOCOL_VERSION: &str = "0.3.0";

// ============================================================================
// Transport Types
// ============================================================================

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

// ============================================================================
// JSON Schema (for skill input/output definitions)
// ============================================================================

/// JSON Schema for skill input/output definitions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JSONSchema {
    /// Schema type (e.g., "object", "string", "array")
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Properties for object type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Map<String, serde_json::Value>>,
    /// Required properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    /// Items schema for array type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JSONSchema>>,
}

impl JSONSchema {
    /// Create a simple type schema
    pub fn simple(schema_type: impl Into<String>) -> Self {
        Self {
            schema_type: Some(schema_type.into()),
            ..Default::default()
        }
    }

    /// Create an object schema with properties
    pub fn object(properties: serde_json::Map<String, serde_json::Value>) -> Self {
        Self {
            schema_type: Some("object".to_string()),
            properties: Some(properties),
            ..Default::default()
        }
    }

    /// Add description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set required fields
    pub fn with_required(mut self, required: Vec<String>) -> Self {
        self.required = Some(required);
        self
    }
}

// ============================================================================
// Authentication
// ============================================================================

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Authentication {
    /// Supported authentication schemes
    pub schemes: Vec<String>,
}

impl Authentication {
    pub fn new(schemes: Vec<String>) -> Self {
        Self { schemes }
    }

    pub fn api_key() -> Self {
        Self { schemes: vec!["apiKey".to_string()] }
    }

    pub fn oauth2() -> Self {
        Self { schemes: vec!["OAuth2".to_string()] }
    }

    pub fn none() -> Self {
        Self { schemes: vec!["none".to_string()] }
    }
}

// ============================================================================
// Provider (Organization Info)
// ============================================================================

/// Provider/organization information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    /// Organization name
    pub organization: String,
    /// Organization URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Provider {
    pub fn new(organization: impl Into<String>) -> Self {
        Self {
            organization: organization.into(),
            url: None,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }
}

// ============================================================================
// Agent Capabilities (A2A)
// ============================================================================

/// Agent capabilities (technical features)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentCapabilities {
    /// Supports streaming responses
    #[serde(default)]
    pub streaming: bool,
    /// Supports push notifications
    #[serde(default)]
    pub push_notifications: bool,
    /// Supports state transition history
    #[serde(default, rename = "stateTransitionHistory")]
    pub state_transition_history: bool,
}

impl AgentCapabilities {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    pub fn with_push_notifications(mut self, push_notifications: bool) -> Self {
        self.push_notifications = push_notifications;
        self
    }

    pub fn with_state_transition_history(mut self, history: bool) -> Self {
        self.state_transition_history = history;
        self
    }
}

// ============================================================================
// Skill (A2A)
// ============================================================================

/// Agent skill - specific capability the agent can perform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique skill identifier
    pub id: String,
    /// Human-readable skill name
    pub name: String,
    /// What this skill does
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Keywords for discovery
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Example prompts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<String>>,
    /// Input JSON Schema
    #[serde(skip_serializing_if = "Option::is_none", rename = "inputSchema")]
    pub input_schema: Option<JSONSchema>,
    /// Output JSON Schema
    #[serde(skip_serializing_if = "Option::is_none", rename = "outputSchema")]
    pub output_schema: Option<JSONSchema>,
    /// Supported input modes
    #[serde(skip_serializing_if = "Option::is_none", rename = "inputModes")]
    pub input_modes: Option<Vec<String>>,
    /// Supported output modes
    #[serde(skip_serializing_if = "Option::is_none", rename = "outputModes")]
    pub output_modes: Option<Vec<String>>,
}

impl Skill {
    /// Create a new skill
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            tags: None,
            examples: None,
            input_schema: None,
            output_schema: None,
            input_modes: None,
            output_modes: None,
        }
    }

    /// Add description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    /// Add examples
    pub fn with_examples(mut self, examples: Vec<String>) -> Self {
        self.examples = Some(examples);
        self
    }

    /// Add input schema
    pub fn with_input_schema(mut self, schema: JSONSchema) -> Self {
        self.input_schema = Some(schema);
        self
    }

    /// Add output schema
    pub fn with_output_schema(mut self, schema: JSONSchema) -> Self {
        self.output_schema = Some(schema);
        self
    }

    /// Set input modes
    pub fn with_input_modes(mut self, modes: Vec<String>) -> Self {
        self.input_modes = Some(modes);
        self
    }

    /// Set output modes
    pub fn with_output_modes(mut self, modes: Vec<String>) -> Self {
        self.output_modes = Some(modes);
        self
    }

    /// Check if has a specific tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.as_ref().map_or(false, |t| t.iter().any(|x| x == tag))
    }
}

// Legacy type alias for backward compatibility
pub type Capability = Skill;

// ============================================================================
// Agent Card (A2A Compliant)
// ============================================================================

/// Agent Card - A2A compliant agent identity document
///
/// Should be hosted at: https://<base-url>/.well-known/agent.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    /// Agent's display name
    pub name: String,
    /// Brief description of the agent's purpose
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Communication endpoint URL
    pub url: String,
    /// Agent version
    pub version: String,
    /// A2A protocol version
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Provider/organization information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    /// Technical capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<AgentCapabilities>,
    /// Authentication schemes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<Authentication>,
    /// Default input modes (e.g., ["text/plain"])
    #[serde(skip_serializing_if = "Option::is_none", rename = "defaultInputModes")]
    pub default_input_modes: Option<Vec<String>>,
    /// Default output modes (e.g., ["text/plain", "application/json"])
    #[serde(skip_serializing_if = "Option::is_none", rename = "defaultOutputModes")]
    pub default_output_modes: Option<Vec<String>>,
    /// Skills (specific capabilities/tasks the agent can perform)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<Skill>>,

    // --- Internal fields (not part of A2A spec) ---
    /// Unique agent ID (internal)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Transport type (internal)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<Transport>,
}

impl AgentCard {
    /// Create a new A2A-compliant agent card
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            url: url.into(),
            version: "1.0.0".to_string(),
            protocol_version: A2A_PROTOCOL_VERSION.to_string(),
            provider: None,
            capabilities: None,
            authentication: None,
            default_input_modes: None,
            default_output_modes: None,
            skills: None,
            id: None,
            transport: None,
        }
    }

    /// Add description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set provider
    pub fn with_provider(mut self, provider: Provider) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Set capabilities
    pub fn with_capabilities(mut self, capabilities: AgentCapabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Set authentication
    pub fn with_authentication(mut self, auth: Authentication) -> Self {
        self.authentication = Some(auth);
        self
    }

    /// Set default input modes
    pub fn with_default_input_modes(mut self, modes: Vec<String>) -> Self {
        self.default_input_modes = Some(modes);
        self
    }

    /// Set default output modes
    pub fn with_default_output_modes(mut self, modes: Vec<String>) -> Self {
        self.default_output_modes = Some(modes);
        self
    }

    /// Add a skill
    pub fn with_skill(mut self, skill: Skill) -> Self {
        self.skills.get_or_insert_with(Vec::new).push(skill);
        self
    }

    /// Add multiple skills
    pub fn with_skills(mut self, skills: Vec<Skill>) -> Self {
        self.skills.get_or_insert_with(Vec::new).extend(skills);
        self
    }

    /// Set internal ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set transport
    pub fn with_transport(mut self, transport: Transport) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Check if agent has a specific skill
    pub fn has_skill(&self, skill_id: &str) -> bool {
        self.skills.as_ref().map_or(false, |s| s.iter().any(|skill| skill.id == skill_id))
    }

    /// Check if agent has any of the specified skills
    pub fn has_any_skill(&self, skill_ids: &[&str]) -> bool {
        self.skills.as_ref().map_or(false, |s| {
            s.iter().any(|skill| skill_ids.contains(&skill.id.as_str()))
        })
    }

    /// Check if agent has all of the specified skills
    pub fn has_all_skills(&self, skill_ids: &[&str]) -> bool {
        if skill_ids.is_empty() {
            return true;
        }
        self.skills.as_ref().map_or(false, |s| {
            skill_ids.iter().all(|id| s.iter().any(|skill| &skill.id == *id))
        })
    }

    /// Check if agent has skill with specific tag
    pub fn has_skill_with_tag(&self, skill_id: &str, tag: &str) -> bool {
        self.skills.as_ref().map_or(false, |s| {
            s.iter().any(|skill| skill.id == skill_id && skill.has_tag(tag))
        })
    }

    /// Check if agent matches a capability filter
    pub fn matches_filter(&self, filter: &crate::acp::message::CapabilityFilter) -> bool {
        // Check skills (AND condition - must have all)
        if let Some(ref required_skills) = filter.capabilities {
            for skill in required_skills {
                if !self.has_skill(skill) {
                    return false;
                }
            }
        }

        // Check tags (OR condition - must have at least one)
        if let Some(ref required_tags) = filter.tags {
            let has_any_tag = self.skills.as_ref().map_or(false, |s| {
                s.iter().any(|skill| {
                    skill.tags.as_ref().map_or(false, |tags| {
                        tags.iter().any(|t| required_tags.contains(t))
                    })
                })
            });
            if !has_any_tag {
                return false;
            }
        }

        // Check agent type (matches name or ID)
        if let Some(ref agent_type) = filter.agent_type {
            let name_match = self.name.to_lowercase().contains(&agent_type.to_lowercase());
            let id_match = self.id.as_ref().map_or(false, |id| {
                id.to_lowercase().contains(&agent_type.to_lowercase())
            });
            if !name_match && !id_match {
                return false;
            }
        }

        true
    }

    /// Create a Claude Code agent card (A2A compliant)
    pub fn claude_code(instance_id: &str) -> Self {
        let id = format!("claude-code@localhost/{}", instance_id);
        let url = format!("acp://{}", id);

        Self::new(
            format!("Claude Code ({})", instance_id),
            &url,
        )
        .with_id(&id)
        .with_description("Anthropic's Claude Code CLI agent")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_provider(
            Provider::new("Anthropic")
                .with_url("https://anthropic.com")
        )
        .with_capabilities(
            AgentCapabilities::new()
                .with_streaming(true)
                .with_state_transition_history(true)
        )
        .with_authentication(Authentication::none())
        .with_default_input_modes(vec!["text/plain".to_string()])
        .with_default_output_modes(vec!["text/plain".to_string(), "application/json".to_string()])
        .with_transport(Transport::Pty)
        .with_skills(vec![
            Skill::new("translation", "Translation")
                .with_description("Translate text between languages")
                .with_tags(vec!["multilingual".to_string()])
                .with_examples(vec![
                    "Translate this text to Japanese".to_string()
                ])
                .with_input_modes(vec!["text/plain".to_string()])
                .with_output_modes(vec!["text/plain".to_string()]),
            Skill::new("code-generation", "Code Generation")
                .with_description("Generate code in various programming languages")
                .with_tags(vec!["programming".to_string(), "coding".to_string()])
                .with_examples(vec![
                    "Write a Python function to sort a list".to_string()
                ])
                .with_input_modes(vec!["text/plain".to_string()])
                .with_output_modes(vec!["text/plain".to_string(), "application/json".to_string()]),
            Skill::new("code-review", "Code Review")
                .with_description("Review code for quality and best practices")
                .with_tags(vec!["programming".to_string()])
                .with_examples(vec![
                    "Review this pull request for potential issues".to_string()
                ]),
            Skill::new("analysis", "Analysis")
                .with_description("Analyze code, data, or text")
                .with_tags(vec!["analysis".to_string()])
                .with_examples(vec![
                    "Analyze the architecture of this codebase".to_string()
                ]),
            Skill::new("writing", "Writing")
                .with_description("Generate written content")
                .with_tags(vec!["content".to_string()])
                .with_examples(vec![
                    "Write documentation for this API".to_string()
                ]),
            Skill::new("summarization", "Summarization")
                .with_description("Summarize long texts or documents")
                .with_tags(vec!["content".to_string()])
                .with_examples(vec![
                    "Summarize this research paper".to_string()
                ]),
        ])
    }

    /// Create a Codex agent card (A2A compliant)
    pub fn codex(instance_id: &str) -> Self {
        let id = format!("codex@localhost/{}", instance_id);
        let url = format!("acp://{}", id);

        Self::new(
            format!("Codex ({})", instance_id),
            &url,
        )
        .with_id(&id)
        .with_description("OpenAI Codex agent")
        .with_provider(
            Provider::new("OpenAI")
                .with_url("https://openai.com")
        )
        .with_capabilities(
            AgentCapabilities::new()
                .with_streaming(true)
        )
        .with_transport(Transport::Pty)
        .with_skills(vec![
            Skill::new("code-generation", "Code Generation")
                .with_tags(vec!["programming".to_string()]),
            Skill::new("code-review", "Code Review")
                .with_tags(vec!["programming".to_string()]),
            Skill::new("debugging", "Debugging")
                .with_tags(vec!["programming".to_string()])
                .with_examples(vec![
                    "Find the bug in this code".to_string()
                ]),
        ])
    }

    /// Export to JSON for .well-known/agent.json
    pub fn to_a2a_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

// ============================================================================
// Discovery Query
// ============================================================================

/// Discovery query for finding agents (A2A compatible)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscoveryQuery {
    /// Filter by skill IDs (AND condition)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    /// Filter by skill tags (OR condition)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Filter by agent type/name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// Filter by streaming support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    /// Filter by push notifications support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_notifications: Option<bool>,

    // Legacy fields (internal use)
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

    pub fn with_agent_type(mut self, agent_type: impl Into<String>) -> Self {
        self.agent_type = Some(agent_type.into());
        self
    }

    pub fn with_streaming(mut self, streaming: bool) -> Self {
        self.streaming = Some(streaming);
        self
    }

    pub fn with_push_notifications(mut self, push_notifications: bool) -> Self {
        self.push_notifications = Some(push_notifications);
        self
    }

    pub fn with_transport(mut self, transport: Transport) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Convert to CapabilityFilter
    pub fn to_capability_filter(&self) -> crate::acp::message::CapabilityFilter {
        let mut filter = crate::acp::message::CapabilityFilter::new();

        if let Some(ref caps) = self.capabilities {
            filter = filter.with_capabilities(caps.clone());
        }
        if let Some(ref tags) = self.tags {
            filter = filter.with_tags(tags.clone());
        }
        if let Some(ref agent_type) = self.agent_type {
            filter = filter.with_agent_type(agent_type.clone());
        }

        filter
    }

    /// Check if an agent card matches this query
    pub fn matches(&self, card: &AgentCard) -> bool {
        // Check streaming capability
        if let Some(streaming_required) = self.streaming {
            if streaming_required {
                let has_streaming = card.capabilities.as_ref().map_or(false, |c| c.streaming);
                if !has_streaming {
                    return false;
                }
            }
        }

        // Check push notifications capability
        if let Some(push_required) = self.push_notifications {
            if push_required {
                let has_push = card.capabilities.as_ref().map_or(false, |c| c.push_notifications);
                if !has_push {
                    return false;
                }
            }
        }

        // Check skills (AND condition)
        if let Some(ref required_skills) = self.capabilities {
            for skill in required_skills {
                if !card.has_skill(skill) {
                    return false;
                }
            }
        }

        // Check tags (OR condition)
        if let Some(ref required_tags) = self.tags {
            let has_tag = card.skills.as_ref().map_or(false, |s| {
                s.iter().any(|skill| {
                    skill.tags.as_ref().map_or(false, |tags| {
                        tags.iter().any(|t| required_tags.contains(t))
                    })
                })
            });
            if !has_tag {
                return false;
            }
        }

        // Check agent type
        if let Some(ref agent_type) = self.agent_type {
            let type_lower = agent_type.to_lowercase();
            let name_match = card.name.to_lowercase().contains(&type_lower);
            let id_match = card.id.as_ref().map_or(false, |id| {
                id.to_lowercase().contains(&type_lower)
            });
            if !name_match && !id_match {
                return false;
            }
        }

        // Check transport
        if let Some(ref transport) = self.transport {
            if card.transport.as_ref() != Some(transport) {
                return false;
            }
        }

        true
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_a2a_compliant() {
        let card = AgentCard::claude_code("main");

        assert_eq!(card.protocol_version, A2A_PROTOCOL_VERSION);
        assert!(card.description.is_some());
        assert!(card.provider.is_some());
        assert!(card.capabilities.is_some());
        assert!(card.skills.is_some());

        // Verify A2A JSON output
        let json = card.to_a2a_json().unwrap();
        assert!(json.contains("\"protocolVersion\":"));
        assert!(json.contains("\"skills\":"));
    }

    #[test]
    fn test_skill_creation() {
        let skill = Skill::new("translation", "Translation")
            .with_description("Translate text between languages")
            .with_tags(vec!["multilingual".to_string()])
            .with_examples(vec!["Translate this to Japanese".to_string()]);

        assert_eq!(skill.id, "translation");
        assert!(skill.has_tag("multilingual"));
    }

    #[test]
    fn test_agent_capabilities() {
        let caps = AgentCapabilities::new()
            .with_streaming(true)
            .with_push_notifications(false)
            .with_state_transition_history(true);

        assert!(caps.streaming);
        assert!(!caps.push_notifications);
        assert!(caps.state_transition_history);
    }

    #[test]
    fn test_discovery_query() {
        let card = AgentCard::claude_code("test");

        // Query by skill
        let query = DiscoveryQuery::new().with_capabilities(vec!["translation".into()]);
        assert!(query.matches(&card));

        // Query by streaming
        let query = DiscoveryQuery::new().with_streaming(true);
        assert!(query.matches(&card));

        // Query by agent type
        let query = DiscoveryQuery::new().with_agent_type("claude");
        assert!(query.matches(&card));
    }

    #[test]
    fn test_matches_filter() {
        use crate::acp::message::CapabilityFilter;

        let card = AgentCard::claude_code("test");

        // Filter by skill
        let filter = CapabilityFilter::new().with_capabilities(vec!["translation".into()]);
        assert!(card.matches_filter(&filter));

        // Filter by tag
        let filter = CapabilityFilter::new().with_tags(vec!["multilingual".into()]);
        assert!(card.matches_filter(&filter));
    }

    #[test]
    fn test_a2a_json_output() {
        let card = AgentCard::new("TestAgent", "https://example.com/agent")
            .with_description("A test agent")
            .with_capabilities(AgentCapabilities::new().with_streaming(true))
            .with_skill(Skill::new("test-skill", "Test Skill"));

        let json = card.to_a2a_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["name"], "TestAgent");
        assert_eq!(parsed["protocolVersion"], A2A_PROTOCOL_VERSION);
        assert_eq!(parsed["capabilities"]["streaming"], true);
    }
}
