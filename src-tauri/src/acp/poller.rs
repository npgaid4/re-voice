//! ステータスポーリングとイベント通知
//!
//! 定期的にエージェントの状態をチェックし、変化があった場合にイベントを発火する。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use super::parser::OutputParser;
use super::tmux::{AgentStatus, PaneInfo, TmuxOrchestrator};
use crate::log;

/// ポーリング設定
#[derive(Debug, Clone)]
pub struct PollerConfig {
    /// ポーリング間隔（ミリ秒）
    pub interval_ms: u64,
    /// 出力変化の最小サイズ（これ以下の変化は無視）
    pub min_output_change: usize,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            interval_ms: 200,  // 200ms間隔でポーリング（Processing状態の検出を改善）
            min_output_change: 10,
        }
    }
}

/// 状態変化イベントのペイロード
#[derive(Debug, Clone, Serialize)]
pub struct StatusChangedPayload {
    pub agent_id: String,
    pub old_status: String,
    pub new_status: String,
}

/// 出力準備完了イベントのペイロード
#[derive(Debug, Clone, Serialize)]
pub struct OutputReadyPayload {
    pub agent_id: String,
    pub content: String,
    pub content_length: usize,
}

/// 質問イベントのペイロード
#[derive(Debug, Clone, Serialize)]
pub struct QuestionPayload {
    pub agent_id: String,
    pub question: String,
    pub question_id: String,
    pub context: String,
}

/// エージェント状態のスナップショット
#[derive(Debug, Clone)]
struct AgentSnapshot {
    status: AgentStatus,
    last_output: String,
    output_length: usize,
}

/// ステータスポーラー
pub struct StatusPoller {
    /// ポーリング設定
    config: PollerConfig,
    /// 実行中フラグ
    running: Arc<AtomicBool>,
    /// ポーリングスレッドハンドル
    handle: Option<JoinHandle<()>>,
    /// エージェントの状態スナップショット
    snapshots: Arc<Mutex<HashMap<String, AgentSnapshot>>>,
}

impl StatusPoller {
    /// 新しいポーラーを作成
    pub fn new(config: Option<PollerConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
            snapshots: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// ポーリングを開始
    pub fn start<R: Runtime>(
        &mut self,
        app_handle: AppHandle<R>,
        orchestrator: Arc<Mutex<Option<TmuxOrchestrator>>>,
    ) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("Poller is already running".to_string());
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let config = self.config.clone();
        let snapshots = self.snapshots.clone();
        let parser = OutputParser::new();

        let handle = thread::spawn(move || {
            log::info("StatusPoller", &format!("Started with interval {}ms", config.interval_ms));

            while running.load(Ordering::SeqCst) {
                // オーケストレーターからエージェント一覧を取得
                let agents: Vec<PaneInfo> = {
                    let orch = orchestrator.lock();
                    if let Some(ref o) = *orch {
                        o.list_agents().into_iter().cloned().collect()
                    } else {
                        Vec::new()
                    }
                };

                // 各エージェントの状態をチェック
                for agent in agents {
                    let pane_content = {
                        let orch = orchestrator.lock();
                        if let Some(ref o) = *orch {
                            o.capture_pane_plain(&agent.pane_id).ok()
                        } else {
                            None
                        }
                    };

                    if let Some(content) = pane_content {
                        // デバッグ: コンテンツ全体の行数と最後の10行を表示
                        let total_lines = content.lines().count();
                        let last_lines: Vec<&str> = content.lines().rev().take(10).collect();
                        log::debug("StatusPoller", &format!("Agent {} captured {} lines, last 10 lines:", agent.agent_id, total_lines));
                        for line in last_lines.iter().rev() {
                            log::debug("StatusPoller", &format!("  {:?}", line));
                        }

                        // パーサーで状態を検出
                        let mut detected_status = parser.parse(&content);

                        // 選択メニューが表示されている場合はWaitingForInputとして扱う
                        if content.contains("Enter to select") || content.contains("↑/↓ to navigate") {
                            log::debug("StatusPoller", &format!("Agent {} has selection menu, forcing WaitingForInput", agent.agent_id));
                            // 選択肢を抽出
                            let options = extract_selection_options(&content);
                            detected_status = AgentStatus::WaitingForInput {
                                question: if options.is_empty() {
                                    "選択してください".to_string()
                                } else {
                                    options
                                },
                            };
                        }

                        log::debug("StatusPoller", &format!("Agent {} detected_status: {:?}", agent.agent_id, detected_status));

                        // 前回の状態と比較（更新前の状態を保存）
                        let (status_changed, old_status) = {
                            let mut snaps = snapshots.lock();
                            let prev = snaps.get(&agent.agent_id);

                            // 更新前の状態を保存
                            let old_status = match prev {
                                Some(prev) => prev.status.clone(),
                                None => AgentStatus::Unknown,
                            };

                            let changed = match prev {
                                Some(prev) => {
                                    // 状態が変化した、または出力が大きく変化した
                                    prev.status != detected_status
                                        || content.len().abs_diff(prev.output_length) > config.min_output_change
                                }
                                None => true,
                            };

                            // スナップショットを更新
                            snaps.insert(
                                agent.agent_id.clone(),
                                AgentSnapshot {
                                    status: detected_status.clone(),
                                    last_output: content.clone(),
                                    output_length: content.len(),
                                },
                            );

                            (changed, old_status)
                        };

                        // イベントを発火
                        if status_changed {
                            let old_status_str = match &old_status {
                                AgentStatus::Initializing => "Initializing".to_string(),
                                AgentStatus::Processing => "Processing".to_string(),
                                AgentStatus::Idle => "Idle".to_string(),
                                AgentStatus::WaitingForInput { question } => {
                                    format!("WaitingForInput:{}", question)
                                }
                                AgentStatus::Error { message } => format!("Error:{}", message),
                                AgentStatus::Unknown => "Unknown".to_string(),
                            };

                            let new_status_str = match &detected_status {
                                AgentStatus::Initializing => "Initializing",
                                AgentStatus::Processing => "Processing",
                                AgentStatus::Idle => "Idle",
                                AgentStatus::WaitingForInput { question } => {
                                    &format!("WaitingForInput:{}", question)
                                }
                                AgentStatus::Error { message } => &format!("Error:{}", message),
                                AgentStatus::Unknown => "Unknown",
                            };

                            // 状態変化イベント
                            let payload = StatusChangedPayload {
                                agent_id: agent.agent_id.clone(),
                                old_status: old_status_str.clone(),
                                new_status: new_status_str.to_string(),
                            };

                            if let Err(e) = app_handle.emit("tmux:status_changed", &payload) {
                                log::error("StatusPoller", &format!("Failed to emit status_changed: {:?}", e));
                            }

                            // 出力準備完了イベント（状態がIdleまたはWaitingForInputに変化した場合）
                            if matches!(detected_status, AgentStatus::Idle | AgentStatus::WaitingForInput { .. }) {
                                let output_payload = OutputReadyPayload {
                                    agent_id: agent.agent_id.clone(),
                                    content: parser.extract_meaningful_content(&content),
                                    content_length: content.len(),
                                };

                                if let Err(e) = app_handle.emit("tmux:output_ready", &output_payload) {
                                    log::error("StatusPoller", &format!("Failed to emit output_ready: {:?}", e));
                                }
                            }

                            // 質問イベント（WaitingForInputに変化した場合）
                            if let AgentStatus::WaitingForInput { question } = &detected_status {
                                // 前回の状態がWaitingForInputでない場合のみ通知
                                let was_waiting = matches!(old_status, AgentStatus::WaitingForInput { .. });
                                if !was_waiting {
                                    let question_payload = QuestionPayload {
                                        agent_id: agent.agent_id.clone(),
                                        question: question.clone(),
                                        question_id: format!("q-{}-{}", agent.agent_id, chrono::Utc::now().timestamp()),
                                        context: parser.extract_meaningful_content(&content),
                                    };

                                    if let Err(e) = app_handle.emit("tmux:question", &question_payload) {
                                        log::error("StatusPoller", &format!("Failed to emit question: {:?}", e));
                                    }

                                    log::info("StatusPoller", &format!("Agent {} asked: {}", agent.agent_id, question));
                                }
                            }

                            log::info(
                                "StatusPoller",
                                &format!("Agent {} status: {} -> {}", agent.agent_id, old_status_str, new_status_str)
                            );
                        }
                    }
                }

                // 次のポーリングまで待機
                thread::sleep(Duration::from_millis(config.interval_ms));
            }

            log::info("StatusPoller", "Stopped");
        });

        self.handle = Some(handle);
        Ok(())
    }

    /// ポーリングを停止
    pub fn stop(&mut self) -> Result<(), String> {
        if !self.running.load(Ordering::SeqCst) {
            return Err("Poller is not running".to_string());
        }

        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.handle.take() {
            // スレッドの終了を待つ（タイムアウト付き）
            // 注: スレッドがポーリング中の場合は少し待つ必要がある
            let _ = handle.join();
        }

        // スナップショットをクリア
        self.snapshots.lock().clear();

        Ok(())
    }

    /// ポーリング中かどうか
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// エージェントの現在の状態を取得
    pub fn get_agent_status(&self, agent_id: &str) -> Option<AgentStatus> {
        let snaps = self.snapshots.lock();
        snaps.get(agent_id).map(|s| s.status.clone())
    }

    /// 全エージェントの状態を取得
    pub fn get_all_statuses(&self) -> HashMap<String, AgentStatus> {
        let snaps = self.snapshots.lock();
        snaps.iter().map(|(k, v)| (k.clone(), v.status.clone())).collect()
    }
}

impl Drop for StatusPoller {
    fn drop(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            self.running.store(false, Ordering::SeqCst);
        }
    }
}

/// 選択肢を抽出する（"Enter to select"の前の選択肢行を探す）
/// 問題文と選択肢を返す（改行区切り）
/// フォーマット: "問題文\n---\n1. 選択肢1\n2. 選択肢2..."
fn extract_selection_options(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut options: Vec<(u32, String)> = Vec::new(); // (番号, 選択肢)
    let mut first_option_index: Option<usize> = None;

    log::debug("extract_selection_options", &format!("Total lines: {}", lines.len()));

    // ナビゲーション行のインデックスを見つける
    let nav_index = lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed.contains("Enter to select")
            || trimmed.contains("↑/↓ to navigate")
            || trimmed.contains("Tab/Arrow keys")
            || trimmed.contains("Esc to cancel")
    });

    let search_end = nav_index.unwrap_or(lines.len());
    log::debug("extract_selection_options", &format!("Search end: {}", search_end));

    // Claude Codeのデフォルト選択肢（除外対象）
    let excluded_options = ["Type something.", "Chat about this", "Ask about"];

    // 前から走査して選択肢を探す（ナビゲーション行まで）
    for (i, line) in lines.iter().take(search_end).enumerate() {
        let trimmed = line.trim();

        // 選択肢のパターン: "1. Option", "2. Option" など
        if let Some(num) = extract_option_number(trimmed) {
            // 先頭の記号を除去してクリーンな選択肢テキストを作成
            let cleaned = clean_option_text(trimmed);

            // 除外対象の選択肢かチェック
            let is_excluded = excluded_options.iter().any(|ex| cleaned.contains(ex));

            if !is_excluded {
                log::debug("extract_selection_options", &format!("Found option {} at {}: {}", num, i, cleaned));
                if first_option_index.is_none() {
                    first_option_index = Some(i);
                }
                options.push((num, cleaned));
            } else {
                log::debug("extract_selection_options", &format!("Excluded option: {}", cleaned));
            }
        }
    }

    log::debug("extract_selection_options", &format!("Total options found: {}", options.len()));

    if options.is_empty() {
        return String::new();
    }

    // 問題文を抽出（最初の選択肢の直前の連続する非空行ブロック）
    let question_text = if let Some(first_idx) = first_option_index {
        // 最初の選択肢より前の行を後ろから走査して、問題文ブロックを見つける
        let mut question_lines: Vec<&str> = Vec::new();
        let mut found_content = false;

        for line in lines.iter().take(first_idx).rev() {
            let trimmed = line.trim();

            // 除外すべき行かチェック
            let should_exclude = trimmed.is_empty()
                || trimmed.starts_with("❯")
                || trimmed.starts_with(">")
                || trimmed.contains("Cooked for")
                || trimmed.starts_with("───")
                || trimmed.contains("? for shortcuts");

            if should_exclude {
                if found_content {
                    // すでに問題文を見つけた後に除外行が来たら、そこで終了
                    break;
                }
                // まだ問題文を見つけていない場合はスキップ
                continue;
            }

            // 問題文の行を追加
            question_lines.push(*line);
            found_content = true;
        }

        // 元の順序に戻す
        question_lines.reverse();
        question_lines.join("\n")
    } else {
        String::new()
    };

    log::debug("extract_selection_options", &format!("Question text: {:?}", question_text));

    // 結果を構築
    let mut result = String::new();

    if !question_text.is_empty() {
        result.push_str(&question_text);
        result.push_str("\n---\n");
    }

    // 選択肢を追加
    for (_, opt) in &options {
        result.push_str(opt);
        result.push('\n');
    }

    result.trim_end().to_string()
}

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

/// 行が選択肢かどうかを判定
fn is_option_line(line: &str) -> bool {
    // "数字. " または "数字:" のパターン（1. 2. 3. または 1: 2: 3:）
    if let Some(first_char) = line.chars().next() {
        if first_char.is_ascii_digit() {
            // "1. " または "1: " または "1." のパターンを探す
            if line.starts_with(|c: char| c.is_ascii_digit()) {
                // 数字の後に続く文字を確認
                let rest = line.trim_start_matches(|c: char| c.is_ascii_digit());
                if rest.starts_with(". ") || rest.starts_with(": ") || rest.starts_with(".") {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poller_config_default() {
        let config = PollerConfig::default();
        assert_eq!(config.interval_ms, 500);
        assert_eq!(config.min_output_change, 10);
    }

    #[test]
    fn test_poller_not_running_initially() {
        let poller = StatusPoller::new(None);
        assert!(!poller.is_running());
    }

    #[test]
    fn test_extract_selection_options() {
        // 基本的な選択肢（改行区切りで返される）
        let content = "Which option?\n1. Option A\n2. Option B\n3. Option C\n\nEnter to select";
        let result = extract_selection_options(content);
        assert!(result.contains("1. Option A"));
        assert!(result.contains("2. Option B"));
        assert!(result.contains("3. Option C"));

        // 選択肢がない場合
        let content2 = "No options here\nEnter to select";
        let result2 = extract_selection_options(content2);
        assert!(result2.is_empty());

        // 最初の選択肢が欠けている場合
        let content3 = "2. Second\n3. Third\n\nEnter to select";
        let result3 = extract_selection_options(content3);
        assert!(result3.contains("※")); // 警告メッセージが含まれる
    }

    #[test]
    fn test_extract_option_number() {
        assert_eq!(extract_option_number("1. First"), Some(1));
        assert_eq!(extract_option_number("2. Second"), Some(2));
        assert_eq!(extract_option_number("10. Tenth"), Some(10));
        assert_eq!(extract_option_number("No number"), None);
        assert_eq!(extract_option_number("1abc"), None); // ドットがない

        // 先頭に記号がある場合
        assert_eq!(extract_option_number("❯ 1. First"), Some(1));
        assert_eq!(extract_option_number("> 2. Second"), Some(2));
        assert_eq!(extract_option_number("  3. Third"), Some(3)); // インデント
    }
}
