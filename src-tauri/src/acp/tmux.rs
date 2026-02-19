//! tmuxベースのマルチエージェントオーケストレーター
//! ACP v2: 技術リスク検証用PoC

use std::collections::HashMap;
use std::process::Command;
use thiserror::Error;

use super::parser::OutputParser;

/// tmux操作のエラー
#[derive(Debug, Error)]
pub enum TmuxError {
    #[error("Command failed: {0}")]
    CommandFailed(String),
    #[error("Session creation failed: {0}")]
    SessionCreationFailed(String),
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Invalid pane ID")]
    InvalidPaneId,
}

/// エージェントの種類
#[derive(Debug, Clone, PartialEq)]
pub enum AgentType {
    ClaudeCode,
    Codex,
    GenericShell,
}

/// エージェントの状態
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    /// 起動中
    Initializing,
    /// 処理中
    Processing,
    /// アイドル（次の指示待ち）
    Idle,
    /// 質問待ち
    WaitingForInput { question: String },
    /// エラー
    Error { message: String },
    /// 不明
    Unknown,
}

/// ペイン情報
#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub pane_id: String,
    pub agent_id: String,
    pub agent_type: AgentType,
    pub capabilities: Vec<String>,
    pub status: AgentStatus,
}

/// tmuxベースのオーケストレーター
pub struct TmuxOrchestrator {
    session_name: String,
    panes: HashMap<String, PaneInfo>,
    parser: OutputParser,
}

impl TmuxOrchestrator {
    pub fn new(session_name: &str) -> Self {
        Self {
            session_name: session_name.to_string(),
            panes: HashMap::new(),
            parser: OutputParser::new(),
        }
    }

    /// tmuxセッションを作成
    pub fn create_session(&mut self) -> Result<(), TmuxError> {
        // 既存のセッションがあれば削除
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.session_name])
            .output();

        // 新しいセッションを作成
        let output = Command::new("tmux")
            .args(["new-session", "-d", "-s", &self.session_name, "-x", "200", "-y", "50"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(TmuxError::SessionCreationFailed(stderr));
        }

        // 最初のペインを登録
        let pane_id = self.get_first_pane_id()?;
        self.panes.insert("main".to_string(), PaneInfo {
            pane_id,
            agent_id: "main".to_string(),
            agent_type: AgentType::GenericShell,
            capabilities: vec![],
            status: AgentStatus::Idle,
        });

        Ok(())
    }

    /// 新しいペインを作成してエージェントを起動
    pub fn spawn_agent(
        &mut self,
        agent_id: &str,
        agent_type: AgentType,
        capabilities: Vec<String>,
    ) -> Result<String, TmuxError> {
        // セッション名だけで参照（最初のウィンドウが使われる）
        let output = Command::new("tmux")
            .args([
                "split-window",
                "-t", &self.session_name,
                "-P",  // ペインIDを出力
                "-F", "#{pane_id}",
            ])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TmuxError::CommandFailed(stderr.to_string()));
        }

        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        eprintln!("[TmuxOrchestrator] Created pane: {} for agent: {}", pane_id, agent_id);

        // ペインレイアウトを調整
        let _ = Command::new("tmux")
            .args(["select-layout", "-t", &self.session_name, "tiled"])
            .output();

        // エージェントを起動
        let cmd = match agent_type {
            AgentType::ClaudeCode => "claude code",
            AgentType::Codex => "codex",
            AgentType::GenericShell => "bash",
        };

        self.send_keys(&pane_id, cmd)?;

        // ペイン情報を登録
        self.panes.insert(agent_id.to_string(), PaneInfo {
            pane_id: pane_id.clone(),
            agent_id: agent_id.to_string(),
            agent_type,
            capabilities,
            status: AgentStatus::Initializing,
        });

        Ok(pane_id)
    }

    /// ペインにキー入力を送信（リテラルモード使用）
    pub fn send_keys(&self, pane_id: &str, text: &str) -> Result<(), TmuxError> {
        // ペインIDの形式チェック
        if !pane_id.starts_with('%') {
            // ペインIDが正しくない場合は、セッション名と組み合わせてみる
        }

        // -l フラグでリテラルモード（特殊文字をエスケープ）
        Command::new("tmux")
            .args(["send-keys", "-t", pane_id, "-l", text])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        // Enterを送信
        Command::new("tmux")
            .args(["send-keys", "-t", pane_id, "Enter"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        Ok(())
    }

    /// ペインの画面内容をキャプチャ（ANSIエスケープシーケンス付き）
    pub fn capture_pane(&self, pane_id: &str) -> Result<String, TmuxError> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", pane_id, "-p", "-e"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// ペインの画面内容をキャプチャ（ANSIエスケープシーケンス除去）
    pub fn capture_pane_plain(&self, pane_id: &str) -> Result<String, TmuxError> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", pane_id, "-p"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// エージェントの状態を検出（OutputParserを使用）
    pub fn detect_status(&self, pane_id: &str) -> AgentStatus {
        if let Ok(content) = self.capture_pane_plain(pane_id) {
            self.parser.parse(&content)
        } else {
            AgentStatus::Unknown
        }
    }

    /// エージェントの状態を検出（生のコンテンツから）
    pub fn detect_status_from_content(&self, content: &str) -> AgentStatus {
        self.parser.parse(content)
    }

    /// 意味のあるコンテンツを抽出
    pub fn extract_meaningful_content(&self, content: &str) -> String {
        self.parser.extract_meaningful_content(content)
    }

    /// エージェント一覧を取得
    pub fn list_agents(&self) -> Vec<&PaneInfo> {
        self.panes.values().collect()
    }

    /// 特定の能力を持つエージェントを検索
    pub fn discover_by_capability(&self, capability: &str) -> Vec<&PaneInfo> {
        self.panes.values()
            .filter(|p| p.capabilities.contains(&capability.to_string()))
            .collect()
    }

    /// エージェントを終了
    pub fn kill_agent(&mut self, agent_id: &str) -> Result<(), TmuxError> {
        if let Some(pane) = self.panes.remove(agent_id) {
            Command::new("tmux")
                .args(["kill-pane", "-t", &pane.pane_id])
                .output()
                .ok();
        }
        Ok(())
    }

    /// セッションを終了
    pub fn destroy_session(&mut self) -> Result<(), TmuxError> {
        Command::new("tmux")
            .args(["kill-session", "-t", &self.session_name])
            .output()
            .ok();
        self.panes.clear();
        Ok(())
    }

    /// 最初のペインIDを取得
    fn get_first_pane_id(&self) -> Result<String, TmuxError> {
        let output = Command::new("tmux")
            .args(["list-panes", "-t", &self.session_name, "-F", "#{pane_id}"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        Ok(String::from_utf8_lossy(&output.stdout).lines().next().unwrap_or("").to_string())
    }

    /// エージェントのペインIDを取得
    pub fn get_pane_id(&self, agent_id: &str) -> Option<&str> {
        self.panes.get(agent_id).map(|p| p.pane_id.as_str())
    }
}

impl Drop for TmuxOrchestrator {
    fn drop(&mut self) {
        // セッションを自動的に削除
        let _ = self.destroy_session();
    }
}

// ============================================================================
// テスト用関数
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        let mut orch = TmuxOrchestrator::new("test-revoice");
        assert!(orch.create_session().is_ok());
        // Dropで自動的にセッションが削除される
    }

    #[test]
    fn test_capture_pane() {
        let mut orch = TmuxOrchestrator::new("test-revoice-capture");
        assert!(orch.create_session().is_ok());

        // コマンドを実行
        let pane_id = orch.get_pane_id("main").unwrap();
        orch.send_keys(pane_id, "echo 'Hello, tmux!'").unwrap();

        // 少し待機
        std::thread::sleep(std::time::Duration::from_millis(500));

        // キャプチャ
        let content = orch.capture_pane_plain(pane_id).unwrap();
        assert!(content.contains("Hello, tmux!"));
    }
}
