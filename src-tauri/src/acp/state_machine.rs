//! Claude Code State Machine
//!
//! CLIモード（--print --output-format stream-json）用の状態管理。
//! tmuxベースから移行し、JSONイベントで状態を明示的に検出する。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Claude Code エージェントの状態
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentState {
    /// 初期化中
    Initializing,

    /// アイドル（次のタスク待ち）
    Idle,

    /// 処理中
    Processing {
        current_tool: Option<String>,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        started_at: DateTime<Utc>,
    },

    /// 権限要求中
    WaitingForPermission {
        tool_name: String,
        tool_input: Value,
        request_id: String,
    },

    /// ユーザー入力待ち（AskTool）
    WaitingForInput {
        question: String,
        options: Vec<String>,
    },

    /// エラー
    Error {
        message: String,
        recoverable: bool,
    },

    /// 完了
    Completed {
        output: String,
    },
}

impl AgentState {
    /// 初期状態（Initializing）を作成
    pub fn initializing() -> Self {
        Self::Initializing
    }

    /// アイドル状態に遷移
    pub fn idle() -> Self {
        Self::Idle
    }

    /// 処理中状態に遷移
    pub fn processing(current_tool: Option<String>) -> Self {
        Self::Processing {
            current_tool,
            started_at: Utc::now(),
        }
    }

    /// 権限要求状態に遷移
    pub fn waiting_for_permission(tool_name: String, tool_input: Value, request_id: String) -> Self {
        Self::WaitingForPermission {
            tool_name,
            tool_input,
            request_id,
        }
    }

    /// 入力待ち状態に遷移
    pub fn waiting_for_input(question: String, options: Vec<String>) -> Self {
        Self::WaitingForInput { question, options }
    }

    /// エラー状態に遷移
    pub fn error(message: String, recoverable: bool) -> Self {
        Self::Error { message, recoverable }
    }

    /// 完了状態に遷移
    pub fn completed(output: String) -> Self {
        Self::Completed { output }
    }

    /// 処理中かどうか
    pub fn is_processing(&self) -> bool {
        matches!(self, Self::Processing { .. })
    }

    /// アイドルまたは完了かどうか（タスク受付可能）
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Idle | Self::Completed { .. })
    }

    /// 待機状態かどうか（権限待ちまたは入力待ち）
    pub fn is_waiting(&self) -> bool {
        matches!(
            self,
            Self::WaitingForPermission { .. } | Self::WaitingForInput { .. }
        )
    }

    /// 状態名を取得（UI表示用）
    pub fn state_name(&self) -> &'static str {
        match self {
            Self::Initializing => "initializing",
            Self::Idle => "idle",
            Self::Processing { .. } => "processing",
            Self::WaitingForPermission { .. } => "waiting_for_permission",
            Self::WaitingForInput { .. } => "waiting_for_input",
            Self::Error { .. } => "error",
            Self::Completed { .. } => "completed",
        }
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::Initializing
    }
}

/// 状態遷移イベント
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum StateEvent {
    /// 初期化完了
    Initialized,

    /// タスク開始
    TaskStarted { prompt: String },

    /// ツール使用開始
    ToolUseStarted { tool_name: String },

    /// ツール使用完了
    ToolUseCompleted { tool_name: String, success: bool },

    /// 権限要求
    PermissionRequired {
        tool_name: String,
        tool_input: Value,
        request_id: String,
    },

    /// 権限許可
    PermissionGranted { request_id: String },

    /// 権限拒否
    PermissionDenied { request_id: String, reason: String },

    /// ユーザー入力要求
    InputRequired { question: String, options: Vec<String> },

    /// ユーザー入力受領
    InputReceived { answer: String },

    /// エラー発生
    ErrorOccurred { message: String, recoverable: bool },

    /// タスク完了
    TaskCompleted { output: String },
}

/// 状態マシン
#[derive(Debug, Clone)]
pub struct StateMachine {
    /// 現在の状態
    current_state: AgentState,
    /// 状態履歴（デバッグ用）
    history: Vec<(AgentState, DateTime<Utc>)>,
}

impl StateMachine {
    /// 新しい状態マシンを作成
    pub fn new() -> Self {
        let initial_state = AgentState::Initializing;
        Self {
            current_state: initial_state.clone(),
            history: vec![(initial_state, Utc::now())],
        }
    }

    /// 現在の状態を取得
    pub fn current_state(&self) -> &AgentState {
        &self.current_state
    }

    /// イベントを処理して状態を遷移
    pub fn transition(&mut self, event: StateEvent) -> AgentState {
        let new_state = self.apply_event(&event);

        // 履歴に追加（最大100件）
        self.history.push((new_state.clone(), Utc::now()));
        if self.history.len() > 100 {
            self.history.remove(0);
        }

        self.current_state = new_state;
        self.current_state.clone()
    }

    /// イベントを適用して新しい状態を計算
    fn apply_event(&self, event: &StateEvent) -> AgentState {
        match (&self.current_state, event) {
            // Initializingからの遷移
            (AgentState::Initializing, StateEvent::Initialized) => AgentState::idle(),

            // Idleからの遷移
            (AgentState::Idle, StateEvent::TaskStarted { .. }) => AgentState::processing(None),

            // Processingからの遷移
            (AgentState::Processing { .. }, StateEvent::ToolUseStarted { tool_name }) => {
                AgentState::processing(Some(tool_name.clone()))
            }
            (AgentState::Processing { .. }, StateEvent::ToolUseCompleted { .. }) => {
                // ツール完了後もProcessing継続（次のツールまたは完了待ち）
                self.current_state.clone()
            }
            (AgentState::Processing { .. }, StateEvent::PermissionRequired { tool_name, tool_input, request_id }) => {
                AgentState::waiting_for_permission(tool_name.clone(), tool_input.clone(), request_id.clone())
            }
            (AgentState::Processing { .. }, StateEvent::InputRequired { question, options }) => {
                AgentState::waiting_for_input(question.clone(), options.clone())
            }
            (AgentState::Processing { .. }, StateEvent::ErrorOccurred { message, recoverable }) => {
                AgentState::error(message.clone(), *recoverable)
            }
            (AgentState::Processing { .. }, StateEvent::TaskCompleted { output }) => {
                AgentState::completed(output.clone())
            }

            // WaitingForPermissionからの遷移
            (AgentState::WaitingForPermission { .. }, StateEvent::PermissionGranted { .. }) => {
                AgentState::processing(None)
            }
            (AgentState::WaitingForPermission { .. }, StateEvent::PermissionDenied { reason, .. }) => {
                AgentState::error(format!("Permission denied: {}", reason), true)
            }

            // WaitingForInputからの遷移
            (AgentState::WaitingForInput { .. }, StateEvent::InputReceived { .. }) => {
                AgentState::processing(None)
            }

            // Errorからの遷移
            (AgentState::Error { recoverable: true, .. }, StateEvent::TaskStarted { .. }) => {
                AgentState::processing(None)
            }
            (AgentState::Error { .. }, _) => {
                // 回復不可能なエラーの場合は状態維持
                self.current_state.clone()
            }

            // Completedからの遷移
            (AgentState::Completed { .. }, StateEvent::TaskStarted { .. }) => {
                AgentState::processing(None)
            }
            (AgentState::Completed { .. }, StateEvent::Initialized) => {
                AgentState::idle()
            }

            // その他の遷移は状態維持
            _ => self.current_state.clone(),
        }
    }

    /// 状態履歴を取得
    pub fn history(&self) -> &[(AgentState, DateTime<Utc>)] {
        &self.history
    }

    /// 強制的に状態を設定（復旧用）
    pub fn force_state(&mut self, state: AgentState) {
        self.history.push((state.clone(), Utc::now()));
        self.current_state = state;
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = StateMachine::new();
        assert!(matches!(sm.current_state(), AgentState::Initializing));
    }

    #[test]
    fn test_transition_to_idle() {
        let mut sm = StateMachine::new();
        let new_state = sm.transition(StateEvent::Initialized);
        assert!(matches!(new_state, AgentState::Idle));
    }

    #[test]
    fn test_transition_to_processing() {
        let mut sm = StateMachine::new();
        sm.transition(StateEvent::Initialized);

        let new_state = sm.transition(StateEvent::TaskStarted {
            prompt: "test".to_string(),
        });
        assert!(matches!(new_state, AgentState::Processing { .. }));
    }

    #[test]
    fn test_transition_to_completed() {
        let mut sm = StateMachine::new();
        sm.transition(StateEvent::Initialized);
        sm.transition(StateEvent::TaskStarted {
            prompt: "test".to_string(),
        });

        let new_state = sm.transition(StateEvent::TaskCompleted {
            output: "done".to_string(),
        });
        assert!(matches!(new_state, AgentState::Completed { .. }));
    }

    #[test]
    fn test_state_is_processing() {
        let state = AgentState::processing(Some("Read".to_string()));
        assert!(state.is_processing());
        assert!(!state.is_ready());
        assert!(!state.is_waiting());
    }

    #[test]
    fn test_state_is_ready() {
        let state = AgentState::idle();
        assert!(state.is_ready());

        let state = AgentState::completed("done".to_string());
        assert!(state.is_ready());
    }

    #[test]
    fn test_transition_to_waiting_for_permission() {
        let mut sm = StateMachine::new();
        sm.transition(StateEvent::Initialized);
        sm.transition(StateEvent::TaskStarted {
            prompt: "test".to_string(),
        });

        let new_state = sm.transition(StateEvent::PermissionRequired {
            tool_name: "Bash".to_string(),
            tool_input: serde_json::json!({"command": "rm -rf /"}),
            request_id: "req-1".to_string(),
        });

        match new_state {
            AgentState::WaitingForPermission { tool_name, .. } => {
                assert_eq!(tool_name, "Bash");
            }
            _ => panic!("Expected WaitingForPermission"),
        }
    }
}
