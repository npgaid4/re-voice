//! ACP Message types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// ACP Message type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    /// Request prompt
    Prompt,
    /// Response to a prompt
    Response,
    /// Broadcast message
    Broadcast,
    /// Agent discovery request
    Discover,
    /// Agent advertisement (capabilities)
    Advertise,
    /// Error message
    Error,
}

/// Message priority
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

/// Message metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    /// Message priority
    pub priority: Option<Priority>,
    /// Time-to-live in seconds
    pub ttl: Option<u64>,
    /// Correlation ID for request-response matching
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

/// Address type for routing
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
}

/// ACP Message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ACPMessage {
    /// Unique message ID
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

    /// Create a broadcast message
    pub fn broadcast(from: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            from: from.into(),
            to: Address::Multiple(vec![]),
            message_type: MessageType::Broadcast,
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
    fn test_frame_encoding_decoding() {
        let msg = ACPMessage::prompt("agent-a", "agent-b", "Test message");
        let encoded = ACPFrame::encode(&msg).unwrap();
        assert!(encoded.starts_with("<ACP>"));
        assert!(encoded.ends_with("</ACP>"));

        let parsed = ACPFrame::parse(&encoded);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].is_ok());
    }
}
