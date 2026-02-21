//! Pipeline Execution Module
//!
//! ACP v3 Pipeline Communication Pattern
//!
//! Flow:
//! Client → Agent#1: prompt
//! Agent#1 → Client: stage #1 done
//! Client → Agent#2: prompt (with context from #1)
//! Agent#2 → Client: stage #2 done
//! ...
//! Agent#N → Client: pipeline_end

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use super::message::{ACPMessageV3, AddressType, AgentAddress, MessageType, PipelineStage};
use super::agent::AgentCard;

// ============================================================================
// Pipeline State Types
// ============================================================================

/// Pipeline execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    /// Pipeline is defined but not started
    Pending,
    /// Pipeline is currently running
    Running,
    /// Pipeline completed successfully
    Completed,
    /// Pipeline failed at some stage
    Failed,
    /// Pipeline was cancelled
    Cancelled,
}

/// Individual stage status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

// ============================================================================
// Pipeline Definition
// ============================================================================

/// Pipeline definition (static configuration)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinition {
    /// Unique pipeline ID
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Pipeline stages
    pub stages: Vec<PipelineStage>,
    /// Default input data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_input: Option<serde_json::Value>,
    /// Whether to stop on first failure
    #[serde(default = "default_stop_on_failure")]
    pub stop_on_failure: bool,
}

fn default_stop_on_failure() -> bool {
    true
}

impl PipelineDefinition {
    /// Create a new pipeline definition
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            stages: Vec::new(),
            default_input: None,
            stop_on_failure: true,
        }
    }

    /// Add a stage to the pipeline
    pub fn add_stage(mut self, stage: PipelineStage) -> Self {
        self.stages.push(stage);
        self
    }

    /// Add multiple stages
    pub fn with_stages(mut self, stages: Vec<PipelineStage>) -> Self {
        self.stages = stages;
        self
    }

    /// Set default input
    pub fn with_default_input(mut self, input: serde_json::Value) -> Self {
        self.default_input = Some(input);
        self
    }

    /// Set stop on failure behavior
    pub fn with_stop_on_failure(mut self, stop: bool) -> Self {
        self.stop_on_failure = stop;
        self
    }

    /// Get total number of stages
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }
}

// ============================================================================
// Pipeline Execution State
// ============================================================================

/// Result of a single stage execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    /// Stage name
    pub stage_name: String,
    /// Stage index in pipeline
    pub stage_index: usize,
    /// Status
    pub status: StageStatus,
    /// Output data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Start timestamp
    pub start_time: DateTime<Utc>,
    /// End timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<DateTime<Utc>>,
}

impl StageResult {
    pub fn pending(stage_name: String, stage_index: usize) -> Self {
        Self {
            stage_name,
            stage_index,
            status: StageStatus::Pending,
            output: None,
            error: None,
            start_time: Utc::now(),
            end_time: None,
        }
    }

    pub fn running(stage_name: String, stage_index: usize) -> Self {
        Self {
            stage_name,
            stage_index,
            status: StageStatus::Running,
            output: None,
            error: None,
            start_time: Utc::now(),
            end_time: None,
        }
    }

    pub fn complete(mut self, output: serde_json::Value) -> Self {
        self.status = StageStatus::Completed;
        self.output = Some(output);
        self.end_time = Some(Utc::now());
        self
    }

    pub fn fail(mut self, error: String) -> Self {
        self.status = StageStatus::Failed;
        self.error = Some(error);
        self.end_time = Some(Utc::now());
        self
    }

    pub fn skip(mut self) -> Self {
        self.status = StageStatus::Skipped;
        self.end_time = Some(Utc::now());
        self
    }

    /// Get duration in milliseconds
    pub fn duration_ms(&self) -> Option<i64> {
        self.end_time.map(|end| {
            (end - self.start_time).num_milliseconds()
        })
    }
}

/// Pipeline execution state (runtime)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineExecution {
    /// Pipeline definition ID
    pub pipeline_id: String,
    /// Unique execution ID
    pub execution_id: String,
    /// Current status
    pub status: PipelineStatus,
    /// Current stage index (0-based)
    pub current_stage: usize,
    /// Results for each stage
    pub stage_results: Vec<StageResult>,
    /// Combined context from all completed stages
    pub context: HashMap<String, serde_json::Value>,
    /// Execution start time
    pub start_time: DateTime<Utc>,
    /// Execution end time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<DateTime<Utc>>,
    /// Error message if pipeline failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PipelineExecution {
    /// Create a new execution for a pipeline definition
    pub fn new(definition: &PipelineDefinition) -> Self {
        let stage_results = definition
            .stages
            .iter()
            .enumerate()
            .map(|(i, stage)| StageResult::pending(stage.name.clone(), i))
            .collect();

        Self {
            pipeline_id: definition.id.clone(),
            execution_id: Uuid::new_v4().to_string(),
            status: PipelineStatus::Pending,
            current_stage: 0,
            stage_results,
            context: HashMap::new(),
            start_time: Utc::now(),
            end_time: None,
            error: None,
        }
    }

    /// Start the pipeline
    pub fn start(&mut self) {
        self.status = PipelineStatus::Running;
        if !self.stage_results.is_empty() {
            self.stage_results[0].status = StageStatus::Running;
        }
    }

    /// Complete current stage and move to next
    pub fn complete_stage(&mut self, output: serde_json::Value) {
        if self.current_stage < self.stage_results.len() {
            // Complete current stage
            let stage_name = self.stage_results[self.current_stage].stage_name.clone();
            self.stage_results[self.current_stage] = StageResult::running(stage_name, self.current_stage)
                .complete(output.clone());

            // Store in context
            self.context.insert(
                self.stage_results[self.current_stage].stage_name.clone(),
                output,
            );

            // Move to next stage
            self.current_stage += 1;

            if self.current_stage < self.stage_results.len() {
                // Mark next stage as running
                let next_name = self.stage_results[self.current_stage].stage_name.clone();
                self.stage_results[self.current_stage] = StageResult::running(next_name, self.current_stage);
            } else {
                // Pipeline complete
                self.status = PipelineStatus::Completed;
                self.end_time = Some(Utc::now());
            }
        }
    }

    /// Fail current stage
    pub fn fail_stage(&mut self, error: String) {
        if self.current_stage < self.stage_results.len() {
            let stage_name = self.stage_results[self.current_stage].stage_name.clone();
            self.stage_results[self.current_stage] = StageResult::running(stage_name, self.current_stage)
                .fail(error.clone());
        }
        self.status = PipelineStatus::Failed;
        self.error = Some(error);
        self.end_time = Some(Utc::now());
    }

    /// Cancel the pipeline
    pub fn cancel(&mut self) {
        self.status = PipelineStatus::Cancelled;
        self.end_time = Some(Utc::now());

        // Mark remaining stages as skipped
        for i in self.current_stage..self.stage_results.len() {
            let stage_name = self.stage_results[i].stage_name.clone();
            self.stage_results[i] = StageResult::pending(stage_name, i).skip();
        }
    }

    /// Get progress as percentage (0-100)
    pub fn progress(&self) -> u8 {
        if self.stage_results.is_empty() {
            return 100;
        }
        let completed = self.stage_results.iter()
            .filter(|r| r.status == StageStatus::Completed || r.status == StageStatus::Skipped)
            .count();
        ((completed * 100) / self.stage_results.len()) as u8
    }

    /// Get total duration in milliseconds
    pub fn duration_ms(&self) -> Option<i64> {
        self.end_time.map(|end| {
            (end - self.start_time).num_milliseconds()
        })
    }
}

// ============================================================================
// Pipeline Executor
// ============================================================================

/// Pipeline execution error
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Pipeline not found: {0}")]
    NotFound(String),

    #[error("Execution not found: {0}")]
    ExecutionNotFound(String),

    #[error("No stages defined in pipeline")]
    NoStages,

    #[error("Stage failed: {0}")]
    StageFailed(String),

    #[error("Agent not available: {0}")]
    AgentNotAvailable(String),

    #[error("Invalid stage index: {0}")]
    InvalidStageIndex(usize),

    #[error("Pipeline already running: {0}")]
    AlreadyRunning(String),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Pipeline executor - manages pipeline definitions and executions
pub struct PipelineExecutor {
    /// Defined pipelines
    pipelines: Arc<Mutex<HashMap<String, PipelineDefinition>>>,
    /// Active executions
    executions: Arc<Mutex<HashMap<String, PipelineExecution>>>,
}

impl PipelineExecutor {
    /// Create a new pipeline executor
    pub fn new() -> Self {
        Self {
            pipelines: Arc::new(Mutex::new(HashMap::new())),
            executions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a pipeline definition
    pub fn register(&self, pipeline: PipelineDefinition) -> String {
        let id = pipeline.id.clone();
        let mut pipelines = self.pipelines.lock().unwrap();
        pipelines.insert(id.clone(), pipeline);
        id
    }

    /// Unregister a pipeline definition
    pub fn unregister(&self, pipeline_id: &str) -> Result<(), PipelineError> {
        let mut pipelines = self.pipelines.lock().unwrap();
        pipelines.remove(pipeline_id)
            .map(|_| ())
            .ok_or_else(|| PipelineError::NotFound(pipeline_id.to_string()))
    }

    /// Get a pipeline definition
    pub fn get_pipeline(&self, pipeline_id: &str) -> Option<PipelineDefinition> {
        let pipelines = self.pipelines.lock().unwrap();
        pipelines.get(pipeline_id).cloned()
    }

    /// List all pipeline definitions
    pub fn list_pipelines(&self) -> Vec<PipelineDefinition> {
        let pipelines = self.pipelines.lock().unwrap();
        pipelines.values().cloned().collect()
    }

    /// Start a new execution of a pipeline
    pub fn start_execution(&self, pipeline_id: &str) -> Result<PipelineExecution, PipelineError> {
        let pipelines = self.pipelines.lock().unwrap();
        let definition = pipelines.get(pipeline_id)
            .ok_or_else(|| PipelineError::NotFound(pipeline_id.to_string()))?;

        if definition.stages.is_empty() {
            return Err(PipelineError::NoStages);
        }

        let mut execution = PipelineExecution::new(definition);
        execution.start();

        let execution_id = execution.execution_id.clone();
        let mut executions = self.executions.lock().unwrap();
        executions.insert(execution_id, execution.clone());

        Ok(execution)
    }

    /// Get execution state
    pub fn get_execution(&self, execution_id: &str) -> Option<PipelineExecution> {
        let executions = self.executions.lock().unwrap();
        executions.get(execution_id).cloned()
    }

    /// Complete a stage in an execution
    pub fn complete_stage(
        &self,
        execution_id: &str,
        output: serde_json::Value,
    ) -> Result<PipelineExecution, PipelineError> {
        let mut executions = self.executions.lock().unwrap();
        let execution = executions.get_mut(execution_id)
            .ok_or_else(|| PipelineError::ExecutionNotFound(execution_id.to_string()))?;

        if execution.status != PipelineStatus::Running {
            return Err(PipelineError::AlreadyRunning(execution_id.to_string()));
        }

        execution.complete_stage(output);
        Ok(execution.clone())
    }

    /// Fail a stage in an execution
    pub fn fail_stage(
        &self,
        execution_id: &str,
        error: String,
    ) -> Result<PipelineExecution, PipelineError> {
        let mut executions = self.executions.lock().unwrap();
        let execution = executions.get_mut(execution_id)
            .ok_or_else(|| PipelineError::ExecutionNotFound(execution_id.to_string()))?;

        execution.fail_stage(error);
        Ok(execution.clone())
    }

    /// Cancel an execution
    pub fn cancel_execution(&self, execution_id: &str) -> Result<PipelineExecution, PipelineError> {
        let mut executions = self.executions.lock().unwrap();
        let execution = executions.get_mut(execution_id)
            .ok_or_else(|| PipelineError::ExecutionNotFound(execution_id.to_string()))?;

        execution.cancel();
        Ok(execution.clone())
    }

    /// Get all active executions
    pub fn get_active_executions(&self) -> Vec<PipelineExecution> {
        let executions = self.executions.lock().unwrap();
        executions.values()
            .filter(|e| e.status == PipelineStatus::Running)
            .cloned()
            .collect()
    }

    /// Clean up completed/failed executions older than specified seconds
    pub fn cleanup_stale(&self, max_age_seconds: i64) -> Vec<String> {
        let mut removed = Vec::new();
        let mut executions = self.executions.lock().unwrap();

        let now = Utc::now();
        let ids_to_remove: Vec<String> = executions.iter()
            .filter(|(_, e)| {
                e.status != PipelineStatus::Running && e.status != PipelineStatus::Pending
            })
            .filter(|(_, e)| {
                e.end_time.map_or(false, |end| {
                    (now - end).num_seconds() > max_age_seconds
                })
            })
            .map(|(id, _)| id.clone())
            .collect();

        for id in ids_to_remove {
            executions.remove(&id);
            removed.push(id);
        }

        removed
    }
}

impl Default for PipelineExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Message Creation Helpers
// ============================================================================

impl PipelineStage {
    /// Create a prompt message for this stage
    pub fn create_prompt(
        &self,
        from: &AgentAddress,
        context: &HashMap<String, serde_json::Value>,
        input: Option<&serde_json::Value>,
    ) -> ACPMessageV3 {
        let content = if let Some(template) = &self.prompt_template {
            // Simple template substitution
            let mut result = template.clone();
            for (key, value) in context {
                let placeholder = format!("{{{{{}}}}}", key);
                if let Ok(json_str) = serde_json::to_string(value) {
                    result = result.replace(&placeholder, &json_str);
                }
            }
            if let Some(input_val) = input {
                result = result.replace("{{input}}", &serde_json::to_string(input_val).unwrap_or_default());
            }
            result
        } else {
            // Default prompt with context
            let context_str = serde_json::to_string_pretty(&context).unwrap_or_default();
            format!("Context:\n{}\n\nPlease process this stage: {}", context_str, self.name)
        };

        ACPMessageV3::prompt(
            from.to_address_string(),
            self.agent.to_address_string(),
            content,
        )
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_definition() {
        let pipeline = PipelineDefinition::new("test-pipeline")
            .add_stage(PipelineStage::new("stage1", AgentAddress::new("agent1")))
            .add_stage(PipelineStage::new("stage2", AgentAddress::new("agent2")));

        assert_eq!(pipeline.name, "test-pipeline");
        assert_eq!(pipeline.stage_count(), 2);
    }

    #[test]
    fn test_pipeline_execution() {
        let pipeline = PipelineDefinition::new("test")
            .add_stage(PipelineStage::new("s1", AgentAddress::new("a1")))
            .add_stage(PipelineStage::new("s2", AgentAddress::new("a2")));

        let mut execution = PipelineExecution::new(&pipeline);
        assert_eq!(execution.status, PipelineStatus::Pending);

        execution.start();
        assert_eq!(execution.status, PipelineStatus::Running);
        assert_eq!(execution.current_stage, 0);

        // Complete first stage
        execution.complete_stage(serde_json::json!({"result": "stage1"}));
        assert_eq!(execution.current_stage, 1);
        assert_eq!(execution.stage_results[0].status, StageStatus::Completed);

        // Complete second stage
        execution.complete_stage(serde_json::json!({"result": "stage2"}));
        assert_eq!(execution.status, PipelineStatus::Completed);
        assert_eq!(execution.progress(), 100);
    }

    #[test]
    fn test_pipeline_executor() {
        let executor = PipelineExecutor::new();

        let pipeline = PipelineDefinition::new("test")
            .add_stage(PipelineStage::new("s1", AgentAddress::new("a1")));

        let pipeline_id = executor.register(pipeline);
        assert!(!pipeline_id.is_empty());

        let execution = executor.start_execution(&pipeline_id).unwrap();
        assert_eq!(execution.status, PipelineStatus::Running);

        let updated = executor.complete_stage(&execution.execution_id, serde_json::json!({"ok": true})).unwrap();
        assert_eq!(updated.status, PipelineStatus::Completed);
    }

    #[test]
    fn test_stage_result_duration() {
        let start = Utc::now();
        let result = StageResult::running("test".to_string(), 0);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let completed = result.complete(serde_json::json!({}));

        assert!(completed.duration_ms().unwrap() >= 10);
    }

    #[test]
    fn test_pipeline_cancellation() {
        let pipeline = PipelineDefinition::new("test")
            .add_stage(PipelineStage::new("s1", AgentAddress::new("a1")))
            .add_stage(PipelineStage::new("s2", AgentAddress::new("a2")));

        let mut execution = PipelineExecution::new(&pipeline);
        execution.start();

        // Cancel before completing
        execution.cancel();
        assert_eq!(execution.status, PipelineStatus::Cancelled);
        assert_eq!(execution.stage_results[1].status, StageStatus::Skipped);
    }
}
