//! Agent Adapter - protocol conversion layer between ACP and native CLI

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::agent::{AgentCard, Capability};

/// Adapter error types
#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Communication failed: {0}")]
    CommunicationFailed(String),

    #[error("Task execution failed: {0}")]
    TaskExecutionFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Agent not ready")]
    NotReady,

    #[error("Task cancelled")]
    Cancelled,
}

/// Agent execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentExecutionStatus {
    /// Agent is idle and ready for tasks
    Idle,
    /// Agent is busy with a task
    Busy { task_id: String },
    /// Agent encountered an error
    Error { message: String },
    /// Agent is shutting down
    Shutdown,
}

/// Task payload extracted from ACP message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPayload {
    /// Main content/prompt
    pub content: String,
    /// Optional structured data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl TaskPayload {
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

/// Context entry for multi-agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    /// Agent that created this entry
    pub agent_id: String,
    /// Summary of the interaction
    pub summary: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Shared context between agents
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharedContext {
    /// Conversation history
    pub conversation_history: Vec<ContextEntry>,
    /// Shared file paths
    pub shared_files: Vec<String>,
    /// Additional metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl SharedContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_history(mut self, history: Vec<ContextEntry>) -> Self {
        self.conversation_history = history;
        self
    }

    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.shared_files = files;
        self
    }

    pub fn add_entry(&mut self, agent_id: String, summary: String) {
        self.conversation_history.push(ContextEntry {
            agent_id,
            summary,
            timestamp: Utc::now(),
        });
    }
}

/// Task request
#[derive(Debug, Clone)]
pub struct TaskRequest {
    /// Unique task ID
    pub task_id: Uuid,
    /// Task payload
    pub payload: TaskPayload,
    /// Shared context from other agents
    pub context: Option<SharedContext>,
}

impl TaskRequest {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            task_id: Uuid::new_v4(),
            payload: TaskPayload::new(content),
            context: None,
        }
    }

    pub fn with_context(mut self, context: SharedContext) -> Self {
        self.context = Some(context);
        self
    }
}

/// Stream output chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Text content
    pub text: String,
    /// Whether this is the final chunk
    pub is_final: bool,
}

impl StreamChunk {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_final: false,
        }
    }

    pub fn final_chunk(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_final: true,
        }
    }
}

/// Task completion result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Final output text
    pub output: String,
    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

impl TaskResult {
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Events from adapter
#[derive(Debug, Clone)]
pub enum AdapterEvent {
    /// Output chunk received
    OutputChunk { task_id: Uuid, chunk: StreamChunk },
    /// Task completed
    TaskComplete { task_id: Uuid, result: TaskResult },
    /// Error occurred
    Error { task_id: Uuid, error: String },
}

/// Parsed output content type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputContentType {
    /// Plain text
    Text,
    /// Code block with language
    CodeBlock { language: String },
    /// Thinking/reflection content
    Thinking,
    /// Tool usage
    ToolUse { tool_name: String },
    /// Error message
    ErrorMessage,
}

/// Parsed output
#[derive(Debug, Clone)]
pub struct ParsedOutput {
    /// Content
    pub content: String,
    /// Content type
    pub content_type: OutputContentType,
    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Input converter trait: ACP -> Native CLI input
pub trait InputConverter: Send + Sync {
    /// Convert ACP task to native input
    fn convert_input(&self, task: &TaskPayload) -> Result<String, AdapterError>;

    /// Embed shared context into prompt
    fn embed_context(&self, prompt: &str, context: &SharedContext) -> String;
}

/// Output converter trait: Native CLI output -> ACP
pub trait OutputConverter: Send + Sync {
    /// Parse raw output
    fn parse_output(&self, raw_output: &str) -> Result<Vec<ParsedOutput>, AdapterError>;

    /// Convert parsed output to stream chunk
    fn to_stream_chunk(&self, parsed: &ParsedOutput) -> Option<StreamChunk>;

    /// Check if prompt is complete (agent is waiting for input)
    fn is_prompt_complete(&self, output: &str) -> bool;
}

/// Agent adapter trait
///
/// Note: Only requires Send (not Sync) to accommodate PTY-based adapters
#[async_trait]
pub trait AgentAdapter: Send {
    /// Get agent information
    fn agent_card(&self) -> &AgentCard;

    /// Get agent capabilities
    fn capabilities(&self) -> &[Capability];

    /// Initialize the agent (start PTY, etc.)
    async fn initialize(&mut self) -> Result<(), AdapterError>;

    /// Shutdown the agent
    async fn shutdown(&mut self) -> Result<(), AdapterError>;

    /// Execute a task (returns task ID for tracking)
    async fn execute_task(&mut self, request: TaskRequest) -> Result<TaskResult, AdapterError>;

    /// Cancel a running task
    async fn cancel_task(&mut self, task_id: Uuid) -> Result<(), AdapterError>;

    /// Get current status
    fn status(&self) -> AgentExecutionStatus;

    /// Receive context from other agents
    async fn receive_context(&mut self, context: SharedContext) -> Result<(), AdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_request() {
        let request = TaskRequest::new("Translate this text");
        assert!(!request.task_id.is_nil());
        assert_eq!(request.payload.content, "Translate this text");
        assert!(request.context.is_none());
    }

    #[test]
    fn test_shared_context() {
        let mut context = SharedContext::new();
        context.add_entry("agent-1".into(), "First message".into());

        assert_eq!(context.conversation_history.len(), 1);
        assert_eq!(context.conversation_history[0].agent_id, "agent-1");
    }

    #[test]
    fn test_stream_chunk() {
        let chunk = StreamChunk::new("Hello");
        assert_eq!(chunk.text, "Hello");
        assert!(!chunk.is_final);

        let final_chunk = StreamChunk::final_chunk("Done");
        assert!(final_chunk.is_final);
    }
}
