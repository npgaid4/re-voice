//! ACP Message types
//!
//! ACP v3 Protocol - Generic Agent Communication Protocol

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// ACP v3 Constants
// ============================================================================

/// Current protocol version
pub const ACP_VERSION: &str = "ACP/3.0";

// ============================================================================
// Message Types
// ============================================================================

/// ACP Message type (v3 extended)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    // Basic types
    /// Request prompt
    Prompt,
    /// Response to a prompt
    Response,
    /// Streaming output chunk
    Stream,
    /// Error message
    Error,

    // Agent management
    /// Agent discovery request
    Discover,
    /// Agent advertisement (capabilities)
    Advertise,
    /// Heartbeat for keep-alive
    Heartbeat,

    // Control
    /// Cancel a task
    Cancel,
    /// Question from agent
    Question,
    /// Answer to question
    Answer,

    // Pipeline
    /// Pipeline start notification
    PipelineStart,
    /// Pipeline stage completion
    PipelineStage,
    /// Pipeline end notification
    PipelineEnd,
}

/// Message priority (v3 extended)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

/// Message metadata (v3)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    /// Message priority
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    /// Time-to-live in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,
    /// Trace ID for distributed tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Correlation ID for request-response matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

/// Message payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePayload {
    /// Main content (prompt/response text)
    pub content: String,
    /// Optional structured data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl MessagePayload {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

// ============================================================================
// Address Types (v3)
// ============================================================================

/// Agent address identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentAddress {
    /// Agent identifier (e.g., "claude-code@localhost/main")
    pub id: String,
    /// Optional instance identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl AgentAddress {
    /// Create a new agent address
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            instance: None,
        }
    }

    /// Create with instance
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// Parse from string format "agent-type@host/instance"
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        if parts.len() == 2 {
            Some(Self {
                id: parts[0].to_string(),
                instance: Some(parts[1].to_string()),
            })
        } else {
            Some(Self {
                id: s.to_string(),
                instance: None,
            })
        }
    }

    /// Convert to string representation
    pub fn to_address_string(&self) -> String {
        match &self.instance {
            Some(inst) => format!("{}/{}", self.id, inst),
            None => self.id.clone(),
        }
    }
}

/// Capability filter for broadcast
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilityFilter {
    /// Required capabilities (AND condition)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    /// Match tags (OR condition)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Filter by agent type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
}

impl CapabilityFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = Some(caps);
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
}

/// Pipeline stage definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    /// Stage name
    pub name: String,
    /// Agent address for this stage
    pub agent: AgentAddress,
    /// Optional prompt template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_template: Option<String>,
}

impl PipelineStage {
    pub fn new(name: impl Into<String>, agent: AgentAddress) -> Self {
        Self {
            name: name.into(),
            agent,
            prompt_template: None,
        }
    }

    pub fn with_prompt_template(mut self, template: impl Into<String>) -> Self {
        self.prompt_template = Some(template.into());
        self
    }
}

/// Address type for routing (v3 extended)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AddressType {
    /// Single recipient
    Single { address: AgentAddress },
    /// Multiple recipients
    Multiple { addresses: Vec<AgentAddress> },
    /// Broadcast with optional filter
    Broadcast { filter: Option<CapabilityFilter> },
    /// Pipeline routing
    Pipeline { stages: Vec<PipelineStage> },
}

impl AddressType {
    /// Create a single address
    pub fn single(addr: impl Into<String>) -> Self {
        Self::Single {
            address: AgentAddress::new(addr),
        }
    }

    /// Create multiple addresses
    pub fn multiple(addrs: Vec<String>) -> Self {
        Self::Multiple {
            addresses: addrs.into_iter().map(AgentAddress::new).collect(),
        }
    }

    /// Create broadcast address
    pub fn broadcast() -> Self {
        Self::Broadcast { filter: None }
    }

    /// Create broadcast with filter
    pub fn broadcast_with_filter(filter: CapabilityFilter) -> Self {
        Self::Broadcast { filter: Some(filter) }
    }

    /// Create pipeline address
    pub fn pipeline(stages: Vec<PipelineStage>) -> Self {
        Self::Pipeline { stages }
    }

    /// Get all recipient addresses (excludes broadcast and pipeline)
    pub fn recipients(&self) -> Vec<&AgentAddress> {
        match self {
            AddressType::Single { address } => vec![address],
            AddressType::Multiple { addresses } => addresses.iter().collect(),
            AddressType::Broadcast { .. } => vec![],
            AddressType::Pipeline { .. } => vec![],
        }
    }
}

// ============================================================================
// Legacy Address (for backward compatibility)
// ============================================================================

/// Address type for routing (legacy v2 format)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Address {
    /// Single recipient
    Single(String),
    /// Multiple recipients
    Multiple(Vec<String>),
}

impl Address {
    pub fn single(addr: impl Into<String>) -> Self {
        Self::Single(addr.into())
    }

    pub fn multiple(addrs: Vec<String>) -> Self {
        Self::Multiple(addrs)
    }

    pub fn recipients(&self) -> Vec<&String> {
        match self {
            Address::Single(addr) => vec![addr],
            Address::Multiple(addrs) => addrs.iter().collect(),
        }
    }

    /// Convert to v3 AddressType
    pub fn to_v3(&self) -> AddressType {
        match self {
            Address::Single(addr) => AddressType::single(addr.clone()),
            Address::Multiple(addrs) => AddressType::multiple(addrs.clone()),
        }
    }
}

/// ACP Message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ACPMessage {
    /// Unique message ID (UUID v4)
    pub id: String,
    /// Message timestamp (ISO 8601)
    pub timestamp: DateTime<Utc>,
    /// Sender address
    pub from: String,
    /// Recipient address(es)
    pub to: Address,
    /// Message type
    #[serde(rename = "type")]
    pub message_type: MessageType,
    /// Message payload
    pub payload: MessagePayload,
    /// Optional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

// ============================================================================
// ACP Envelope (v3)
// ============================================================================

/// ACP v3 Envelope - wraps message with protocol info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ACPEnvelope {
    /// Protocol version (always "ACP/3.0")
    pub protocol: String,
    /// The wrapped message
    pub message: ACPMessageV3,
    /// Optional envelope-level metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<EnvelopeMetadata>,
}

impl ACPEnvelope {
    /// Create a new envelope with the current protocol version
    pub fn new(message: ACPMessageV3) -> Self {
        Self {
            protocol: ACP_VERSION.to_string(),
            message,
            metadata: None,
        }
    }

    /// Add envelope metadata
    pub fn with_metadata(mut self, metadata: EnvelopeMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Envelope-level metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvelopeMetadata {
    /// Message priority
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    /// Time-to-live in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,
    /// Trace ID for distributed tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Correlation ID for request-response matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

impl EnvelopeMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.ttl = Some(ttl);
        self
    }

    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }
}

// ============================================================================
// ACP Message v3 (with extended address type)
// ============================================================================

/// ACP Message v3 with extended address support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ACPMessageV3 {
    /// Unique message ID (UUID v4)
    pub id: String,
    /// Message timestamp (ISO 8601)
    pub timestamp: DateTime<Utc>,
    /// Sender address
    pub from: AgentAddress,
    /// Recipient address(es) - v3 extended
    pub to: AddressType,
    /// Message type
    #[serde(rename = "type")]
    pub message_type: MessageType,
    /// Message payload
    pub payload: MessagePayload,
    /// Optional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

impl ACPMessageV3 {
    /// Create a new prompt message to a single recipient
    pub fn prompt(from: impl Into<String>, to: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::single(to),
            message_type: MessageType::Prompt,
            payload: MessagePayload::new(content),
            metadata: None,
        }
    }

    /// Create a response message
    pub fn response(
        from: impl Into<String>,
        to: impl Into<String>,
        content: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::single(to),
            message_type: MessageType::Response,
            payload: MessagePayload::new(content),
            metadata: Some(MessageMetadata {
                correlation_id: Some(correlation_id.into()),
                ..Default::default()
            }),
        }
    }

    /// Create a streaming output chunk
    pub fn stream(from: impl Into<String>, to: impl Into<String>, content: impl Into<String>, correlation_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::single(to),
            message_type: MessageType::Stream,
            payload: MessagePayload::new(content),
            metadata: Some(MessageMetadata {
                correlation_id: Some(correlation_id.into()),
                ..Default::default()
            }),
        }
    }

    /// Create a broadcast message with optional filter
    pub fn broadcast(from: impl Into<String>, content: impl Into<String>, filter: Option<CapabilityFilter>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::Broadcast { filter },
            message_type: MessageType::Prompt,
            payload: MessagePayload::new(content),
            metadata: None,
        }
    }

    /// Create a discovery request
    pub fn discover(from: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::broadcast(),
            message_type: MessageType::Discover,
            payload: MessagePayload::new(""),
            metadata: None,
        }
    }

    /// Create an advertisement message
    pub fn advertise(from: impl Into<String>, capabilities_json: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::broadcast(),
            message_type: MessageType::Advertise,
            payload: MessagePayload::new("").with_data(capabilities_json),
            metadata: None,
        }
    }

    /// Create an error message
    pub fn error(from: impl Into<String>, to: impl Into<String>, error_msg: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::single(to),
            message_type: MessageType::Error,
            payload: MessagePayload::new(error_msg),
            metadata: None,
        }
    }

    /// Create a cancel message
    pub fn cancel(from: impl Into<String>, to: impl Into<String>, task_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::single(to),
            message_type: MessageType::Cancel,
            payload: MessagePayload::new("").with_data(serde_json::json!({ "task_id": task_id.into() })),
            metadata: None,
        }
    }

    /// Create a question message
    pub fn question(from: impl Into<String>, to: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::single(to),
            message_type: MessageType::Question,
            payload: MessagePayload::new(content),
            metadata: None,
        }
    }

    /// Create an answer message
    pub fn answer(from: impl Into<String>, to: impl Into<String>, content: impl Into<String>, correlation_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::single(to),
            message_type: MessageType::Answer,
            payload: MessagePayload::new(content),
            metadata: Some(MessageMetadata {
                correlation_id: Some(correlation_id.into()),
                ..Default::default()
            }),
        }
    }

    /// Create a pipeline start message
    pub fn pipeline_start(from: impl Into<String>, stages: Vec<PipelineStage>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::pipeline(stages),
            message_type: MessageType::PipelineStart,
            payload: MessagePayload::new(""),
            metadata: None,
        }
    }

    /// Create a pipeline stage completion message
    pub fn pipeline_stage(
        from: impl Into<String>,
        stage_name: impl Into<String>,
        result: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: AgentAddress::new(from),
            to: AddressType::broadcast(),
            message_type: MessageType::PipelineStage,
            payload: MessagePayload::new("").with_data(serde_json::json!({
                "stage": stage_name.into(),
                "result": result
            })),
            metadata: None,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.metadata = Some(self.metadata.unwrap_or_default());
        if let Some(ref mut meta) = self.metadata {
            meta.priority = Some(priority);
        }
        self
    }

    /// Set TTL
    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.metadata = Some(self.metadata.unwrap_or_default());
        if let Some(ref mut meta) = self.metadata {
            meta.ttl = Some(ttl);
        }
        self
    }

    /// Set trace ID
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.metadata = Some(self.metadata.unwrap_or_default());
        if let Some(ref mut meta) = self.metadata {
            meta.trace_id = Some(trace_id.into());
        }
        self
    }

    /// Convert to envelope
    pub fn into_envelope(self) -> ACPEnvelope {
        ACPEnvelope::new(self)
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl ACPMessage {
    /// Create a new prompt message
    pub fn prompt(from: impl Into<String>, to: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: from.into(),
            to: Address::single(to),
            message_type: MessageType::Prompt,
            payload: MessagePayload::new(content),
            metadata: None,
        }
    }

    /// Create a response message
    pub fn response(
        from: impl Into<String>,
        to: impl Into<String>,
        content: impl Into<String>,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: from.into(),
            to: Address::single(to),
            message_type: MessageType::Response,
            payload: MessagePayload::new(content),
            metadata: Some(MessageMetadata {
                correlation_id: Some(correlation_id.into()),
                ..Default::default()
            }),
        }
    }

    /// Create a broadcast message (legacy)
    pub fn broadcast(from: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: from.into(),
            to: Address::Multiple(vec![]),
            message_type: MessageType::Prompt,
            payload: MessagePayload::new(content),
            metadata: None,
        }
    }

    /// Create a discovery request
    pub fn discover(from: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: from.into(),
            to: Address::Multiple(vec![]),
            message_type: MessageType::Discover,
            payload: MessagePayload::new(""),
            metadata: None,
        }
    }

    /// Create an advertisement message
    pub fn advertise(from: impl Into<String>, capabilities_json: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: from.into(),
            to: Address::Multiple(vec![]),
            message_type: MessageType::Advertise,
            payload: MessagePayload::new("").with_data(capabilities_json),
            metadata: None,
        }
    }

    /// Create an error message
    pub fn error(from: impl Into<String>, to: impl Into<String>, error_msg: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: from.into(),
            to: Address::single(to),
            message_type: MessageType::Error,
            payload: MessagePayload::new(error_msg),
            metadata: None,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.metadata = Some(self.metadata.unwrap_or_default());
        if let Some(ref mut meta) = self.metadata {
            meta.priority = Some(priority);
        }
        self
    }

    /// Set TTL
    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.metadata = Some(self.metadata.unwrap_or_default());
        if let Some(ref mut meta) = self.metadata {
            meta.ttl = Some(ttl);
        }
        self
    }

    /// Set trace ID
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.metadata = Some(self.metadata.unwrap_or_default());
        if let Some(ref mut meta) = self.metadata {
            meta.trace_id = Some(trace_id.into());
        }
        self
    }

    /// Convert to v3 format
    pub fn to_v3(&self) -> ACPMessageV3 {
        let from = AgentAddress::parse(&self.from).unwrap_or_else(|| AgentAddress::new(&self.from));
        ACPMessageV3 {
            id: self.id.clone(),
            timestamp: self.timestamp,
            from,
            to: self.to.to_v3(),
            message_type: self.message_type.clone(),
            payload: self.payload.clone(),
            metadata: self.metadata.clone(),
        }
    }

    /// Convert to v3 envelope
    pub fn into_envelope(self) -> ACPEnvelope {
        ACPEnvelope::new(self.to_v3())
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// ACP frame for PTY transport
/// Format: <ACP>{json}</ACP>
pub struct ACPFrame;

impl ACPFrame {
    pub const START_MARKER: &'static str = "<ACP>";
    pub const END_MARKER: &'static str = "</ACP>";

    /// Encode a message as a framed string
    pub fn encode(message: &ACPMessage) -> Result<String, serde_json::Error> {
        let json = message.to_json()?;
        Ok(format!("{}{}{}", Self::START_MARKER, json, Self::END_MARKER))
    }

    /// Parse framed messages from raw output
    pub fn parse(output: &str) -> Vec<Result<ACPMessage, ACPParseError>> {
        let mut messages = Vec::new();
        let mut remaining = output;

        while let Some(start) = remaining.find(Self::START_MARKER) {
            let after_start = &remaining[start + Self::START_MARKER.len()..];
            if let Some(end) = after_start.find(Self::END_MARKER) {
                let json = &after_start[..end];
                messages.push(ACPMessage::from_json(json).map_err(ACPParseError::JsonError));
                remaining = &after_start[end + Self::END_MARKER.len()..];
            } else {
                break;
            }
        }

        messages
    }
}

/// ACP frame parse error
#[derive(Debug)]
pub enum ACPParseError {
    JsonError(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = ACPMessage::prompt("agent-a", "agent-b", "Hello!");
        assert_eq!(msg.message_type, MessageType::Prompt);
        assert_eq!(msg.from, "agent-a");
        assert_eq!(msg.payload.content, "Hello!");
    }

    #[test]
    fn test_message_v3_creation() {
        let msg = ACPMessageV3::prompt("agent-a", "agent-b", "Hello v3!");
        assert_eq!(msg.message_type, MessageType::Prompt);
        assert_eq!(msg.from.id, "agent-a");
        assert_eq!(msg.payload.content, "Hello v3!");
    }

    #[test]
    fn test_agent_address() {
        let addr = AgentAddress::parse("claude-code@localhost/main").unwrap();
        assert_eq!(addr.id, "claude-code@localhost");
        assert_eq!(addr.instance, Some("main".to_string()));

        let simple = AgentAddress::parse("simple-agent").unwrap();
        assert_eq!(simple.id, "simple-agent");
        assert_eq!(simple.instance, None);
    }

    #[test]
    fn test_address_type_single() {
        let addr = AddressType::single("agent-1");
        match addr {
            AddressType::Single { address } => assert_eq!(address.id, "agent-1"),
            _ => panic!("Expected Single address type"),
        }
    }

    #[test]
    fn test_address_type_broadcast_with_filter() {
        let filter = CapabilityFilter::new()
            .with_capabilities(vec!["translation".into()])
            .with_tags(vec!["multilingual".into()]);
        let addr = AddressType::broadcast_with_filter(filter);
        match addr {
            AddressType::Broadcast { filter: Some(f) } => {
                assert_eq!(f.capabilities, Some(vec!["translation".to_string()]));
            }
            _ => panic!("Expected Broadcast address type"),
        }
    }

    #[test]
    fn test_pipeline_stage() {
        let stage = PipelineStage::new("translate", AgentAddress::new("translator@local"))
            .with_prompt_template("Translate: {{input}}");
        assert_eq!(stage.name, "translate");
        assert_eq!(stage.prompt_template, Some("Translate: {{input}}".to_string()));
    }

    #[test]
    fn test_envelope_creation() {
        let msg = ACPMessageV3::prompt("agent-a", "agent-b", "Test");
        let envelope = msg.into_envelope();
        assert_eq!(envelope.protocol, "ACP/3.0");
    }

    #[test]
    fn test_envelope_serialization() {
        let msg = ACPMessageV3::prompt("agent-a", "agent-b", "Test");
        let envelope = msg.into_envelope();
        let json = envelope.to_json().unwrap();

        // Verify it can be deserialized back
        let parsed = ACPEnvelope::from_json(&json).unwrap();
        assert_eq!(parsed.protocol, "ACP/3.0");
    }

    #[test]
    fn test_frame_encoding_decoding() {
        let msg = ACPMessage::prompt("agent-a", "agent-b", "Test message");
        let encoded = ACPFrame::encode(&msg).unwrap();
        assert!(encoded.starts_with("<ACP>"));
        assert!(encoded.ends_with("</ACP>"));

        let parsed = ACPFrame::parse(&encoded);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].is_ok());
    }

    #[test]
    fn test_message_type_v3_extended() {
        let msg = ACPMessageV3::stream("agent-a", "agent-b", "chunk", "corr-1");
        assert_eq!(msg.message_type, MessageType::Stream);

        let cancel = ACPMessageV3::cancel("agent-a", "agent-b", "task-123");
        assert_eq!(cancel.message_type, MessageType::Cancel);

        let question = ACPMessageV3::question("agent-a", "agent-b", "What is this?");
        assert_eq!(question.message_type, MessageType::Question);
    }

    #[test]
    fn test_legacy_to_v3_conversion() {
        let legacy = ACPMessage::prompt("agent-a", "agent-b", "Convert me");
        let v3 = legacy.to_v3();

        assert_eq!(v3.id, legacy.id);
        assert_eq!(v3.from.id, "agent-a");
        assert_eq!(v3.message_type, MessageType::Prompt);
    }
}
