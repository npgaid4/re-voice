//! tmuxベースのマルチエージェントオーケストレーター
//! ACP v2: 技術リスク検証用PoC
//! ACP v3: Broadcast機能追加

use std::collections::HashMap;
use std::process::Command;
use thiserror::Error;

use super::parser::OutputParser;
use super::message::CapabilityFilter;

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

        // グローバル履歴サイズを設定
        let _ = Command::new("tmux")
            .args(["set-option", "-g", "history-limit", "50000"])
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

        // セッションの履歴サイズも設定
        let _ = Command::new("tmux")
            .args(["set-option", "-t", &self.session_name, "history-limit", "50000"])
            .output();

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
        // Claude Code は CLAUDECODE 環境変数をアンセットしないとネストセッションエラーになる
        let cmd = match agent_type {
            AgentType::ClaudeCode => "unset CLAUDECODE && claude code",
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
    /// テキストを一括送信してからEnterを送信
    pub fn send_keys(&self, pane_id: &str, text: &str) -> Result<(), TmuxError> {
        crate::log::info("send_keys", &format!("Sending text ({} bytes)", text.len()));

        // -l フラグでリテラルモード（特殊文字をエスケープ）
        // 複数行テキストも一括送信
        let output1 = Command::new("tmux")
            .args(["send-keys", "-t", pane_id, "-l", text])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        crate::log::info("send_keys", &format!("send-keys -l result: {:?}", output1.status));

        // 少し待機
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Enterを送信（最後に1回だけ）
        let output2 = Command::new("tmux")
            .args(["send-keys", "-t", pane_id, "Enter"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        crate::log::info("send_keys", &format!("send-keys Enter result: {:?}", output2.status));

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
        // -S - でスクロールバックの先頭から全行取得
        // -E - で履歴の最後まで取得
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", pane_id, "-p", "-S", "-", "-E", "-"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        let content = String::from_utf8_lossy(&output.stdout).to_string();
        eprintln!("[capture_pane_plain] Captured {} lines", content.lines().count());
        Ok(content)
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

    // =========================================================================
    // ACP v3: Broadcast Support
    // =========================================================================

    /// フィルターに基づいてエージェントを検索
    pub fn discover_agents(&self, filter: &CapabilityFilter) -> Vec<&PaneInfo> {
        self.panes.values()
            .filter(|pane| {
                // capabilities フィルター (AND条件)
                if let Some(ref required_caps) = filter.capabilities {
                    for cap in required_caps {
                        if !pane.capabilities.contains(cap) {
                            return false;
                        }
                    }
                }

                // tags フィルター (OR条件) - capabilitiesベースで簡易実装
                if let Some(ref required_tags) = filter.tags {
                    // タグはcapability名のプレフィックスで簡易判定
                    let has_tag = pane.capabilities.iter().any(|cap| {
                        required_tags.iter().any(|tag| cap.contains(tag) || tag.contains(cap))
                    });
                    if !has_tag {
                        return false;
                    }
                }

                // agent_type フィルター
                if let Some(ref agent_type) = filter.agent_type {
                    let matches = match pane.agent_type {
                        AgentType::ClaudeCode => agent_type == "claude-code" || agent_type == "claude_code",
                        AgentType::Codex => agent_type == "codex",
                        AgentType::GenericShell => agent_type == "shell" || agent_type == "generic",
                    };
                    if !matches {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    /// 複数のエージェントにメッセージをブロードキャスト
    /// 戻り値: (成功したエージェントIDのリスト, 失敗したエージェントIDとエラーメッセージ)
    pub fn broadcast_message(
        &self,
        content: &str,
        filter: Option<&CapabilityFilter>,
    ) -> (Vec<String>, Vec<(String, String)>) {
        let targets = if let Some(f) = filter {
            self.discover_agents(f)
        } else {
            // フィルターなしの場合は全エージェント
            self.panes.values().collect()
        };

        let mut success = Vec::new();
        let mut failures = Vec::new();

        for pane in targets {
            match self.send_keys(&pane.pane_id, content) {
                Ok(_) => success.push(pane.agent_id.clone()),
                Err(e) => failures.push((pane.agent_id.clone(), e.to_string())),
            }
        }

        crate::log::info("broadcast", &format!(
            "Broadcast complete: {} succeeded, {} failed",
            success.len(),
            failures.len()
        ));

        (success, failures)
    }

    /// アイドル状態のエージェントにのみブロードキャスト
    pub fn broadcast_to_idle(
        &self,
        content: &str,
        filter: Option<&CapabilityFilter>,
    ) -> (Vec<String>, Vec<(String, String)>) {
        let targets = if let Some(f) = filter {
            self.discover_agents(f)
        } else {
            self.panes.values().collect()
        };

        let idle_targets: Vec<&PaneInfo> = targets.into_iter()
            .filter(|p| p.status == AgentStatus::Idle)
            .collect();

        let mut success = Vec::new();
        let mut failures = Vec::new();

        for pane in idle_targets {
            match self.send_keys(&pane.pane_id, content) {
                Ok(_) => success.push(pane.agent_id.clone()),
                Err(e) => failures.push((pane.agent_id.clone(), e.to_string())),
            }
        }

        crate::log::info("broadcast_to_idle", &format!(
            "Broadcast to idle agents: {} succeeded, {} failed",
            success.len(),
            failures.len()
        ));

        (success, failures)
    }

    /// 特定の能力を持つエージェントにメッセージを送信（最初の1つのみ）
    pub fn send_to_capability(&self, capability: &str, content: &str) -> Result<String, TmuxError> {
        let agents = self.discover_by_capability(capability);
        if agents.is_empty() {
            return Err(TmuxError::AgentNotFound(format!(
                "No agent with capability: {}",
                capability
            )));
        }

        let pane = agents.into_iter().next().unwrap();
        self.send_keys(&pane.pane_id, content)?;
        Ok(pane.agent_id.clone())
    }

    /// エージェント数を取得
    pub fn agent_count(&self) -> usize {
        self.panes.len()
    }

    /// ステータス別のエージェント数を取得
    pub fn count_by_status(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for pane in self.panes.values() {
            let status_name = match &pane.status {
                AgentStatus::Initializing => "initializing",
                AgentStatus::Processing => "processing",
                AgentStatus::Idle => "idle",
                AgentStatus::WaitingForInput { .. } => "waiting_for_input",
                AgentStatus::Error { .. } => "error",
                AgentStatus::Unknown => "unknown",
            };
            *counts.entry(status_name.to_string()).or_insert(0) += 1;
        }
        counts
    }

    /// 全エージェントの状態を更新
    pub fn refresh_all_statuses(&mut self) {
        let pane_ids: Vec<(String, String)> = self.panes.iter()
            .map(|(id, pane)| (id.clone(), pane.pane_id.clone()))
            .collect();

        for (agent_id, pane_id) in pane_ids {
            let status = self.detect_status(&pane_id);
            if let Some(pane) = self.panes.get_mut(&agent_id) {
                pane.status = status;
            }
        }
    }

    /// 特定のエージェントの状態を更新
    pub fn refresh_status(&mut self, agent_id: &str) -> Result<AgentStatus, TmuxError> {
        let pane_id = self.get_pane_id(agent_id)
            .ok_or_else(|| TmuxError::AgentNotFound(agent_id.to_string()))?
            .to_string();

        let status = self.detect_status(&pane_id);
        if let Some(pane) = self.panes.get_mut(agent_id) {
            pane.status = status.clone();
        }

        Ok(status)
    }

    /// Askツールの選択肢を選択（矢印キーで移動してEnter）
    /// answer: 選択する選択肢の番号（"1"）またはテキスト（"Yes"）
    pub fn select_option(&self, pane_id: &str, answer: &str) -> Result<(), TmuxError> {
        // 現在の画面内容を取得
        let content = self.capture_pane_plain(pane_id)?;
        crate::log::info("select_option", &format!("Selecting option: {:?}", answer));

        // 選択肢一覧を解析
        let options = self.parse_selection_options(&content);
        crate::log::info("select_option", &format!("Found {} options", options.len()));
        for (idx, text, selected) in &options {
            let marker = if *selected { " [SELECTED]" } else { "" };
            crate::log::info("select_option", &format!("  Option {}: {}{}", idx, text, marker));
        }

        // 現在選択されている選択肢（❯がついているもの）を探す
        let current_selection = options.iter().find(|(_, _, selected)| *selected);
        let current_index = current_selection.map(|(idx, _, _)| *idx).unwrap_or(0);
        crate::log::info("select_option", &format!("Current selection index: {}", current_index));

        // 目的の選択肢を探す
        // まず番号として探す（"1" -> インデックス0）
        let target_by_number = answer.parse::<usize>().ok()
            .and_then(|num| {
                if num > 0 && num <= options.len() {
                    Some((num - 1, options[num - 1].1.clone()))
                } else {
                    None
                }
            });

        // テキストとして探す
        let target_by_text = options.iter().find(|(_, text, _)| {
            text.contains(answer) || answer.contains(text)
        }).map(|(idx, text, _)| (*idx, text.clone()));

        // 番号優先、なければテキスト
        let target = target_by_number.or(target_by_text);

        if let Some((target_index, target_text)) = target {
            crate::log::info("select_option", &format!("Target found at index {}: {:?}", target_index, target_text));

            // 移動が必要な回数を計算
            let moves_needed = if target_index > current_index {
                target_index - current_index
            } else if target_index < current_index {
                // 上に戻る必要がある場合（Esc でキャンセルしてから再選択する方が確実）
                crate::log::info("select_option", "Need to go up, pressing Esc first");
                self.send_key(pane_id, "Escape")?;
                std::thread::sleep(std::time::Duration::from_millis(300));
                // 最初の選択肢からtarget_index回下に移動
                target_index
            } else {
                0 // すでに選択されている
            };

            // 矢印キーを送信
            for i in 0..moves_needed {
                crate::log::info("select_option", &format!("Sending Down arrow ({}/{})", i + 1, moves_needed));
                self.send_key(pane_id, "Down")?;
                std::thread::sleep(std::time::Duration::from_millis(150));
            }

            // Enterで確定
            std::thread::sleep(std::time::Duration::from_millis(200));
            crate::log::info("select_option", "Sending Enter to confirm");
            self.send_key(pane_id, "Enter")?;

            Ok(())
        } else {
            // 選択肢が見つからない場合はテキストとして送信（Type something. 用）
            crate::log::info("select_option", &format!("Option not found in list, sending as text: {:?}", answer));
            self.send_keys(pane_id, answer)
        }
    }

    /// 選択肢を解析して (インデックス, テキスト, 選択中かどうか) のリストを返す
    fn parse_selection_options(&self, content: &str) -> Vec<(usize, String, bool)> {
        let mut options = Vec::new();
        let mut current_idx = 0;

        for line in content.lines() {
            let trimmed = line.trim();

            // "❯ 数字. " または "数字. " のパターンを探す
            // ❯ は3バイトのUTF-8文字なので strip_prefix を使う
            let (text, is_selected) = if let Some(rest) = trimmed.strip_prefix("❯") {
                (rest.trim().to_string(), true)
            } else {
                (trimmed.to_string(), false)
            };

            // 選択肢のパターンマッチ
            if let Some(num) = extract_option_number(&text) {
                let clean_text = clean_option_text(&text);
                // 数字が連続している場合のみ追加
                if num == (current_idx + 1) as u32 {
                    current_idx = num as usize;
                    options.push((current_idx - 1, clean_text, is_selected));
                } else if num == 1 && options.is_empty() {
                    // 最初の選択肢
                    current_idx = 1;
                    options.push((0, clean_text, is_selected));
                }
            }
        }

        options
    }

    /// 単一キーを送信（矢印キー、Escapeなど）
    pub fn send_key(&self, pane_id: &str, key: &str) -> Result<(), TmuxError> {
        crate::log::info("send_key", &format!("Sending key: {:?}", key));

        let output = Command::new("tmux")
            .args(["send-keys", "-t", pane_id, key])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            crate::log::error("send_key", &format!("Failed: {}", stderr));
            return Err(TmuxError::CommandFailed(stderr));
        }

        Ok(())
    }
}

impl Drop for TmuxOrchestrator {
    fn drop(&mut self) {
        // セッションを自動的に削除
        let _ = self.destroy_session();
    }
}

// ============================================================================
// ヘルパー関数
// ============================================================================

/// 選択肢テキストから先頭の記号を除去
fn clean_option_text(line: &str) -> String {
    line.trim_start_matches(|c: char| c == '❯' || c == '>' || c == '○' || c == '●' || c == '◉' || c == ' ')
        .trim()
        .to_string()
}

/// 選択肢から番号を抽出
fn extract_option_number(line: &str) -> Option<u32> {
    // 先頭の記号（❯, >, ○, ●, ◉ など）を除去
    let cleaned = line
        .trim_start_matches(|c: char| c == '❯' || c == '>' || c == '○' || c == '●' || c == '◉' || c == ' ')
        .trim();

    // "1. " または "1: " のパターン
    if let Some(first_char) = cleaned.chars().next() {
        if first_char.is_ascii_digit() {
            // 数字部分を抽出
            let num_str: String = cleaned.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(num) = num_str.parse::<u32>() {
                // 数字の後に ". " または ": " または "." があるか確認
                let rest = cleaned.trim_start_matches(|c: char| c.is_ascii_digit());
                if rest.starts_with(". ") || rest.starts_with(": ") || rest.starts_with(".") {
                    return Some(num);
                }
            }
        }
    }
    None
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
