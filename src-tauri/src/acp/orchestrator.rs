//! Agent Orchestrator - manages multiple agents and routes messages

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::adapter::{AdapterError, SharedContext, TaskRequest, TaskResult};
use super::agent::{AgentCard, DiscoveryQuery};
use super::registry::AgentRegistry;

/// Orchestrator error types
#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Agent busy: {0}")]
    AgentBusy(String),

    #[error("Adapter error: {0}")]
    AdapterError(#[from] AdapterError),

    #[error("Registry error: {0}")]
    RegistryError(String),

    #[error("No agents available for capability: {0}")]
    NoAgentsAvailable(String),

    #[error("Message routing failed: {0}")]
    RoutingFailed(String),

    #[error("Task failed: {0}")]
    TaskFailed(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),
}

impl From<String> for OrchestratorError {
    fn from(s: String) -> Self {
        OrchestratorError::RegistryError(s)
    }
}

/// Task execution state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    /// Task ID
    pub task_id: String,
    /// Original message ID
    pub message_id: String,
    /// Source agent
    pub from: String,
    /// Target agent
    pub to: String,
    /// Task status
    pub status: TaskExecutionStatus,
    /// Result (if completed)
    pub result: Option<TaskResult>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Task execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskExecutionStatus {
    /// Task is pending
    Pending,
    /// Task is running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
}

/// Orchestrator statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrchestratorStats {
    /// Total agents registered
    pub total_agents: usize,
    /// Available agents
    pub available_agents: usize,
    /// Tasks completed
    pub tasks_completed: usize,
    /// Tasks failed
    pub tasks_failed: usize,
    /// Tasks in progress
    pub tasks_in_progress: usize,
}

/// Agent Orchestrator
///
/// This is a simplified version that doesn't store adapters directly.
/// Agents are managed via the registry, and execution is handled externally.
pub struct AgentOrchestrator {
    /// Agent registry
    registry: AgentRegistry,
    /// Shared context for multi-agent tasks
    shared_context: Arc<RwLock<SharedContext>>,
    /// Pending tasks
    tasks: Arc<RwLock<HashMap<String, TaskState>>>,
    /// Statistics
    stats: Arc<RwLock<OrchestratorStats>>,
}

impl AgentOrchestrator {
    /// Create a new orchestrator
    pub fn new() -> Self {
        Self {
            registry: AgentRegistry::new(),
            shared_context: Arc::new(RwLock::new(SharedContext::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(OrchestratorStats::default())),
        }
    }

    /// Register an agent (just the card, not the adapter)
    pub fn register_agent_card(&self, card: AgentCard) -> Result<(), OrchestratorError> {
        self.registry.register(card)?;
        self.stats.write().total_agents = self.registry.count();
        Ok(())
    }

    /// Unregister an agent
    pub fn unregister_agent(&self, agent_id: &str) -> Result<(), OrchestratorError> {
        self.registry.unregister(agent_id)?;
        self.stats.write().total_agents = self.registry.count();
        Ok(())
    }

    /// Discover agents by query
    pub fn discover_agents(&self, query: &DiscoveryQuery) -> Vec<AgentCard> {
        self.registry.discover(query)
    }

    /// Get all registered agents
    pub fn list_agents(&self) -> Vec<AgentCard> {
        self.registry.list_available()
    }

    /// Get agent card by ID
    pub fn get_agent(&self, agent_id: &str) -> Option<AgentCard> {
        self.registry.get(agent_id)
    }

    /// Create a task request for later execution
    pub fn create_task(
        &self,
        from: &str,
        to: &str,
        content: &str,
        message_id: &str,
    ) -> Result<TaskRequest, OrchestratorError> {
        // Check if agent exists
        if self.get_agent(to).is_none() {
            return Err(OrchestratorError::AgentNotFound(to.to_string()));
        }

        let task_id = Uuid::new_v4();

        // Create task state
        {
            let mut tasks = self.tasks.write();
            tasks.insert(
                task_id.to_string(),
                TaskState {
                    task_id: task_id.to_string(),
                    message_id: message_id.to_string(),
                    from: from.to_string(),
                    to: to.to_string(),
                    status: TaskExecutionStatus::Pending,
                    result: None,
                    error: None,
                },
            );
            self.stats.write().tasks_in_progress += 1;
        }

        // Create task request with shared context
        let request = TaskRequest::new(content).with_context(self.shared_context.read().clone());

        Ok(request)
    }

    /// Mark a task as completed
    pub fn complete_task(&self, task_id: &str, result: TaskResult) {
        let mut tasks = self.tasks.write();
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = TaskExecutionStatus::Completed;
            task.result = Some(result.clone());

            // Update shared context
            self.shared_context
                .write()
                .add_entry(task.to.clone(), result.output);
        }

        let mut stats = self.stats.write();
        stats.tasks_completed += 1;
        stats.tasks_in_progress = stats.tasks_in_progress.saturating_sub(1);
    }

    /// Mark a task as failed
    pub fn fail_task(&self, task_id: &str, error: String) {
        let mut tasks = self.tasks.write();
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = TaskExecutionStatus::Failed;
            task.error = Some(error);
        }

        let mut stats = self.stats.write();
        stats.tasks_failed += 1;
        stats.tasks_in_progress = stats.tasks_in_progress.saturating_sub(1);
    }

    /// Get orchestrator statistics
    pub fn stats(&self) -> OrchestratorStats {
        self.stats.read().clone()
    }

    /// Get task state
    pub fn get_task(&self, task_id: &str) -> Option<TaskState> {
        self.tasks.read().get(task_id).cloned()
    }

    /// Update heartbeat for an agent
    pub fn heartbeat(&self, agent_id: &str) -> Result<(), OrchestratorError> {
        self.registry
            .heartbeat(agent_id)
            .map_err(OrchestratorError::from)
    }

    /// Clean up stale agents
    pub fn cleanup_stale(&self) -> Vec<String> {
        self.registry.cleanup_stale()
    }

    /// Get shared context
    pub fn get_shared_context(&self) -> SharedContext {
        self.shared_context.read().clone()
    }
}

impl Default for AgentOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_creation() {
        let orchestrator = AgentOrchestrator::new();
        let stats = orchestrator.stats();
        assert_eq!(stats.total_agents, 0);
    }

    #[test]
    fn test_register_agent_card() {
        let orchestrator = AgentOrchestrator::new();
        let card = AgentCard::claude_code("test");

        orchestrator.register_agent_card(card).unwrap();
        assert_eq!(orchestrator.stats().total_agents, 1);
    }
}
