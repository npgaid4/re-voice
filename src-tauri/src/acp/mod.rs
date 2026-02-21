//! ACP (Agent Communication Protocol) Module
//!
//! A multi-agent communication protocol for AI applications.
//! A2A Protocol Compliant: https://github.com/google/A2A
//!
//! ## アーキテクチャ（v3 - CLIベース）
//!
//! tmuxベースからCLIベース（--print --output-format stream-json）に移行。
//! - `executor`: Claude Code子プロセス管理
//! - `stream_parser`: JSONイベントパーサー
//! - `state_machine`: 状態マシン
//! - `permission`: 権限管理

pub mod adapter;
pub mod agent;
pub mod adapters;
pub mod ask;  // ACP v3: Ask Tool handler
pub mod executor;  // CLI-based Claude Code executor
pub mod message;
pub mod orchestrator;
pub mod permission;  // Permission management
pub mod pipeline;  // ACP v3: Pipeline execution
pub mod registry;
pub mod runner;  // ACP v3: Pipeline runner
pub mod state_machine;  // State machine for agent states
pub mod stream_parser;  // Stream JSON parser
pub mod subtitle_parser;  // VTT subtitle parser
pub mod transport;

// Legacy modules (kept for backward compatibility during migration)
pub mod parser;  // Output parser for status detection (legacy)
pub mod poller;  // Status polling and event emission (legacy)
pub mod tmux;  // tmux-based orchestrator (legacy)

// Re-exports for convenience
pub use adapter::SharedContext;
pub use agent::{
    A2A_PROTOCOL_VERSION, AgentCapabilities, AgentCard, Authentication, DiscoveryQuery,
    JSONSchema, Provider, Skill, Transport,
};
// Legacy alias
pub use agent::Skill as Capability;
pub use executor::{ClaudeCodeExecutor, ExecutorError, ExecutorEvent, ExecutorOptions};
pub use message::{
    ACP_VERSION, ACPEnvelope, ACPMessage, ACPMessageV3, Address, AddressType,
    AgentAddress, CapabilityFilter, EnvelopeMetadata, MessageMetadata, MessagePayload,
    MessageType, PipelineStage, Priority,
};
pub use orchestrator::{AgentOrchestrator, OrchestratorStats, TaskState};
pub use parser::OutputParser;
pub use permission::{PermissionDecision, PermissionManager, PermissionPolicy, PermissionRequest};
pub use pipeline::{
    PipelineDefinition, PipelineError, PipelineExecution, PipelineExecutor, PipelineStatus,
    StageResult, StageStatus,
};
pub use poller::{PollerConfig, StatusPoller, StatusChangedPayload, OutputReadyPayload, QuestionPayload};
pub use runner::{PipelineRunner, RunnerError, ExecutionContext, ProgressPayload};
pub use state_machine::{AgentState, StateEvent, StateMachine};
pub use stream_parser::{StreamParser, StreamEvent, ParsedEvent, ParseError};
pub use subtitle_parser::{VttParser, SubtitleSegment, ParseError as SubtitleParseError};
pub use tmux::{TmuxOrchestrator, TmuxError, AgentType as TmuxAgentType, AgentStatus, PaneInfo};
pub use ask::{AskToolHandler, AskType, AskOption, AskResult, ParsedQuestion, HumanAnswer, AutoAnswerPolicy};
