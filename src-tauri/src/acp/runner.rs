//! Pipeline Runner - ACP v3自動実行エンジン（CLIベース版）
//!
//! ClaudeCodeExecutor（--print --output-format stream-json）を使用して
//! パイプラインを自動的に実行する。
//!
//! ## 責任分担
//! - **Rust**: 字幕ダウンロード、VTT解析、音声生成（確実で高速）
//! - **Claude Code**: 翻訳（LLMが必要）
//!
//! ## 4ステージパイプライン
//! 1. Stage1: 字幕DL (yt-dlp/Rust)
//! 2. Stage2: VTT解析 (Rust)
//! 3. Stage3: 翻訳 (Claude Code)
//! 4. Stage4: 音声生成 (VOICEVOX/Rust)

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use thiserror::Error;
use tokio::sync::RwLock;

use super::ask::AskToolHandler;
use super::executor::{ClaudeCodeExecutor, ExecutorOptions};
use super::pipeline::{PipelineDefinition, PipelineError, PipelineExecution, PipelineExecutor};
use super::message::PipelineStage;
use super::subtitle_parser::{VttParser, SubtitleSegment, parse_translated_text};
use crate::log;
use crate::youtube::YoutubeDownloader;
use crate::voicevox::VoicevoxClient;

/// UTF-8安全な文字列切り詰め
fn truncate_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

/// PipelineRunnerエラー
#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("Pipeline error: {0}")]
    Pipeline(#[from] PipelineError),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Timeout waiting for agent: {0}")]
    Timeout(String),

    #[error("Stage failed: {0}")]
    StageFailed(String),

    #[error("Execution not found: {0}")]
    ExecutionNotFound(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YouTube download error: {0}")]
    Youtube(String),

    #[error("VTT parse error: {0}")]
    VttParse(String),

    #[error("VOICEVOX error: {0}")]
    Voicevox(String),

    #[error("Claude Code executor error: {0}")]
    Executor(String),

    #[error("Executor not available")]
    ExecutorNotAvailable,
}

/// 実行コンテキスト（ステージ間で共有）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// パイプラインID
    pub pipeline_id: String,
    /// 実行ID
    pub execution_id: String,
    /// 現在のステージインデックス
    pub current_stage: usize,
    /// 各ステージの出力（意味のあるコンテンツのみ）
    pub stage_outputs: HashMap<String, String>,
    /// 抽出されたファイルパス
    pub extracted_files: HashMap<String, Vec<String>>,
    /// 入力データ
    pub input: Value,
}

impl ExecutionContext {
    pub fn new(pipeline_id: &str, execution_id: &str, input: Value) -> Self {
        Self {
            pipeline_id: pipeline_id.to_string(),
            execution_id: execution_id.to_string(),
            current_stage: 0,
            stage_outputs: HashMap::new(),
            extracted_files: HashMap::new(),
            input,
        }
    }
}

/// 進捗イベントのペイロード
#[derive(Debug, Clone, Serialize)]
pub struct ProgressPayload {
    pub execution_id: String,
    pub stage_index: usize,
    pub stage_name: String,
    pub status: String,
    pub progress_percent: u8,
    pub message: String,
}

/// PipelineRunner - パイプライン自動実行エンジン（CLIベース版）
///
/// 注: CLIエグゼキューターはlib.rs側で管理され、このrunnerは
/// パイプライン定義と進捗管理のみを担当する。
#[derive(Clone)]
pub struct PipelineRunner {
    /// パイプライン実行管理
    executor: Arc<Mutex<PipelineExecutor>>,
    /// CLI-based Claude Code executor
    cli_executor: Arc<RwLock<Option<ClaudeCodeExecutor>>>,
    /// Ask Tool ハンドラー
    ask_handler: Arc<AskToolHandler>,
    /// アプリハンドル（イベント送信用）
    app_handle: Arc<Mutex<Option<AppHandle>>>,
    /// 実行コンテキスト
    contexts: Arc<Mutex<HashMap<String, ExecutionContext>>>,
}

impl PipelineRunner {
    /// 新しいPipelineRunnerを作成
    pub fn new(
        executor: Arc<Mutex<PipelineExecutor>>,
        _tmux: Arc<Mutex<Option<super::tmux::TmuxOrchestrator>>>,
    ) -> Self {
        Self {
            executor,
            cli_executor: Arc::new(RwLock::new(None)),
            ask_handler: Arc::new(AskToolHandler::new()),
            app_handle: Arc::new(Mutex::new(None)),
            contexts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// CLIエグゼキューターを指定して作成
    pub fn with_cli_executor(
        executor: Arc<Mutex<PipelineExecutor>>,
        cli_executor: Arc<RwLock<Option<ClaudeCodeExecutor>>>,
    ) -> Self {
        Self {
            executor,
            cli_executor,
            ask_handler: Arc::new(AskToolHandler::new()),
            app_handle: Arc::new(Mutex::new(None)),
            contexts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// CLIエグゼキューターを設定
    pub fn set_cli_executor(&self, executor: Arc<RwLock<Option<ClaudeCodeExecutor>>>) {
        // 実際にはArcをcloneできないので、このメソッドは使用しない
        // 代わりにwith_cli_executorを使用する
        let _ = executor;
    }

    /// AppHandleを設定
    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock() = Some(handle.clone());
        self.ask_handler.set_app_handle(handle);
    }

    /// 字幕翻訳パイプラインを実行
    ///
    /// ## 実行フロー
    /// 1. **Rustで字幕ダウンロード** (yt-dlp)
    /// 2. **Claude Codeで翻訳** (CLIベース)
    /// 3. **Rustで音声生成** (VOICEVOX)
    pub async fn run_subtitle_pipeline(
        &self,
        youtube_url: &str,
        subtitle_lang: &str,
        output_dir: &str,
    ) -> Result<PipelineExecution, RunnerError> {
        log::info("PipelineRunner", &format!(
            "Starting subtitle pipeline: url={}, lang={}, output={}",
            youtube_url, subtitle_lang, output_dir
        ));

        // パイプライン定義を作成
        let pipeline = self.create_subtitle_pipeline(youtube_url, subtitle_lang, output_dir)?;

        // パイプラインを登録
        let pipeline_id = {
            let executor = self.executor.lock();
            executor.register(pipeline)
        };

        // 入力データ
        let input = serde_json::json!({
            "youtube_url": youtube_url,
            "subtitle_lang": subtitle_lang,
            "output_dir": output_dir,
        });

        // 実行開始
        self.run(&pipeline_id, input).await
    }

    /// 字幕翻訳パイプラインの定義を作成（4ステージ版）
    fn create_subtitle_pipeline(
        &self,
        youtube_url: &str,
        subtitle_lang: &str,
        output_dir: &str,
    ) -> Result<PipelineDefinition, RunnerError> {
        use super::message::AgentAddress;

        let mut pipeline = PipelineDefinition::new("subtitle-translation");

        // ステージ1: 字幕ダウンロード（Rust/yt-dlp）
        let download_stage = PipelineStage::new(
            "download-subtitles",
            AgentAddress::new("rust-direct"),
        )
        .with_prompt_template(format!(
            "RUST_DIRECT:{}",
            serde_json::json!({
                "url": youtube_url,
                "lang": subtitle_lang,
                "output_dir": output_dir,
                "stage": "download"
            }).to_string()
        ));

        // ステージ2: VTT解析（Rust）
        let parse_stage = PipelineStage::new(
            "parse-subtitles",
            AgentAddress::new("rust-direct"),
        )
        .with_prompt_template(format!(
            "RUST_DIRECT:{}",
            serde_json::json!({
                "stage": "parse",
                "output_dir": output_dir
            }).to_string()
        ));

        // ステージ3: 翻訳（Claude Code）
        let translate_stage = PipelineStage::new(
            "translate-subtitles",
            AgentAddress::new("claude-code"),
        )
        .with_prompt_template(
            r#"以下の字幕テキストを日本語に翻訳してください。
翻訳結果のみを出力してください。各セグメントの番号を維持してください。

{{parse-subtitles}}

【翻訳ルール】
1. 自然な日本語に翻訳
2. 短すぎず長すぎない、適切な長さに
3. 番号付きフォーマットを維持: [0] テキスト

翻訳結果:""#,
        );

        // ステージ4: 音声生成（Rust/VOICEVOX）
        let voice_stage = PipelineStage::new(
            "voice-synthesis",
            AgentAddress::new("rust-direct"),
        )
        .with_prompt_template(format!(
            "RUST_DIRECT:{}",
            serde_json::json!({
                "stage": "voicevox",
                "output_dir": output_dir,
                "speaker": 1
            }).to_string()
        ));

        pipeline = pipeline
            .add_stage(download_stage)
            .add_stage(parse_stage)
            .add_stage(translate_stage)
            .add_stage(voice_stage);

        Ok(pipeline)
    }

    /// パイプラインを実行
    pub async fn run(
        &self,
        pipeline_id: &str,
        input: Value,
    ) -> Result<PipelineExecution, RunnerError> {
        log::info("PipelineRunner", &format!("Running pipeline: {}", pipeline_id));

        // 実行開始
        let execution = {
            let executor = self.executor.lock();
            executor.start_execution(pipeline_id)?
        };

        let execution_id = execution.execution_id.clone();

        // コンテキスト作成
        let context = ExecutionContext::new(pipeline_id, &execution_id, input.clone());
        {
            let mut ctx = self.contexts.lock();
            ctx.insert(execution_id.clone(), context);
        }

        // 進捗イベントを送信
        self.emit_progress(&execution_id, 0, "pipeline-started", "パイプライン開始");

        // パイプライン定義を取得
        let pipeline = {
            let executor = self.executor.lock();
            executor.get_pipeline(pipeline_id)
                .ok_or_else(|| RunnerError::ExecutionNotFound(pipeline_id.to_string()))?
        };

        // 各ステージを実行
        for (stage_index, stage) in pipeline.stages.iter().enumerate() {
            log::info("PipelineRunner", &format!(
                "Executing stage {}: {}",
                stage_index, stage.name
            ));

            self.emit_progress(
                &execution_id,
                stage_index,
                "stage-started",
                &format!("ステージ開始: {}", stage.name),
            );

            // ステージを実行
            match self.execute_stage(&execution_id, stage, stage_index).await {
                Ok(output) => {
                    // 出力をコンテキストに保存
                    {
                        let mut ctx = self.contexts.lock();
                        if let Some(c) = ctx.get_mut(&execution_id) {
                            c.stage_outputs.insert(stage.name.clone(), output.clone());
                        }
                    }

                    // ステージ完了
                    {
                        let executor = self.executor.lock();
                        executor.complete_stage(&execution_id, serde_json::json!({ "output": output }))?;
                    }

                    self.emit_progress(
                        &execution_id,
                        stage_index,
                        "stage-completed",
                        &format!("ステージ完了: {}", stage.name),
                    );
                }
                Err(e) => {
                    log::error("PipelineRunner", &format!("Stage {} failed: {}", stage_index, e));

                    // ステージ失敗
                    {
                        let executor = self.executor.lock();
                        executor.fail_stage(&execution_id, e.to_string())?;
                    }

                    self.emit_progress(
                        &execution_id,
                        stage_index,
                        "stage-failed",
                        &format!("ステージ失敗: {} - {}", stage.name, e),
                    );

                    return Err(e);
                }
            }
        }

        // 最終的な実行状態を取得
        let final_execution = {
            let executor = self.executor.lock();
            executor.get_execution(&execution_id)
                .ok_or_else(|| RunnerError::ExecutionNotFound(execution_id.clone()))?
        };

        self.emit_progress(&execution_id, pipeline.stages.len() - 1, "pipeline-completed", "パイプライン完了");

        log::info("PipelineRunner", &format!(
            "Pipeline completed: {} with status {:?}",
            execution_id, final_execution.status
        ));

        Ok(final_execution)
    }

    /// 単一ステージを実行
    ///
    /// 実行モード:
    /// - RUST_DIRECT: Rust直接実行（字幕DL、VTT解析、音声生成）
    /// - その他: Claude Code実行（翻訳）
    async fn execute_stage(
        &self,
        execution_id: &str,
        stage: &PipelineStage,
        stage_index: usize,
    ) -> Result<String, RunnerError> {
        log::info("PipelineRunner", &format!("Starting stage {} ({})", stage_index, stage.name));

        // Rust直接実行チェック
        if let Some(ref template) = stage.prompt_template {
            if template.starts_with("RUST_DIRECT:") {
                return self.execute_rust_direct(template, execution_id).await;
            }
        }

        // Claude Code実行
        self.execute_claude_code(execution_id, stage, stage_index).await
    }

    /// Rust直接実行（字幕ダウンロード、VTT解析、音声生成）
    async fn execute_rust_direct(
        &self,
        template: &str,
        execution_id: &str,
    ) -> Result<String, RunnerError> {
        let json_str = template.strip_prefix("RUST_DIRECT:")
            .ok_or_else(|| RunnerError::StageFailed("Invalid RUST_DIRECT format".to_string()))?;

        let params: Value = serde_json::from_str(json_str)
            .map_err(|e| RunnerError::StageFailed(format!("Invalid JSON in RUST_DIRECT: {}", e)))?;

        let stage = params["stage"].as_str().unwrap_or("");

        match stage {
            "download" => {
                self.execute_download_stage(&params).await
            }
            "parse" => {
                self.execute_parse_stage(execution_id, &params).await
            }
            "voicevox" => {
                self.execute_voicevox_stage(execution_id, &params).await
            }
            _ => {
                Err(RunnerError::StageFailed(format!("Unknown RUST_DIRECT stage: {}", stage)))
            }
        }
    }

    /// Stage1: 字幕ダウンロード
    async fn execute_download_stage(&self, params: &Value) -> Result<String, RunnerError> {
        let url = params["url"].as_str()
            .ok_or_else(|| RunnerError::StageFailed("Missing url".to_string()))?;
        let lang = params["lang"].as_str()
            .ok_or_else(|| RunnerError::StageFailed("Missing lang".to_string()))?;
        let output_dir = params["output_dir"].as_str()
            .ok_or_else(|| RunnerError::StageFailed("Missing output_dir".to_string()))?;

        log::info("PipelineRunner", &format!("Stage1: Downloading subtitle from {} [{}]", url, lang));

        let url_owned = url.to_string();
        let lang_owned = lang.to_string();
        let output_dir_owned = output_dir.to_string();

        let result = tokio::task::spawn_blocking(move || {
            let downloader = YoutubeDownloader::new();
            downloader.download_subtitle(&url_owned, &output_dir_owned, &lang_owned)
        }).await.map_err(|e| RunnerError::Youtube(e.to_string()))?;

        match result {
            Ok(download_result) => {
                log::info("PipelineRunner", &format!(
                    "Stage1 complete: {} ({} bytes)",
                    download_result.file_path, download_result.size
                ));
                Ok(download_result.file_path)
            }
            Err(e) => {
                Err(RunnerError::Youtube(e.to_string()))
            }
        }
    }

    /// Stage2: VTT解析
    async fn execute_parse_stage(
        &self,
        execution_id: &str,
        params: &Value,
    ) -> Result<String, RunnerError> {
        let output_dir = params["output_dir"].as_str()
            .ok_or_else(|| RunnerError::StageFailed("Missing output_dir".to_string()))?;

        // 前のステージから字幕ファイルパスを取得
        let vtt_path = {
            let ctx = self.contexts.lock();
            let c = ctx.get(execution_id)
                .ok_or_else(|| RunnerError::ExecutionNotFound(execution_id.to_string()))?;
            c.stage_outputs.get("download-subtitles")
                .cloned()
                .ok_or_else(|| RunnerError::StageFailed("No subtitle file from stage1".to_string()))?
        };

        log::info("PipelineRunner", &format!("Stage2: Parsing VTT file: {}", vtt_path));

        // VTTをパース
        let segments = VttParser::parse_file(&vtt_path)
            .map_err(|e| RunnerError::VttParse(e.to_string()))?;

        log::info("PipelineRunner", &format!("Stage2: Parsed {} segments", segments.len()));

        // 翻訳用テキストを生成
        let translation_text = VttParser::to_translation_text(&segments);

        // セグメント情報をJSONとして保存（後で使用）
        let segments_json = serde_json::to_string(&segments)
            .map_err(|e| RunnerError::Json(e))?;

        let segments_path = format!("{}/segments.json", output_dir);
        std::fs::write(&segments_path, &segments_json)
            .map_err(|e| RunnerError::Io(e))?;

        log::info("PipelineRunner", &format!("Stage2 complete: saved segments to {}", segments_path));

        Ok(translation_text)
    }

    /// Stage4: 音声生成（VOICEVOX）
    async fn execute_voicevox_stage(
        &self,
        execution_id: &str,
        params: &Value,
    ) -> Result<String, RunnerError> {
        let output_dir = params["output_dir"].as_str()
            .ok_or_else(|| RunnerError::StageFailed("Missing output_dir".to_string()))?;
        let speaker = params["speaker"].as_i64().unwrap_or(1) as i32;

        // 前のステージから翻訳テキストを取得
        let translated_text = {
            let ctx = self.contexts.lock();
            let c = ctx.get(execution_id)
                .ok_or_else(|| RunnerError::ExecutionNotFound(execution_id.to_string()))?;
            c.stage_outputs.get("translate-subtitles")
                .cloned()
                .ok_or_else(|| RunnerError::StageFailed("No translated text from stage3".to_string()))?
        };

        log::info("PipelineRunner", &format!("Stage4: Synthesizing audio with VOICEVOX (speaker={})", speaker));

        // 翻訳テキストをパース
        let translations = parse_translated_text(&translated_text);
        log::info("PipelineRunner", &format!("Stage4: Parsed {} translation segments", translations.len()));

        // セグメント情報を読み込み
        let segments_path = format!("{}/segments.json", output_dir);
        let segments_json = std::fs::read_to_string(&segments_path)
            .map_err(|e| RunnerError::Io(e))?;
        let original_segments: Vec<SubtitleSegment> = serde_json::from_str(&segments_json)
            .map_err(|e| RunnerError::Json(e))?;

        // 翻訳済みVTTを生成
        let translated_vtt = VttParser::rebuild_vtt(&original_segments, &translations);
        let vtt_path = format!("{}/translated.ja.vtt", output_dir);
        std::fs::write(&vtt_path, &translated_vtt)
            .map_err(|e| RunnerError::Io(e))?;

        // 音声生成ディレクトリ
        let audio_dir = format!("{}/audio", output_dir);
        std::fs::create_dir_all(&audio_dir)
            .map_err(|e| RunnerError::Io(e))?;

        // VOICEVOXで音声生成
        let client = VoicevoxClient::new();
        if !client.is_running() {
            log::warn("PipelineRunner", "VOICEVOX Engine not running, skipping audio synthesis");
            return Ok(format!("Translated VTT saved to {} (VOICEVOX not running)", vtt_path));
        }

        let mut audio_files = Vec::new();
        for (i, text) in translations.iter().enumerate() {
            if text.trim().is_empty() {
                continue;
            }
            let audio_path = format!("{}/audio_{:04}.wav", audio_dir, i);
            match client.text_to_speech(text, speaker, &audio_path) {
                Ok(path) => {
                    audio_files.push(path);
                    log::info("PipelineRunner", &format!("Generated: {}", audio_path));
                }
                Err(e) => {
                    log::error("PipelineRunner", &format!("VOICEVOX error for segment {}: {}", i, e));
                }
            }
        }

        log::info("PipelineRunner", &format!(
            "Stage4 complete: {} audio files generated",
            audio_files.len()
        ));

        Ok(format!(
            "Generated {} audio files in {}",
            audio_files.len(),
            audio_dir
        ))
    }

    /// Claude Code実行（翻訳ステージ）
    async fn execute_claude_code(
        &self,
        execution_id: &str,
        stage: &PipelineStage,
        stage_index: usize,
    ) -> Result<String, RunnerError> {
        // プロンプトを構築
        let prompt = {
            let ctx = self.contexts.lock();
            let c = ctx.get(execution_id)
                .ok_or_else(|| RunnerError::ExecutionNotFound(execution_id.to_string()))?;

            self.build_prompt(stage, &c.stage_outputs, &c.extracted_files, &c.input)
        };

        log::info("PipelineRunner", &format!(
            "Stage {} (Claude Code): {} chars prompt",
            stage_index, prompt.len()
        ));

        // CLIエグゼキューターを使用
        let cli_executor = self.cli_executor.clone();
        let prompt_owned = prompt.clone();

        // 非同期で実行
        let result = async move {
            let mut guard = cli_executor.write().await;

            if let Some(ref mut executor) = *guard {
                executor.execute(&prompt_owned).await
                    .map_err(|e| RunnerError::Executor(e.to_string()))
            } else {
                Err(RunnerError::ExecutorNotAvailable)
            }
        }.await;

        match result {
            Ok(output) => {
                log::info("PipelineRunner", &format!(
                    "Stage {} complete: {} chars output",
                    stage_index, output.len()
                ));
                Ok(output)
            }
            Err(e) => {
                log::error("PipelineRunner", &format!("Claude Code execution failed: {}", e));

                // エグゼキューターが利用できない場合はフォールバック
                if matches!(e, RunnerError::ExecutorNotAvailable) {
                    log::warn("PipelineRunner", "Using fallback mode - returning prompt for manual execution");
                    Ok(format!("[FALLBACK - Manual execution required]\n\n{}", prompt))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// プロンプトを構築
    fn build_prompt(
        &self,
        stage: &PipelineStage,
        stage_outputs: &HashMap<String, String>,
        extracted_files: &HashMap<String, Vec<String>>,
        input: &Value,
    ) -> String {
        if let Some(ref template) = stage.prompt_template {
            let mut result = template.clone();

            // 前段階の出力を置換
            for (stage_name, output) in stage_outputs {
                let placeholder = format!("{{{{{}}}}}", stage_name);
                result = result.replace(&placeholder, output);
            }

            // 抽出されたファイルパスを置換
            for (stage_name, files) in extracted_files {
                if !files.is_empty() {
                    let placeholder = format!("{{{{{}}}}}", stage_name);
                    result = result.replace(&placeholder, &files[0]);
                }
            }

            // 入力データを置換
            if let Some(obj) = input.as_object() {
                for (key, value) in obj {
                    let placeholder = format!("{{{{{}}}}}", key);
                    if let Some(s) = value.as_str() {
                        result = result.replace(&placeholder, s);
                    } else {
                        result = result.replace(&placeholder, &value.to_string());
                    }
                }
            }
            result = result.replace("{{input}}", &serde_json::to_string(input).unwrap_or_default());

            result
        } else {
            format!(
                "Context: {:?}\n\nInput: {:?}\n\nExecute stage: {}",
                stage_outputs, input, stage.name
            )
        }
    }

    /// 進捗イベントを送信
    fn emit_progress(
        &self,
        execution_id: &str,
        stage_index: usize,
        status: &str,
        message: &str,
    ) {
        let handle = self.app_handle.lock();
        if let Some(ref h) = *handle {
            let stage_name = {
                let executor = self.executor.lock();
                executor.get_execution(execution_id)
                    .and_then(|e| e.stage_results.get(stage_index).map(|s| s.stage_name.clone()))
                    .unwrap_or_default()
            };

            let progress_percent = {
                let executor = self.executor.lock();
                executor.get_execution(execution_id)
                    .map(|e| e.progress())
                    .unwrap_or(0)
            };

            let payload = ProgressPayload {
                execution_id: execution_id.to_string(),
                stage_index,
                stage_name,
                status: status.to_string(),
                progress_percent,
                message: message.to_string(),
            };

            if let Err(e) = h.emit("pipeline:progress", &payload) {
                log::error("PipelineRunner", &format!("Failed to emit progress: {:?}", e));
            }
        }
    }

    /// 実行状態を取得
    pub fn get_execution(&self, execution_id: &str) -> Option<PipelineExecution> {
        let executor = self.executor.lock();
        executor.get_execution(execution_id)
    }

    /// アクティブな実行一覧を取得
    pub fn get_active_executions(&self) -> Vec<PipelineExecution> {
        let executor = self.executor.lock();
        executor.get_active_executions()
    }

    /// 実行をキャンセル
    pub fn cancel_execution(&self, execution_id: &str) -> Result<PipelineExecution, RunnerError> {
        let executor = self.executor.lock();
        let execution = executor.cancel_execution(execution_id)?;

        self.emit_progress(execution_id, execution.current_stage, "cancelled", "パイプラインキャンセル");

        Ok(execution)
    }

    /// AskToolHandlerを取得
    pub fn ask_handler(&self) -> &AskToolHandler {
        &self.ask_handler
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_context() {
        let ctx = ExecutionContext::new(
            "pipeline-1",
            "exec-1",
            serde_json::json!({ "test": "value" }),
        );

        assert_eq!(ctx.pipeline_id, "pipeline-1");
        assert_eq!(ctx.execution_id, "exec-1");
        assert!(ctx.stage_outputs.is_empty());
    }

    #[test]
    fn test_progress_payload() {
        let payload = ProgressPayload {
            execution_id: "exec-1".to_string(),
            stage_index: 0,
            stage_name: "test-stage".to_string(),
            status: "running".to_string(),
            progress_percent: 50,
            message: "Test message".to_string(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("exec-1"));
        assert!(json.contains("test-stage"));
    }

    #[test]
    fn test_truncate_safe() {
        let s = "日本語テスト";
        let truncated = truncate_safe(s, 10);
        assert!(truncated.len() <= 10);
        assert!(s.starts_with(truncated));
    }
}
