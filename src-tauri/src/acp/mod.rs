//! ACP (Agent Communication Protocol) Module
//!
//! A multi-agent communication protocol for AI applications.

pub mod adapter;
pub mod agent;
pub mod adapters;
pub mod message;
pub mod orchestrator;
pub mod parser;  // Output parser for status detection
pub mod poller;  // Status polling and event emission
pub mod registry;
pub mod transport;
pub mod tmux;  // ACP v2: tmux-based orchestrator

// Re-exports for convenience
pub use adapter::SharedContext;
pub use agent::{AgentCard, DiscoveryQuery, Transport};
pub use orchestrator::{AgentOrchestrator, OrchestratorStats, TaskState};
pub use parser::OutputParser;
pub use poller::{PollerConfig, StatusPoller, StatusChangedPayload, OutputReadyPayload};
pub use tmux::{TmuxOrchestrator, TmuxError, AgentType as TmuxAgentType, AgentStatus, PaneInfo};
