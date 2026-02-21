mod acp;
mod log;
mod pty;
mod voicevox;
mod youtube;

use chrono;
use parking_lot::Mutex;
use pty::{PtyEvent, PtyManager};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

use acp::{
    AgentCard, AgentOrchestrator, DiscoveryQuery, OrchestratorStats, SharedContext, TaskState,
    Transport, StatusPoller, PollerConfig, CapabilityFilter,
    PipelineDefinition, PipelineExecution, PipelineExecutor, PipelineStage, AgentAddress,
    AskToolHandler, HumanAnswer, ParsedQuestion,
    ClaudeCodeExecutor, ExecutorOptions, AgentState,
};
use acp::permission::PermissionDecision;
use acp::tmux::{TmuxOrchestrator, AgentType as TmuxAgentType};
use acp::runner::{PipelineRunner, ExecutionContext, ProgressPayload};
use acp::subtitle_parser::{VttParser, SubtitleSegment};
use voicevox::{VoicevoxClient, VoicevoxError, Speaker, SynthesisOptions};
use youtube::{YoutubeDownloader, SubtitleDownloadResult, YoutubeError};

/// Application state
pub struct AppState {
    pty: Arc<Mutex<PtyManager>>,
    orchestrator: Arc<Mutex<AgentOrchestrator>>,
    tmux_orchestrator: Arc<Mutex<Option<TmuxOrchestrator>>>,
    status_poller: Arc<Mutex<Option<StatusPoller>>>,
    pipeline_executor: Arc<Mutex<PipelineExecutor>>,
    pipeline_runner: Arc<PipelineRunner>,
    voicevox_client: Arc<Mutex<VoicevoxClient>>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
    /// CLI-based Claude Code executor (async-aware)
    cli_executor: Arc<RwLock<Option<ClaudeCodeExecutor>>>,
}

impl AppState {
    pub fn new() -> Self {
        let pipeline_executor = Arc::new(Mutex::new(PipelineExecutor::new()));
        let tmux_orchestrator: Arc<Mutex<Option<TmuxOrchestrator>>> = Arc::new(Mutex::new(None));
        let executor = pipeline_executor.clone();
        let cli_executor: Arc<RwLock<Option<ClaudeCodeExecutor>>> = Arc::new(RwLock::new(None));

        // CLIエグゼキューターをPipelineRunnerに注入
        let pipeline_runner = Arc::new(PipelineRunner::with_cli_executor(
            executor,
            cli_executor.clone(),
        ));

        Self {
            pty: Arc::new(Mutex::new(PtyManager::new())),
            orchestrator: Arc::new(Mutex::new(AgentOrchestrator::new())),
            tmux_orchestrator,
            status_poller: Arc::new(Mutex::new(None)),
            pipeline_executor,
            pipeline_runner,
            voicevox_client: Arc::new(Mutex::new(VoicevoxClient::new())),
            app_handle: Arc::new(Mutex::new(None)),
            cli_executor,
        }
    }

    /// AppHandleを設定（初期化時に呼ぶ）
    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock() = Some(handle.clone());

        // PipelineRunnerにも設定
        self.pipeline_runner.set_app_handle(handle);
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

    let agent_id = card.id.clone().unwrap_or_else(|| card.name.clone());
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

/// yt-dlpが利用可能か確認
#[tauri::command]
fn check_ytdlp_available() -> Result<(), String> {
    let downloader = YoutubeDownloader::new();
    downloader.check_available().map_err(|e| e.to_string())
}

/// 字幕をダウンロード（Rust版）
#[tauri::command]
fn youtube_download_subtitle(
    url: String,
    output_dir: String,
    lang: String,
) -> Result<SubtitleDownloadResult, String> {
    let downloader = YoutubeDownloader::new();
    downloader.download_subtitle(&url, &output_dir, &lang)
        .map_err(|e| e.to_string())
}

/// 利用可能な字幕言語一覧を取得
#[tauri::command]
fn youtube_list_subs(url: String) -> Result<Vec<String>, String> {
    let downloader = YoutubeDownloader::new();
    downloader.list_available_subs(&url)
        .map_err(|e| e.to_string())
}

/// 字幕情報を取得（レガシー）
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

/// 字幕をダウンロード（レガシー）
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

/// 自動生成字幕をダウンロード（手動字幕がない場合・レガシー）
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

/// 質問に回答する
#[tauri::command]
fn tmux_answer_question(
    state: State<AppState>,
    agent_id: String,
    answer: String,
) -> Result<(), String> {
    log::info("tmux_answer_question", &format!("Answer request: agent={}, answer={}", agent_id, answer));
    let tmux = state.tmux_orchestrator.lock();
    if let Some(ref orch) = *tmux {
        if let Some(pane_id) = orch.get_pane_id(&agent_id) {
            // Askツール用: 矢印キーで選択肢を選択
            orch.select_option(pane_id, &answer).map_err(|e| {
                log::error("tmux_answer_question", &format!("select_option error: {}", e));
                e.to_string()
            })?;
            log::info("tmux_answer_question", &format!("Answer sent to {}: {}", agent_id, answer));
            Ok(())
        } else {
            let err = format!("Agent not found: {}", agent_id);
            log::error("tmux_answer_question", &err);
            Err(err)
        }
    } else {
        let err = "Session not created".to_string();
        log::error("tmux_answer_question", &err);
        Err(err)
    }
}

/// エージェントの現在の状態を取得
#[tauri::command]
fn tmux_get_agent_status(state: State<AppState>, agent_id: String) -> Result<String, String> {
    let poller = state.status_poller.lock();
    if let Some(ref p) = *poller {
        if let Some(status) = p.get_agent_status(&agent_id) {
            Ok(format!("{:?}", status))
        } else {
            Ok("Unknown".to_string())
        }
    } else {
        Ok("Poller not running".to_string())
    }
}

// ============================================================================
// ACP v3: Pipeline Commands
// ============================================================================

/// パイプラインを定義
#[tauri::command]
fn acp_define_pipeline(
    state: State<AppState>,
    name: String,
    stages: Vec<serde_json::Value>,
) -> Result<String, String> {
    let executor = state.pipeline_executor.lock();

    let mut pipeline = PipelineDefinition::new(&name);

    for stage_json in stages {
        let stage: PipelineStage = serde_json::from_value(stage_json)
            .map_err(|e| format!("Invalid stage definition: {}", e))?;
        pipeline = pipeline.add_stage(stage);
    }

    let pipeline_id = executor.register(pipeline);
    log::info("acp_define_pipeline", &format!("Pipeline defined: {} -> {}", name, pipeline_id));

    Ok(pipeline_id)
}

/// パイプラインを実行
#[tauri::command]
fn acp_execute_pipeline(
    state: State<AppState>,
    pipeline_id: String,
) -> Result<PipelineExecution, String> {
    let executor = state.pipeline_executor.lock();

    let execution = executor.start_execution(&pipeline_id)
        .map_err(|e| e.to_string())?;

    log::info("acp_execute_pipeline", &format!(
        "Pipeline {} started, execution_id: {}",
        pipeline_id, execution.execution_id
    ));

    Ok(execution)
}

/// パイプライン実行状態を取得
#[tauri::command]
fn acp_get_pipeline_status(
    state: State<AppState>,
    execution_id: String,
) -> Option<PipelineExecution> {
    let executor = state.pipeline_executor.lock();
    executor.get_execution(&execution_id)
}

/// パイプラインのステージを完了（内部用）
#[tauri::command]
fn acp_complete_pipeline_stage(
    state: State<AppState>,
    execution_id: String,
    output: serde_json::Value,
) -> Result<PipelineExecution, String> {
    let executor = state.pipeline_executor.lock();
    executor.complete_stage(&execution_id, output)
        .map_err(|e| e.to_string())
}

/// パイプラインをキャンセル
#[tauri::command]
fn acp_cancel_pipeline(
    state: State<AppState>,
    execution_id: String,
) -> Result<PipelineExecution, String> {
    let executor = state.pipeline_executor.lock();
    executor.cancel_execution(&execution_id)
        .map_err(|e| e.to_string())
}

/// パイプライン一覧を取得
#[tauri::command]
fn acp_list_pipelines(state: State<AppState>) -> Vec<serde_json::Value> {
    let executor = state.pipeline_executor.lock();
    executor.list_pipelines().iter().map(|p| {
        serde_json::json!({
            "id": p.id,
            "name": p.name,
            "stage_count": p.stages.len(),
            "stop_on_failure": p.stop_on_failure,
        })
    }).collect()
}

/// アクティブなパイプライン実行一覧を取得
#[tauri::command]
fn acp_list_active_executions(state: State<AppState>) -> Vec<PipelineExecution> {
    let executor = state.pipeline_executor.lock();
    executor.get_active_executions()
}

// ============================================================================
// ACP v3: Enhanced Broadcast Commands
// ============================================================================

/// ブロードキャスト（v3 - フィルター対応）
#[tauri::command]
fn acp_broadcast_v3(
    state: State<AppState>,
    content: String,
    filter: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    // tmuxオーケストレーターがあれば使用
    let tmux = state.tmux_orchestrator.lock();

    if let Some(ref orch) = *tmux {
        let cap_filter = if let Some(f) = filter {
            let cf: CapabilityFilter = serde_json::from_value(f)
                .map_err(|e| format!("Invalid filter: {}", e))?;
            Some(cf)
        } else {
            None
        };

        let (success, failures) = orch.broadcast_message(&content, cap_filter.as_ref());

        Ok(serde_json::json!({
            "success": success,
            "failures": failures,
            "total_sent": success.len(),
            "total_failed": failures.len(),
        }))
    } else {
        // フォールバック: レガシーPTY
        let pty = state.pty.lock();
        if pty.is_running() {
            pty.send_message(&content).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "success": ["legacy-pty"],
                "failures": [],
                "total_sent": 1,
                "total_failed": 0,
            }))
        } else {
            Ok(serde_json::json!({
                "success": [],
                "failures": [],
                "total_sent": 0,
                "total_failed": 0,
            }))
        }
    }
}

/// アイドル状態のエージェントにのみブロードキャスト
#[tauri::command]
fn acp_broadcast_to_idle(
    state: State<AppState>,
    content: String,
    filter: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let tmux = state.tmux_orchestrator.lock();

    if let Some(ref orch) = *tmux {
        let cap_filter = if let Some(f) = filter {
            let cf: CapabilityFilter = serde_json::from_value(f)
                .map_err(|e| format!("Invalid filter: {}", e))?;
            Some(cf)
        } else {
            None
        };

        let (success, failures) = orch.broadcast_to_idle(&content, cap_filter.as_ref());

        Ok(serde_json::json!({
            "success": success,
            "failures": failures,
            "total_sent": success.len(),
            "total_failed": failures.len(),
        }))
    } else {
        Err("No tmux session available".to_string())
    }
}

/// エージェントを検索（v3 - CapabilityFilter対応）
#[tauri::command]
fn acp_discover_agents_v3(
    state: State<AppState>,
    filter: Option<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    let tmux = state.tmux_orchestrator.lock();

    if let Some(ref orch) = *tmux {
        let cap_filter = if let Some(f) = filter {
            let cf: CapabilityFilter = serde_json::from_value(f)
                .map_err(|e| format!("Invalid filter: {}", e))?;
            cf
        } else {
            CapabilityFilter::default()
        };

        let agents = orch.discover_agents(&cap_filter);

        Ok(agents.iter().map(|p| {
            serde_json::json!({
                "agent_id": p.agent_id,
                "pane_id": p.pane_id,
                "agent_type": format!("{:?}", p.agent_type),
                "capabilities": p.capabilities,
                "status": format!("{:?}", p.status),
            })
        }).collect())
    } else {
        Ok(vec![])
    }
}

/// エージェント統計を取得（v3 - 拡張版）
#[tauri::command]
fn acp_stats_v3(state: State<AppState>) -> serde_json::Value {
    let tmux = state.tmux_orchestrator.lock();

    if let Some(ref orch) = *tmux {
        let status_counts = orch.count_by_status();
        let active_executions = {
            let executor = state.pipeline_executor.lock();
            executor.get_active_executions().len()
        };

        serde_json::json!({
            "total_agents": orch.agent_count(),
            "status_breakdown": status_counts,
            "active_pipelines": active_executions,
        })
    } else {
        serde_json::json!({
            "total_agents": 0,
            "status_breakdown": {},
            "active_pipelines": 0,
        })
    }
}

// ============================================================================
// Pipeline Runner Commands (Phase 3)
// ============================================================================

/// 字幕翻訳パイプラインを実行（非同期・バックグラウンド）
#[tauri::command]
async fn run_subtitle_pipeline(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    youtube_url: String,
    subtitle_lang: String,
    output_dir: String,
) -> Result<String, String> {
    eprintln!("[run_subtitle_pipeline] ===== STARTING =====");
    eprintln!("[run_subtitle_pipeline] url={}, lang={}, dir={}", youtube_url, subtitle_lang, output_dir);

    log::info("run_subtitle_pipeline", &format!(
        "Starting pipeline: url={}, lang={}, dir={}",
        youtube_url, subtitle_lang, output_dir
    ));

    // AppHandleを設定
    state.pipeline_runner.set_app_handle(app_handle);

    // Arcをclone（Send可能）
    let runner = state.pipeline_runner.clone();
    let url = youtube_url.clone();
    let lang = subtitle_lang.clone();
    let dir = output_dir.clone();

    // バックグラウンドでパイプラインを実行
    tokio::spawn(async move {
        eprintln!("[run_subtitle_pipeline] Background task started");
        match runner.run_subtitle_pipeline(&url, &lang, &dir).await {
            Ok(exec) => {
                eprintln!("[run_subtitle_pipeline] Pipeline completed: {}", exec.execution_id);
                log::info("run_subtitle_pipeline", &format!(
                    "Pipeline completed: {} with status {:?}",
                    exec.execution_id, exec.status
                ));
            }
            Err(e) => {
                eprintln!("[run_subtitle_pipeline] Pipeline FAILED: {}", e);
                log::error("run_subtitle_pipeline", &format!("Pipeline failed: {}", e));
            }
        }
    });

    eprintln!("[run_subtitle_pipeline] Returning 'started'");
    Ok("started".to_string())
}

/// パイプライン実行状態を取得
#[tauri::command]
fn get_pipeline_execution(
    state: State<AppState>,
    execution_id: String,
) -> Option<PipelineExecution> {
    state.pipeline_runner.get_execution(&execution_id)
}

/// アクティブなパイプライン実行一覧を取得
#[tauri::command]
fn list_active_pipeline_executions(state: State<AppState>) -> Vec<PipelineExecution> {
    state.pipeline_runner.get_active_executions()
}

/// パイプライン実行をキャンセル
#[tauri::command]
fn cancel_pipeline_execution(
    state: State<AppState>,
    execution_id: String,
) -> Result<PipelineExecution, String> {
    state.pipeline_runner.cancel_execution(&execution_id)
        .map_err(|e| e.to_string())
}

// ============================================================================
// Ask Tool Commands (ACP v3)
// ============================================================================

/// 保留中の質問一覧を取得
#[tauri::command]
fn acp_get_pending_questions(state: State<AppState>) -> Vec<(String, ParsedQuestion)> {
    state.pipeline_runner.ask_handler().get_pending_questions()
}

/// 質問に回答する
#[tauri::command]
fn acp_submit_answer(
    state: State<AppState>,
    question_id: String,
    answer: String,
    remember_choice: bool,
) -> Result<(), String> {
    let human_answer = HumanAnswer {
        question_id,
        answer,
        remember_choice,
    };
    state.pipeline_runner.ask_handler().submit_answer(human_answer)
        .map_err(|e| e.to_string())
}

// ============================================================================
// CLI Executor Commands (v3 - stream-json based)
// ============================================================================

/// CLIエグゼキューターを起動
#[tauri::command]
async fn executor_start(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    working_dir: Option<String>,
    allowed_tools: Option<Vec<String>>,
    session_id: Option<String>,
) -> Result<String, String> {
    log::info("executor_start", "Starting CLI executor");

    let options = ExecutorOptions {
        working_dir,
        allowed_tools: allowed_tools.unwrap_or_default(),
        session_id,
        ..Default::default()
    };

    let cli_executor = state.cli_executor.clone();

    // 非同期でエグゼキューターを作成・保存
    let mut guard = cli_executor.write().await;

    let mut executor = ClaudeCodeExecutor::new(options);
    executor.set_app_handle(app_handle);

    // 起動
    executor.start().await
        .map_err(|e| format!("Failed to start executor: {}", e))?;

    let session_id = executor.session_id()
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    *guard = Some(executor);

    log::info("executor_start", &format!("CLI executor started, session: {}", session_id));
    Ok(session_id)
}

/// CLIエグゼキューターでタスクを実行
#[tauri::command]
async fn executor_execute(
    state: State<'_, AppState>,
    prompt: String,
) -> Result<String, String> {
    log::info("executor_execute", &format!("Executing task ({} chars)", prompt.len()));

    let cli_executor = state.cli_executor.clone();

    let mut guard = cli_executor.write().await;

    if let Some(ref mut executor) = *guard {
        executor.execute(&prompt).await
            .map_err(|e| format!("Execution failed: {}", e))
    } else {
        Err("Executor not started".to_string())
    }
}

/// CLIエグゼキューターを停止
#[tauri::command]
async fn executor_stop(state: State<'_, AppState>) -> Result<(), String> {
    log::info("executor_stop", "Stopping CLI executor");

    let cli_executor = state.cli_executor.clone();

    let mut guard = cli_executor.write().await;

    if let Some(ref mut executor) = *guard {
        executor.stop().await
            .map_err(|e| format!("Failed to stop executor: {}", e))?;
    }

    *guard = None;

    log::info("executor_stop", "CLI executor stopped");
    Ok(())
}

/// CLIエグゼキューターの状態を取得
#[tauri::command]
async fn executor_get_state(state: State<'_, AppState>) -> Result<AgentState, String> {
    let cli_executor = state.cli_executor.clone();
    let guard = cli_executor.read().await;

    let executor = guard.as_ref().ok_or("Executor not started")?;
    Ok(executor.current_state())
}

/// 権限要求に回答
#[tauri::command]
async fn executor_submit_permission(
    state: State<'_, AppState>,
    request_id: String,
    allow: bool,
    always: bool,
) -> Result<(), String> {
    let decision = if allow {
        PermissionDecision::Allow { always }
    } else {
        PermissionDecision::Deny {
            reason: "User denied".to_string(),
        }
    };

    let cli_executor = state.cli_executor.clone();
    let guard = cli_executor.read().await;

    if let Some(ref executor) = *guard {
        executor.submit_permission_response(&request_id, decision).await
            .map_err(|e| format!("Failed to submit permission: {}", e))?;
    }

    log::info("executor_submit_permission", &format!(
        "Permission response: request_id={}, allow={}, always={}",
        request_id, allow, always
    ));

    Ok(())
}

/// CLIエグゼキューターが起動しているか確認
#[tauri::command]
async fn executor_is_running(state: State<'_, AppState>) -> Result<bool, String> {
    let cli_executor = state.cli_executor.clone();
    let guard = cli_executor.read().await;
    Ok(guard.is_some())
}

// ============================================================================
// VOICEVOX Commands
// ============================================================================

/// VOICEVOX Engineが起動しているか確認
#[tauri::command]
fn voicevox_is_running(state: State<AppState>) -> bool {
    let client = state.voicevox_client.lock();
    client.is_running()
}

/// VOICEVOXのバージョンを取得
#[tauri::command]
fn voicevox_get_version(state: State<AppState>) -> Result<String, String> {
    let client = state.voicevox_client.lock();
    client.get_version()
        .map_err(|e| e.to_string())
}

/// VOICEVOX話者一覧を取得
#[tauri::command]
fn voicevox_get_speakers(state: State<AppState>) -> Result<Vec<Speaker>, String> {
    let client = state.voicevox_client.lock();
    client.get_speakers()
        .map_err(|e| e.to_string())
}

/// テキストから音声を合成
#[tauri::command]
fn voicevox_synthesize(
    state: State<AppState>,
    text: String,
    speaker: i32,
    output_path: String,
) -> Result<String, String> {
    let client = state.voicevox_client.lock();
    client.text_to_speech(&text, speaker, &output_path)
        .map_err(|e| e.to_string())
}

/// オプション付きでテキストから音声を合成
#[tauri::command]
fn voicevox_synthesize_with_options(
    state: State<AppState>,
    text: String,
    speaker: i32,
    speed_scale: Option<f64>,
    pitch_scale: Option<f64>,
    intonation_scale: Option<f64>,
    volume_scale: Option<f64>,
    output_path: String,
) -> Result<String, String> {
    let client = state.voicevox_client.lock();
    let options = SynthesisOptions {
        speaker,
        speed_scale: speed_scale.unwrap_or(1.0),
        pitch_scale: pitch_scale.unwrap_or(0.0),
        intonation_scale: intonation_scale.unwrap_or(1.0),
        volume_scale: volume_scale.unwrap_or(1.0),
    };
    client.text_to_speech_with_options(&text, options, &output_path)
        .map_err(|e| e.to_string())
}

// ============================================================================
// Application Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ロガーを初期化
    if let Err(e) = log::init_logger() {
        eprintln!("Failed to initialize logger: {}", e);
    }
    log::info("APP", "Application starting");

    let start_time = chrono::Local::now().format("%H:%M:%S").to_string();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(move |app| {
            // タイトルバーに起動時刻を表示
            if let Some(window) = app.get_webview_window("main") {
                let title = format!("Re-Voice [{}]", start_time);
                window.set_title(&title).ok();
            }
            Ok(())
        })
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
            check_ytdlp_available,
            youtube_download_subtitle,
            youtube_list_subs,
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
            tmux_answer_question,
            tmux_get_agent_status,
            // ACP v3 commands
            acp_define_pipeline,
            acp_execute_pipeline,
            acp_get_pipeline_status,
            acp_complete_pipeline_stage,
            acp_cancel_pipeline,
            acp_list_pipelines,
            acp_list_active_executions,
            acp_broadcast_v3,
            acp_broadcast_to_idle,
            acp_discover_agents_v3,
            acp_stats_v3,
            // Pipeline Runner commands (Phase 3)
            run_subtitle_pipeline,
            get_pipeline_execution,
            list_active_pipeline_executions,
            cancel_pipeline_execution,
            // Ask Tool commands (ACP v3)
            acp_get_pending_questions,
            acp_submit_answer,
            // CLI Executor commands (v3 - stream-json based)
            executor_start,
            executor_execute,
            executor_stop,
            executor_get_state,
            executor_submit_permission,
            executor_is_running,
            // VOICEVOX commands
            voicevox_is_running,
            voicevox_get_version,
            voicevox_get_speakers,
            voicevox_synthesize,
            voicevox_synthesize_with_options,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
