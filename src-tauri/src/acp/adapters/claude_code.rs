//! Claude Code Agent Adapter

use async_trait::async_trait;
use regex::Regex;
use uuid::Uuid;

use crate::acp::adapter::*;
use crate::acp::agent::{AgentCard, Capability, Transport};
use crate::pty::PtyManager;

/// Claude Code input converter
pub struct ClaudeCodeInputConverter;

impl InputConverter for ClaudeCodeInputConverter {
    fn convert_input(&self, task: &TaskPayload) -> Result<String, AdapterError> {
        // Claude Code understands natural language directly
        // Just return the content as-is
        Ok(task.content.clone())
    }

    fn embed_context(&self, prompt: &str, context: &SharedContext) -> String {
        if context.conversation_history.is_empty() && context.shared_files.is_empty() {
            return prompt.to_string();
        }

        let mut full_prompt = String::new();

        // Add shared files
        if !context.shared_files.is_empty() {
            full_prompt.push_str("## Related Files\n\n");
            for file in &context.shared_files {
                full_prompt.push_str(&format!("- {}\n", file));
            }
            full_prompt.push('\n');
        }

        // Add conversation history
        if !context.conversation_history.is_empty() {
            full_prompt.push_str("## Previous Context\n\n");
            for entry in &context.conversation_history {
                full_prompt.push_str(&format!("> [{}] {}\n\n", entry.agent_id, entry.summary));
            }
        }

        full_prompt.push_str("---\n\n");
        full_prompt.push_str(prompt);

        full_prompt
    }
}

/// Claude Code output converter
pub struct ClaudeCodeOutputConverter {
    ansi_regex: Regex,
    completion_patterns: Vec<Regex>,
}

impl ClaudeCodeOutputConverter {
    pub fn new() -> Self {
        Self {
            // Remove ANSI escape sequences
            ansi_regex: Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap(),
            // Patterns indicating Claude Code is waiting for input
            completion_patterns: vec![
                Regex::new(r"> $").unwrap(),   // Standard prompt
                Regex::new(r"❯ $").unwrap(),   // Custom prompt
                Regex::new(r"› $").unwrap(),   // Another custom prompt
            ],
        }
    }
}

impl Default for ClaudeCodeOutputConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputConverter for ClaudeCodeOutputConverter {
    fn parse_output(&self, raw_output: &str) -> Result<Vec<ParsedOutput>, AdapterError> {
        // Remove ANSI escape sequences
        let clean_output = self.ansi_regex.replace_all(raw_output, "");

        // Simple parsing - treat everything as text for now
        // A more sophisticated implementation would detect code blocks, etc.
        let content = clean_output.trim().to_string();

        if content.is_empty() {
            Ok(vec![])
        } else {
            Ok(vec![ParsedOutput {
                content,
                content_type: OutputContentType::Text,
                metadata: None,
            }])
        }
    }

    fn to_stream_chunk(&self, parsed: &ParsedOutput) -> Option<StreamChunk> {
        if parsed.content.is_empty() {
            None
        } else {
            Some(StreamChunk::new(&parsed.content))
        }
    }

    fn is_prompt_complete(&self, output: &str) -> bool {
        // Check if Claude Code is waiting for input
        for pattern in &self.completion_patterns {
            if pattern.is_match(output) {
                return true;
            }
        }
        false
    }
}

/// Claude Code adapter
pub struct ClaudeCodeAdapter {
    card: AgentCard,
    pty: PtyManager,
    input_converter: ClaudeCodeInputConverter,
    output_converter: ClaudeCodeOutputConverter,
    status: AgentExecutionStatus,
    pending_context: Option<SharedContext>,
}

impl ClaudeCodeAdapter {
    /// Create a new Claude Code adapter
    pub fn new(instance_id: &str) -> Self {
        Self {
            card: AgentCard::claude_code(instance_id),
            pty: PtyManager::new(),
            input_converter: ClaudeCodeInputConverter,
            output_converter: ClaudeCodeOutputConverter::new(),
            status: AgentExecutionStatus::Idle,
            pending_context: None,
        }
    }

    /// Create with custom skills
    pub fn with_capabilities(instance_id: &str, skills: Vec<Capability>) -> Self {
        let mut adapter = Self::new(instance_id);
        adapter.card.skills = Some(skills);
        adapter
    }

    /// Read available output from PTY
    fn read_pty_output(&self) -> Result<String, AdapterError> {
        let mut buffer = [0u8; 8192];
        let pty = &self.pty;

        match pty.read_output(&mut buffer) {
            Ok(n) if n > 0 => {
                String::from_utf8(buffer[..n].to_vec())
                    .map_err(|e| AdapterError::CommunicationFailed(format!("UTF-8 decode error: {}", e)))
            }
            Ok(_) => Ok(String::new()),
            Err(e) => Err(AdapterError::CommunicationFailed(e.to_string())),
        }
    }
}

#[async_trait]
impl AgentAdapter for ClaudeCodeAdapter {
    fn agent_card(&self) -> &AgentCard {
        &self.card
    }

    fn capabilities(&self) -> Vec<&Capability> {
        self.card.skills.as_ref()
            .map(|s| s.iter().collect())
            .unwrap_or_default()
    }

    async fn initialize(&mut self) -> Result<(), AdapterError> {
        if self.pty.is_running() {
            return Ok(());
        }

        self.pty
            .spawn_claude_code()
            .map_err(|e| AdapterError::InitializationFailed(e.to_string()))?;

        // Wait for Claude Code to initialize
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        self.status = AgentExecutionStatus::Idle;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), AdapterError> {
        self.status = AgentExecutionStatus::Shutdown;
        // PTY will be cleaned up when dropped
        Ok(())
    }

    async fn execute_task(
        &mut self,
        request: TaskRequest,
    ) -> Result<TaskResult, AdapterError> {
        if !self.pty.is_running() {
            return Err(AdapterError::NotReady);
        }

        // Prepare the prompt
        let prompt = if let Some(ref context) = request.context {
            let base_prompt = self.input_converter.convert_input(&request.payload)?;
            self.input_converter.embed_context(&base_prompt, context)
        } else if let Some(ref context) = self.pending_context {
            let base_prompt = self.input_converter.convert_input(&request.payload)?;
            let result = self.input_converter.embed_context(&base_prompt, context);
            self.pending_context = None;
            result
        } else {
            self.input_converter.convert_input(&request.payload)?
        };

        // Send to PTY
        self.pty
            .send_message(&prompt)
            .map_err(|e| AdapterError::CommunicationFailed(e.to_string()))?;

        self.status = AgentExecutionStatus::Busy {
            task_id: request.task_id.to_string(),
        };

        // In a real implementation, we would read the PTY output here
        // For now, return a simple result
        Ok(TaskResult::new("Task submitted to Claude Code"))
    }

    async fn cancel_task(&mut self, _task_id: Uuid) -> Result<(), AdapterError> {
        // Send Ctrl+C to PTY
        self.pty
            .send_message("\x03")
            .map_err(|e| AdapterError::CommunicationFailed(e.to_string()))?;

        self.status = AgentExecutionStatus::Idle;
        Ok(())
    }

    fn status(&self) -> AgentExecutionStatus {
        self.status.clone()
    }

    async fn receive_context(&mut self, context: SharedContext) -> Result<(), AdapterError> {
        self.pending_context = Some(context);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_converter() {
        let converter = ClaudeCodeInputConverter;
        let task = TaskPayload::new("Hello, Claude!");
        let result = converter.convert_input(&task).unwrap();
        assert_eq!(result, "Hello, Claude!");
    }

    #[test]
    fn test_context_embedding() {
        let converter = ClaudeCodeInputConverter;
        let mut context = SharedContext::new();
        context.add_entry("agent-1".into(), "Previous work".into());
        context.shared_files.push("file.rs".into());

        let prompt = "Do something";
        let result = converter.embed_context(prompt, &context);

        assert!(result.contains("Related Files"));
        assert!(result.contains("file.rs"));
        assert!(result.contains("Previous Context"));
        assert!(result.contains("Previous work"));
        assert!(result.contains(prompt));
    }

    #[test]
    fn test_output_converter() {
        let converter = ClaudeCodeOutputConverter::new();

        // Test clean output
        let output = "Hello, this is Claude.";
        let parsed = converter.parse_output(output).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].content, "Hello, this is Claude.");

        // Test ANSI-stripped output
        let output_with_ansi = "\x1b[32mHello\x1b[0m";
        let parsed = converter.parse_output(output_with_ansi).unwrap();
        assert_eq!(parsed[0].content, "Hello");
    }

    #[test]
    fn test_adapter_creation() {
        let adapter = ClaudeCodeAdapter::new("test");
        assert_eq!(adapter.card.id, Some("claude-code@localhost/test".to_string()));
        assert!(adapter.card.skills.as_ref().map_or(false, |s| s.iter().any(|skill| skill.id == "translation")));
    }
}
