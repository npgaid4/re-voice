mod acp;
mod pty;

use chrono;
use parking_lot::Mutex;
use pty::{PtyEvent, PtyManager};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

use acp::{
    AgentCard, AgentOrchestrator, DiscoveryQuery, OrchestratorStats, SharedContext, TaskState,
    Transport, StatusPoller, PollerConfig,
};
use acp::tmux::{TmuxOrchestrator, AgentType as TmuxAgentType};

/// Application state
pub struct AppState {
    pty: Arc<Mutex<PtyManager>>,
    orchestrator: Arc<Mutex<AgentOrchestrator>>,
    tmux_orchestrator: Arc<Mutex<Option<TmuxOrchestrator>>>,
    status_poller: Arc<Mutex<Option<StatusPoller>>>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            pty: Arc::new(Mutex::new(PtyManager::new())),
            orchestrator: Arc::new(Mutex::new(AgentOrchestrator::new())),
            tmux_orchestrator: Arc::new(Mutex::new(None)),
            status_poller: Arc::new(Mutex::new(None)),
            app_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// AppHandleを設定（初期化時に呼ぶ）
    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock() = Some(handle);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Legacy PTY Commands (for backwards compatibility)
// ============================================================================

/// Claude Codeを起動
#[tauri::command]
fn spawn_claude(state: State<AppState>, app_handle: AppHandle) -> Result<String, String> {
    // AppHandleを保存
    state.set_app_handle(app_handle.clone());

    let mut pty = state.pty.lock();

    // イベントコールバックを設定
    let handle = app_handle.clone();
    pty.set_event_callback(move |event| {
        let now = chrono::Local::now();
        let ts = now.format("%H:%M:%S%.3f");

        match event {
            PtyEvent::Output(text) => {
                eprintln!("[{}] [PTY OUTPUT EVENT] {} bytes", ts, text.len());
                eprintln!("[{}] [PTY OUTPUT CONTENT] {:?}", ts, text);
                // フロントエンドにイベントを送信
                if let Err(e) = handle.emit("pty-output", &text) {
                    eprintln!("[{}] [PTY EMIT ERROR] {}", ts, e);
                }
            }
            PtyEvent::Prompt => {
                eprintln!("[{}] [PTY PROMPT EVENT]", ts);
                let _ = handle.emit("pty-prompt", ());
            }
            PtyEvent::Error(msg) => {
                eprintln!("[{}] [PTY ERROR EVENT] {}", ts, msg);
                let _ = handle.emit("pty-error", &msg);
            }
            PtyEvent::InputRequired { prompt_type, context } => {
                eprintln!("[{}] [PTY INPUT REQUIRED EVENT] {:?}", ts, prompt_type);
                // フロントエンドに入力要求イベントを送信
                let payload = serde_json::json!({
                    "promptType": prompt_type,
                    "context": context,
                });
                let _ = handle.emit("pty-input-required", &payload);
            }
        }
    });

    pty.spawn_claude_code().map_err(|e| e.to_string())?;
    Ok("Claude Code started".to_string())
}

/// Claude Codeにメッセージを送信
#[tauri::command]
fn send_to_claude(state: State<AppState>, message: String) -> Result<String, String> {
    let now = chrono::Local::now();
    eprintln!("[{}] [send_to_claude] called with {} bytes", now.format("%H:%M:%S%.3f"), message.len());

    let pty = state.pty.lock();
    pty.send_message(&message).map_err(|e| e.to_string())?;

    let now = chrono::Local::now();
    eprintln!("[{}] [send_to_claude] completed", now.format("%H:%M:%S%.3f"));
    Ok("Message sent".to_string())
}

/// Claude Codeから出力を取得
#[tauri::command]
fn read_from_claude(state: State<AppState>) -> Result<String, String> {
    let pty = state.pty.lock();
    Ok(pty.get_output())
}

/// PTYテスト: 送信直後に読み取り
#[tauri::command]
fn pty_test_roundtrip(state: State<AppState>, message: String) -> Result<String, String> {
    let now = chrono::Local::now();
    eprintln!("[{}] [pty_test_roundtrip] Starting", now.format("%H:%M:%S%.3f"));

    let pty = state.pty.lock();

    // 送信
    pty.send_message(&message).map_err(|e| e.to_string())?;

    // 少し待機
    drop(pty);
    std::thread::sleep(std::time::Duration::from_millis(1000));

    // 読み取り
    let pty = state.pty.lock();
    let output = pty.get_output();

    let now = chrono::Local::now();
    eprintln!("[{}] [pty_test_roundtrip] Output: {} chars", now.format("%H:%M:%S%.3f"), output.len());

    Ok(output)
}

/// 現在のレスポンスを取得（最後のメッセージ送信以降の出力）
#[tauri::command]
fn get_claude_response(state: State<AppState>) -> Result<String, String> {
    let pty = state.pty.lock();
    Ok(pty.get_response())
}

/// Claude Codeが起動しているか確認
#[tauri::command]
fn is_claude_running(state: State<AppState>) -> bool {
    let pty = state.pty.lock();
    pty.is_running()
}

/// 子プロセスが生きているか確認
#[tauri::command]
fn is_child_alive(state: State<AppState>) -> bool {
    let mut pty = state.pty.lock();
    pty.is_child_alive()
}

/// 子プロセスのPIDを取得
#[tauri::command]
fn get_child_pid(state: State<AppState>) -> Option<u32> {
    let pty = state.pty.lock();
    pty.child_pid()
}

/// テスト用: 汎用コマンドを実行
#[tauri::command]
fn execute_command(state: State<AppState>, command: String) -> Result<String, String> {
    let pty = state.pty.lock();

    if !pty.is_running() {
        drop(pty);

        let path = std::env::var("PATH").unwrap_or_default();
        let extended_path = format!("/opt/homebrew/bin:/usr/local/bin:{}", path);

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .env("PATH", &extended_path)
            .output()
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(stdout)
        } else {
            Err(format!("{}{}", stdout, stderr))
        }
    } else {
        pty.send_message(&command).map_err(|e| e.to_string())?;
        Ok("Command sent".to_string())
    }
}

// ============================================================================
// ACP Commands
// ============================================================================

/// ACP: エージェントを登録
#[tauri::command]
fn acp_register_agent(
    state: State<AppState>,
    agent_type: String,
    instance_id: String,
) -> Result<String, String> {
    let orchestrator = state.orchestrator.lock();

    // Create agent card based on type
    let card = match agent_type.as_str() {
        "claude-code" => AgentCard::claude_code(&instance_id),
        _ => return Err(format!("Unknown agent type: {}", agent_type)),
    };

    let agent_id = card.id.clone();
    orchestrator
        .register_agent_card(card)
        .map_err(|e| e.to_string())?;

    Ok(agent_id)
}

/// ACP: エージェントを発見
#[tauri::command]
fn acp_discover_agents(
    state: State<AppState>,
    capabilities: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    transport: Option<String>,
) -> Result<Vec<AgentCard>, String> {
    let orchestrator = state.orchestrator.lock();

    let mut query = DiscoveryQuery::new();

    if let Some(caps) = capabilities {
        query = query.with_capabilities(caps);
    }

    if let Some(t) = tags {
        query = query.with_tags(t);
    }

    if let Some(tr) = transport {
        let transport_type = match tr.as_str() {
            "pty" => Transport::Pty,
            "stdio" => Transport::Stdio,
            "websocket" => Transport::WebSocket,
            "http" => Transport::Http,
            _ => return Err(format!("Unknown transport type: {}", tr)),
        };
        query = query.with_transport(transport_type);
    }

    Ok(orchestrator.discover_agents(&query))
}

/// ACP: 全エージェント一覧を取得
#[tauri::command]
fn acp_list_agents(state: State<AppState>) -> Vec<AgentCard> {
    let orchestrator = state.orchestrator.lock();
    orchestrator.list_agents()
}

/// ACP: エージェント情報を取得
#[tauri::command]
fn acp_get_agent(state: State<AppState>, agent_id: String) -> Option<AgentCard> {
    let orchestrator = state.orchestrator.lock();
    orchestrator.get_agent(&agent_id)
}

/// ACP: メッセージを送信（イベント駆動）
/// メッセージを送信し、応答は "pty-output" イベントで通知される
#[tauri::command]
fn acp_send_message(
    state: State<AppState>,
    to: String,
    content: String,
    _from: String,
) -> Result<String, String> {
    let now = chrono::Local::now();
    eprintln!("[{}] [acp_send_message] Sending to {}: {:?}", now.format("%H:%M:%S%.3f"), to, content);

    let pty = state.pty.lock();

    if pty.is_running() {
        // メッセージ送信（応答はイベントで通知）
        pty.send_message(&content).map_err(|e| e.to_string())?;
        Ok(format!("Message sent to {}. Response will arrive via pty-output events.", to))
    } else {
        Err(format!("Agent {} is not running", to))
    }
}

/// ACP: 現在のレスポンスを取得
#[tauri::command]
fn acp_get_response(state: State<AppState>) -> Result<String, String> {
    let pty = state.pty.lock();
    Ok(pty.get_response())
}

/// ACP: ブロードキャスト（簡易実装）
#[tauri::command]
fn acp_broadcast(
    state: State<AppState>,
    content: String,
    _capabilities: Option<Vec<String>>,
    _from: String,
) -> Result<Vec<String>, String> {
    // For now, just send to the legacy PTY if running
    let pty = state.pty.lock();

    if pty.is_running() {
        pty.send_message(&content).map_err(|e| e.to_string())?;
        Ok(vec!["Message broadcasted".to_string()])
    } else {
        Ok(vec![])
    }
}

/// ACP: タスク状態を取得
#[tauri::command]
fn acp_get_task(state: State<AppState>, task_id: String) -> Option<TaskState> {
    let orchestrator = state.orchestrator.lock();
    orchestrator.get_task(&task_id)
}

/// ACP: 統計情報を取得
#[tauri::command]
fn acp_stats(state: State<AppState>) -> OrchestratorStats {
    let orchestrator = state.orchestrator.lock();
    orchestrator.stats()
}

/// ACP: 共有コンテキストを取得
#[tauri::command]
fn acp_get_context(state: State<AppState>) -> SharedContext {
    let orchestrator = state.orchestrator.lock();
    orchestrator.get_shared_context()
}

// ============================================================================
// YouTube/Subtitle Commands
// ============================================================================

/// 字幕情報を取得
#[tauri::command]
fn get_available_subtitles(url: String) -> Result<String, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let extended_path = format!("/opt/homebrew/bin:/usr/local/bin:{}", path);

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("yt-dlp --list-subs \"{}\"", url))
        .env("PATH", &extended_path)
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(format!("{}{}", stdout, stderr))
    }
}

/// 字幕をダウンロード
#[tauri::command]
fn download_subtitles(url: String, lang: String, output_path: String) -> Result<String, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let extended_path = format!("/opt/homebrew/bin:/usr/local/bin:{}", path);

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "yt-dlp --write-subs --sub-lang {} --skip-download --sub-format vtt --output \"{}\" \"{}\"",
            lang, output_path, url
        ))
        .env("PATH", &extended_path)
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(format!("字幕をダウンロードしました: {}.{}.vtt", output_path, lang))
    } else {
        Err(format!("{}{}", stdout, stderr))
    }
}

/// 自動生成字幕をダウンロード（手動字幕がない場合）
#[tauri::command]
fn download_auto_subtitles(url: String, lang: String, output_path: String) -> Result<String, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let extended_path = format!("/opt/homebrew/bin:/usr/local/bin:{}", path);

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "yt-dlp --write-auto-subs --sub-lang {} --skip-download --sub-format vtt --output \"{}\" \"{}\"",
            lang, output_path, url
        ))
        .env("PATH", &extended_path)
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(format!("自動生成字幕をダウンロードしました: {}.{}.vtt", output_path, lang))
    } else {
        Err(format!("{}{}", stdout, stderr))
    }
}

// ============================================================================
// tmux Test Commands (ACP v2 PoC)
// ============================================================================

/// tmuxセッションを作成
#[tauri::command]
fn tmux_create_session(state: State<AppState>) -> Result<String, String> {
    let mut tmux = state.tmux_orchestrator.lock();
    let mut orch = TmuxOrchestrator::new("revoice");
    orch.create_session().map_err(|e| e.to_string())?;
    *tmux = Some(orch);
    Ok("tmux session created".to_string())
}

/// tmuxエージェントを起動
#[tauri::command]
fn tmux_spawn_agent(
    state: State<AppState>,
    agent_id: String,
    agent_type: String,
    capabilities: Vec<String>,
) -> Result<String, String> {
    let mut tmux = state.tmux_orchestrator.lock();
    if let Some(ref mut orch) = *tmux {
        let atype = match agent_type.as_str() {
            "claude-code" => TmuxAgentType::ClaudeCode,
            "codex" => TmuxAgentType::Codex,
            _ => TmuxAgentType::GenericShell,
        };
        let pane_id = orch.spawn_agent(&agent_id, atype, capabilities)
            .map_err(|e| e.to_string())?;
        Ok(pane_id)
    } else {
        Err("Session not created. Call tmux_create_session first.".to_string())
    }
}

/// tmuxペインの内容を取得
#[tauri::command]
fn tmux_capture_pane(state: State<AppState>, agent_id: String) -> Result<String, String> {
    let tmux = state.tmux_orchestrator.lock();
    if let Some(ref orch) = *tmux {
        if let Some(pane_id) = orch.get_pane_id(&agent_id) {
            orch.capture_pane_plain(pane_id).map_err(|e| e.to_string())
        } else {
            Err(format!("Agent not found: {}", agent_id))
        }
    } else {
        Err("Session not created".to_string())
    }
}

/// tmuxペインにメッセージを送信
#[tauri::command]
fn tmux_send_message(state: State<AppState>, agent_id: String, message: String) -> Result<(), String> {
    let tmux = state.tmux_orchestrator.lock();
    if let Some(ref orch) = *tmux {
        if let Some(pane_id) = orch.get_pane_id(&agent_id) {
            orch.send_keys(pane_id, &message).map_err(|e| e.to_string())
        } else {
            Err(format!("Agent not found: {}", agent_id))
        }
    } else {
        Err("Session not created".to_string())
    }
}

/// tmuxエージェントの状態を取得
#[tauri::command]
fn tmux_get_status(state: State<AppState>, agent_id: String) -> Result<String, String> {
    let tmux = state.tmux_orchestrator.lock();
    if let Some(ref orch) = *tmux {
        if let Some(pane_id) = orch.get_pane_id(&agent_id) {
            let status = orch.detect_status(pane_id);
            Ok(format!("{:?}", status))
        } else {
            Err(format!("Agent not found: {}", agent_id))
        }
    } else {
        Err("Session not created".to_string())
    }
}

/// tmuxエージェント一覧を取得
#[tauri::command]
fn tmux_list_agents(state: State<AppState>) -> Result<Vec<serde_json::Value>, String> {
    let tmux = state.tmux_orchestrator.lock();
    if let Some(ref orch) = *tmux {
        let agents: Vec<serde_json::Value> = orch.list_agents().iter().map(|p| {
            serde_json::json!({
                "agent_id": p.agent_id,
                "pane_id": p.pane_id,
                "agent_type": format!("{:?}", p.agent_type),
                "capabilities": p.capabilities,
                "status": format!("{:?}", p.status),
            })
        }).collect();
        Ok(agents)
    } else {
        Ok(vec![])
    }
}

/// tmuxセッションを終了
#[tauri::command]
fn tmux_destroy_session(state: State<AppState>) -> Result<(), String> {
    // ポーリングを停止
    {
        let mut poller = state.status_poller.lock();
        if let Some(ref mut p) = *poller {
            let _ = p.stop();
        }
        *poller = None;
    }

    let mut tmux = state.tmux_orchestrator.lock();
    if let Some(ref mut orch) = *tmux {
        orch.destroy_session().map_err(|e| e.to_string())?;
    }
    *tmux = None;
    Ok(())
}

/// tmuxステータスポーリングを開始
#[tauri::command]
fn tmux_start_polling(
    app_handle: AppHandle,
    state: State<AppState>,
    interval_ms: Option<u64>,
) -> Result<(), String> {
    // 既にポーリング中かチェック
    {
        let poller = state.status_poller.lock();
        if let Some(ref p) = *poller {
            if p.is_running() {
                return Err("Polling is already running".to_string());
            }
        }
    }

    // 新しいポーラーを作成
    let config = interval_ms.map(|ms| PollerConfig {
        interval_ms: ms,
        ..Default::default()
    });

    let mut poller = StatusPoller::new(config);
    let orch = state.tmux_orchestrator.clone();

    poller.start(app_handle, orch).map_err(|e| e.to_string())?;

    // ポーラーを保存
    {
        let mut p = state.status_poller.lock();
        *p = Some(poller);
    }

    eprintln!("[tmux_start_polling] Polling started");
    Ok(())
}

/// tmuxステータスポーリングを停止
#[tauri::command]
fn tmux_stop_polling(state: State<AppState>) -> Result<(), String> {
    let mut poller = state.status_poller.lock();
    if let Some(ref mut p) = *poller {
        p.stop().map_err(|e| e.to_string())?;
    }
    *poller = None;

    eprintln!("[tmux_stop_polling] Polling stopped");
    Ok(())
}

/// ポーリング状態を取得
#[tauri::command]
fn tmux_is_polling(state: State<AppState>) -> bool {
    let poller = state.status_poller.lock();
    if let Some(ref p) = *poller {
        p.is_running()
    } else {
        false
    }
}

// ============================================================================
// Application Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // Legacy PTY commands
            spawn_claude,
            send_to_claude,
            read_from_claude,
            get_claude_response,
            is_claude_running,
            is_child_alive,
            get_child_pid,
            execute_command,
            pty_test_roundtrip,
            // ACP commands
            acp_register_agent,
            acp_discover_agents,
            acp_list_agents,
            acp_get_agent,
            acp_send_message,
            acp_get_response,
            acp_broadcast,
            acp_get_task,
            acp_stats,
            acp_get_context,
            // YouTube/Subtitle commands
            get_available_subtitles,
            download_subtitles,
            download_auto_subtitles,
            // tmux test commands (ACP v2 PoC)
            tmux_create_session,
            tmux_spawn_agent,
            tmux_capture_pane,
            tmux_send_message,
            tmux_get_status,
            tmux_list_agents,
            tmux_destroy_session,
            tmux_start_polling,
            tmux_stop_polling,
            tmux_is_polling,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
