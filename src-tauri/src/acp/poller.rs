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
use super::tmux::{AgentStatus, AgentType, PaneInfo, TmuxOrchestrator};

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
            interval_ms: 500,
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
            eprintln!("[StatusPoller] Started with interval {}ms", config.interval_ms);

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
                        // パーサーで状態を検出
                        let detected_status = parser.parse(&content);

                        // 前回の状態と比較
                        let status_changed = {
                            let mut snaps = snapshots.lock();
                            let prev = snaps.get(&agent.agent_id);

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

                            changed
                        };

                        // イベントを発火
                        if status_changed {
                            let old_status_str = match &agent.status {
                                AgentStatus::Initializing => "Initializing",
                                AgentStatus::Processing => "Processing",
                                AgentStatus::Idle => "Idle",
                                AgentStatus::WaitingForInput { .. } => "WaitingForInput",
                                AgentStatus::Error { .. } => "Error",
                                AgentStatus::Unknown => "Unknown",
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
                                old_status: old_status_str.to_string(),
                                new_status: new_status_str.to_string(),
                            };

                            if let Err(e) = app_handle.emit("tmux:status_changed", &payload) {
                                eprintln!("[StatusPoller] Failed to emit status_changed: {:?}", e);
                            }

                            // 出力準備完了イベント（状態がIdleまたはWaitingForInputに変化した場合）
                            if matches!(detected_status, AgentStatus::Idle | AgentStatus::WaitingForInput { .. }) {
                                let output_payload = OutputReadyPayload {
                                    agent_id: agent.agent_id.clone(),
                                    content: parser.extract_meaningful_content(&content),
                                    content_length: content.len(),
                                };

                                if let Err(e) = app_handle.emit("tmux:output_ready", &output_payload) {
                                    eprintln!("[StatusPoller] Failed to emit output_ready: {:?}", e);
                                }
                            }

                            eprintln!(
                                "[StatusPoller] Agent {} status: {} -> {}",
                                agent.agent_id, old_status_str, new_status_str
                            );
                        }
                    }
                }

                // 次のポーリングまで待機
                thread::sleep(Duration::from_millis(config.interval_ms));
            }

            eprintln!("[StatusPoller] Stopped");
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
}
