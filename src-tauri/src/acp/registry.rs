//! Agent Registry - manages registered agents

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use super::agent::{AgentCard, DiscoveryQuery};

/// Agent status in the registry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    /// Agent is online and available
    Online,
    /// Agent is busy processing
    Busy,
    /// Agent is offline
    Offline,
    /// Agent has errored
    Error,
}

/// Registered agent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredAgent {
    /// Agent card
    pub card: AgentCard,
    /// Current status
    pub status: AgentStatus,
    /// Last heartbeat timestamp
    pub last_heartbeat: DateTime<Utc>,
    /// Registration timestamp
    pub registered_at: DateTime<Utc>,
}

impl RegisteredAgent {
    pub fn new(card: AgentCard) -> Self {
        let now = Utc::now();
        Self {
            card,
            status: AgentStatus::Online,
            last_heartbeat: now,
            registered_at: now,
        }
    }

    /// Update heartbeat
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Utc::now();
        if self.status == AgentStatus::Offline {
            self.status = AgentStatus::Online;
        }
    }

    /// Set status
    pub fn set_status(&mut self, status: AgentStatus) {
        self.status = status;
    }

    /// Check if agent is available (online or busy)
    pub fn is_available(&self) -> bool {
        matches!(self.status, AgentStatus::Online | AgentStatus::Busy)
    }

    /// Check if heartbeat is stale (older than threshold in seconds)
    pub fn is_stale(&self, threshold_seconds: i64) -> bool {
        let elapsed = (Utc::now() - self.last_heartbeat).num_seconds();
        elapsed > threshold_seconds
    }
}

/// Agent Registry
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<String, RegisteredAgent>>>,
    /// Heartbeat timeout in seconds
    heartbeat_timeout: i64,
}

impl AgentRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            heartbeat_timeout: 3600, // Default: 1 hour (no automatic heartbeat yet)
        }
    }

    /// Create a new registry with custom heartbeat timeout
    pub fn with_heartbeat_timeout(timeout_seconds: i64) -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            heartbeat_timeout: timeout_seconds,
        }
    }

    /// Register a new agent
    pub fn register(&self, card: AgentCard) -> Result<(), String> {
        let id = card.id.clone();
        let mut agents = self.agents.write();

        if agents.contains_key(&id) {
            return Err(format!("Agent {} is already registered", id));
        }

        agents.insert(id, RegisteredAgent::new(card));
        Ok(())
    }

    /// Unregister an agent
    pub fn unregister(&self, agent_id: &str) -> Result<(), String> {
        let mut agents = self.agents.write();

        if agents.remove(agent_id).is_none() {
            return Err(format!("Agent {} not found", agent_id));
        }

        Ok(())
    }

    /// Update agent heartbeat
    pub fn heartbeat(&self, agent_id: &str) -> Result<(), String> {
        let mut agents = self.agents.write();

        if let Some(agent) = agents.get_mut(agent_id) {
            agent.heartbeat();
            Ok(())
        } else {
            Err(format!("Agent {} not found", agent_id))
        }
    }

    /// Set agent status
    pub fn set_status(&self, agent_id: &str, status: AgentStatus) -> Result<(), String> {
        let mut agents = self.agents.write();

        if let Some(agent) = agents.get_mut(agent_id) {
            agent.set_status(status);
            Ok(())
        } else {
            Err(format!("Agent {} not found", agent_id))
        }
    }

    /// Get agent card by ID
    pub fn get(&self, agent_id: &str) -> Option<AgentCard> {
        let agents = self.agents.read();
        agents.get(agent_id).map(|a| a.card.clone())
    }

    /// Get registered agent info by ID
    pub fn get_registered(&self, agent_id: &str) -> Option<RegisteredAgent> {
        let agents = self.agents.read();
        agents.get(agent_id).cloned()
    }

    /// Discover agents matching a query
    pub fn discover(&self, query: &DiscoveryQuery) -> Vec<AgentCard> {
        let agents = self.agents.read();

        agents
            .values()
            .filter(|agent| {
                // Only return available agents
                agent.is_available() && !agent.is_stale(self.heartbeat_timeout)
            })
            .filter(|agent| query.matches(&agent.card))
            .map(|agent| agent.card.clone())
            .collect()
    }

    /// List all registered agents
    pub fn list_all(&self) -> Vec<RegisteredAgent> {
        let agents = self.agents.read();
        agents.values().cloned().collect()
    }

    /// List all available agents (online, not stale)
    pub fn list_available(&self) -> Vec<AgentCard> {
        let agents = self.agents.read();

        agents
            .values()
            .filter(|agent| agent.is_available() && !agent.is_stale(self.heartbeat_timeout))
            .map(|agent| agent.card.clone())
            .collect()
    }

    /// Clean up stale agents (mark as offline)
    pub fn cleanup_stale(&self) -> Vec<String> {
        let mut agents = self.agents.write();
        let mut cleaned = Vec::new();

        for (id, agent) in agents.iter_mut() {
            if agent.is_stale(self.heartbeat_timeout) && agent.status != AgentStatus::Offline {
                agent.status = AgentStatus::Offline;
                cleaned.push(id.clone());
            }
        }

        cleaned
    }

    /// Get count of registered agents
    pub fn count(&self) -> usize {
        self.agents.read().len()
    }

    /// Get count of available agents
    pub fn available_count(&self) -> usize {
        let agents = self.agents.read();
        agents
            .values()
            .filter(|a| a.is_available() && !a.is_stale(self.heartbeat_timeout))
            .count()
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
    fn test_register_agent() {
        let registry = AgentRegistry::new();
        let card = AgentCard::claude_code("test");

        assert!(registry.register(card).is_ok());
        assert_eq!(registry.count(), 1);

        // Duplicate registration should fail
        let card2 = AgentCard::claude_code("test");
        assert!(registry.register(card2).is_err());
    }

    #[test]
    fn test_discover_agents() {
        let registry = AgentRegistry::new();

        // Register multiple agents
        registry.register(AgentCard::claude_code("main")).unwrap();
        registry.register(AgentCard::claude_code("worker")).unwrap();
        registry.register(AgentCard::codex("helper")).unwrap();

        // Query for translation capability
        let query = DiscoveryQuery::new().with_capabilities(vec!["translation".into()]);
        let results = registry.discover(&query);
        assert_eq!(results.len(), 2); // Both Claude Code instances

        // Query for debugging capability
        let query = DiscoveryQuery::new().with_capabilities(vec!["debugging".into()]);
        let results = registry.discover(&query);
        assert_eq!(results.len(), 1); // Only Codex
    }

    #[test]
    fn test_heartbeat_and_stale() {
        let registry = AgentRegistry::with_heartbeat_timeout(1); // 1 second timeout
        let card = AgentCard::claude_code("test");

        registry.register(card).unwrap();

        // Initially available
        let available = registry.list_available();
        assert_eq!(available.len(), 1);

        // Wait for timeout
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Should be stale now
        let cleaned = registry.cleanup_stale();
        assert_eq!(cleaned.len(), 1);

        // No longer available
        let available = registry.list_available();
        assert_eq!(available.len(), 0);
    }
}
