use anyhow::{anyhow, Result};
use chrono;
use parking_lot::Mutex;
use portable_pty::{native_pty_system, Child, CommandBuilder, PtyPair, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// PTYイベント
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum PtyEvent {
    /// 出力チャンク
    Output(String),
    /// プロンプト検知（入力待ち状態）
    Prompt,
    /// エラー
    Error(String),
    /// ユーザー入力が必要
    InputRequired {
        /// 検出されたプロンプトタイプ
        prompt_type: PromptType,
        /// 直近のコンテキスト（ユーザーに表示用）
        context: String,
    },
}

/// PTYマネージャー - Claude Code等のCLIツールとの通信を管理
/// イベント駆動で動作し、バックグラウンドスレッドで出力を読み取る
pub struct PtyManager {
    pair: Option<PtyPair>,
    #[allow(dead_code)]
    child: Option<Box<dyn Child + Send + Sync>>,
    reader: Arc<Mutex<Option<Box<dyn Read + Send>>>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    /// バックグラウンドリーダーのハンドル
    reader_handle: Option<JoinHandle<()>>,
    /// リーダー停止フラグ
    stop_flag: Arc<AtomicBool>,
    /// 出力バッファ
    output_buffer: Arc<Mutex<String>>,
    /// 最後のプロンプト以降の出力（レスポンス用）
    response_buffer: Arc<Mutex<String>>,
    /// イベントコールバック
    event_callback: Arc<Mutex<Option<Box<dyn Fn(PtyEvent) + Send>>>>,
    /// 子プロセスPID
    child_pid: Option<u32>,
    /// 最後のアクティビティ時刻（タイムアウト検出用）
    last_activity: Arc<Mutex<std::time::Instant>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            pair: None,
            child: None,
            reader: Arc::new(Mutex::new(None)),
            writer: Arc::new(Mutex::new(None)),
            reader_handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
            output_buffer: Arc::new(Mutex::new(String::new())),
            response_buffer: Arc::new(Mutex::new(String::new())),
            event_callback: Arc::new(Mutex::new(None)),
            child_pid: None,
            last_activity: Arc::new(Mutex::new(std::time::Instant::now())),
        }
    }

    /// イベントコールバックを設定
    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(PtyEvent) + Send + 'static,
    {
        *self.event_callback.lock() = Some(Box::new(callback));
    }

    /// Claude CodeをPTYで起動
    pub fn spawn_claude_code(&mut self) -> Result<()> {
        let pty_system = native_pty_system();

        // 120x50の仮想端末を作成（スクロールバッファ拡大）
        let pair = pty_system
            .openpty(PtySize {
                rows: 50,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow!("Failed to create PTY: {}", e))?;

        // Claude Codeを起動（通常モード）
        // PromptDetectorが確認プロンプトに自動応答する
        let cmd = CommandBuilder::new("claude");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| anyhow!("Failed to spawn claude code: {}", e))?;

        let pid = child.process_id();
        eprintln!("[PTY] Child process spawned, PID: {:?}", pid);
        self.child_pid = pid;

        // リーダーとライターを保存
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| anyhow!("Failed to clone reader: {}", e))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| anyhow!("Failed to take writer: {}", e))?;

        *self.reader.lock() = Some(reader);
        *self.writer.lock() = Some(writer);
        self.pair = Some(pair);

        // 子プロセスハンドルを保存（プロセスを維持するため）
        // PtyChild + Send は portable-pty でサポートされている
        self.child = Some(child);

        // バックグラウンドリーダーを開始
        self.start_background_reader();

        Ok(())
    }

    /// バックグラウンドリーダーを開始
    fn start_background_reader(&mut self) {
        self.stop_flag.store(false, Ordering::SeqCst);

        let reader = Arc::clone(&self.reader);
        let writer = Arc::clone(&self.writer);
        let stop_flag = Arc::clone(&self.stop_flag);
        let output_buffer = Arc::clone(&self.output_buffer);
        let response_buffer = Arc::clone(&self.response_buffer);
        let event_callback = Arc::clone(&self.event_callback);

        let handle = thread::spawn(move || {
            let mut buffer = [0u8; 4096];

            fn log(msg: &str) {
                let now = chrono::Local::now();
                eprintln!("[{}] {}", now.format("%H:%M:%S%.3f"), msg);
            }

            log("[PTY READER] Background reader started");

            while !stop_flag.load(Ordering::SeqCst) {
                log("[PTY READER] Waiting for data...");
                let mut reader_lock = reader.lock();

                if let Some(ref mut r) = *reader_lock {
                    log("[PTY READER] Calling read()...");
                    match r.read(&mut buffer) {
                        Ok(0) => {
                            // EOF - プロセスが終了
                            drop(reader_lock);
                            log("[PTY READER] EOF received");
                            if let Some(cb) = event_callback.lock().as_ref() {
                                cb(PtyEvent::Error("PTY EOF - process terminated".to_string()));
                            }
                            break;
                        }
                        Ok(n) => {
                            log(&format!("[PTY READER] Read {} bytes", n));
                            log(&format!("[PTY READER] Raw bytes: {:?}", &buffer[..n]));
                            drop(reader_lock); // ロックを解放

                            // ANSIエスケープシーケンスを処理
                            let clean_chunk = process_ansi(&buffer[..n]);
                            log(&format!("[PTY READER] After process_ansi: {} bytes", clean_chunk.len()));

                            // 出力バッファに追加
                            let current_output = {
                                let mut buf = output_buffer.lock();
                                buf.push_str(&clean_chunk);

                                // バッファサイズ制限（最新100KB）
                                if buf.len() > 100_000 {
                                    let drain = buf.len() - 100_000;
                                    buf.drain(0..drain);
                                }
                                buf.clone()
                            };

                            // プロンプト検知（PromptDetector使用）
                            if let Some(prompt_type) = PromptDetector::detect(&current_output) {
                                log(&format!("[PTY READER] Prompt detected: {:?}", prompt_type));

                                // 自動応答可能かチェック
                                if let Some(response) = PromptDetector::get_auto_response(&prompt_type) {
                                    log(&format!("[PTY READER] Auto-response would be: {:?}", response));

                                    // 自動応答を送信
                                    thread::sleep(std::time::Duration::from_millis(500));

                                    log("[PTY READER] Acquiring writer for auto-response...");
                                    if let Some(ref mut w) = *writer.lock() {
                                        // 選択肢番号だけを送信（Enterなし）
                                        let choice = response.trim();
                                        log(&format!("[PTY READER] Writing choice: {:?}", choice.as_bytes()));
                                        let _ = w.write_all(choice.as_bytes());
                                        let _ = w.flush();
                                        log("[PTY READER] Choice written, waiting...");

                                        // 少し待ってからEnterを送信
                                        thread::sleep(std::time::Duration::from_millis(300));

                                        log("[PTY READER] Writing Enter...");
                                        let _ = w.write_all(b"\r");
                                        let _ = w.flush();
                                        log("[PTY READER] Auto-response completed");
                                    }

                                    // 自動応答したので出力バッファをクリア（プロンプトを除外）
                                    output_buffer.lock().clear();
                                    response_buffer.lock().clear();

                                    // 自動応答したプロンプトはイベント発火しない
                                    continue;
                                } else if matches!(prompt_type, PromptType::InputReady) {
                                    // 通常の入力待ち - ユーザーに通知
                                    response_buffer.lock().clear();
                                    if let Some(cb) = event_callback.lock().as_ref() {
                                        cb(PtyEvent::Prompt);
                                    }
                                    continue;
                                } else if matches!(prompt_type, PromptType::PendingPrompt) {
                                    // プロンプト検出中 - 選択肢待ち
                                    // イベント発火せず、次のチャンクを待つ
                                    log("[PTY READER] Pending prompt detected, waiting for choices...");
                                    continue;
                                } else if matches!(prompt_type, PromptType::AuthenticationRequired { .. })
                                    || matches!(prompt_type, PromptType::UserInputRequired { .. })
                                {
                                    // ユーザー入力が必要 - フロントエンドに通知
                                    log("[PTY READER] User input required, notifying frontend...");
                                    if let Some(cb) = event_callback.lock().as_ref() {
                                        cb(PtyEvent::InputRequired {
                                            prompt_type,
                                            context: current_output.clone(),
                                        });
                                    }
                                    // 出力バッファはクリアしない（コンテキスト保持）
                                    continue;
                                }
                            }

                            // 自動応答不要の場合のみイベント発火
                            {
                                let mut resp = response_buffer.lock();
                                resp.push_str(&clean_chunk);
                            }

                            if let Some(cb) = event_callback.lock().as_ref() {
                                cb(PtyEvent::Output(clean_chunk));
                            }
                        }
                        Err(e) => {
                            drop(reader_lock);
                            if e.kind() != std::io::ErrorKind::WouldBlock {
                                log(&format!("[PTY READER] Error: {}", e));
                                // エラー通知
                                if let Some(cb) = event_callback.lock().as_ref() {
                                    cb(PtyEvent::Error(e.to_string()));
                                }
                            }
                            // 少し待機してリトライ
                            thread::sleep(std::time::Duration::from_millis(10));
                        }
                    }
                } else {
                    drop(reader_lock);
                    break;
                }
            }
            log("[PTY READER] Background reader stopped");
        });

        self.reader_handle = Some(handle);
    }

    /// バックグラウンドリーダーを停止
    pub fn stop_background_reader(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);

        if let Some(handle) = self.reader_handle.take() {
            // スレッドの終了を待機（最大1秒）
            // 注: PTYのreadがブロックしている場合、すぐには終了しない可能性がある
            let _ = handle.join();
        }
    }

    /// 入力を送信
    pub fn write_input(&self, data: &[u8]) -> Result<()> {
        let now = chrono::Local::now();
        eprintln!("[{}] [write_input] Acquiring writer lock...", now.format("%H:%M:%S%.3f"));

        let mut writer = self.writer.lock();

        let now = chrono::Local::now();
        eprintln!("[{}] [write_input] Lock acquired, writing {} bytes...", now.format("%H:%M:%S%.3f"), data.len());

        if let Some(ref mut w) = *writer {
            w.write_all(data)
                .map_err(|e| anyhow!("Failed to write to PTY: {}", e))?;

            let now = chrono::Local::now();
            eprintln!("[{}] [write_input] Flushing...", now.format("%H:%M:%S%.3f"));

            w.flush()
                .map_err(|e| anyhow!("Failed to flush PTY: {}", e))?;

            let now = chrono::Local::now();
            eprintln!("[{}] [write_input] Done", now.format("%H:%M:%S%.3f"));
            Ok(())
        } else {
            Err(anyhow!("PTY not initialized"))
        }
    }

    /// メッセージを送信（改行付き）
    pub fn send_message(&self, message: &str) -> Result<()> {
        let now = chrono::Local::now();
        eprintln!("[{}] [PTY] send_message called: {} bytes", now.format("%H:%M:%S%.3f"), message.len());

        // レスポンスバッファをクリア
        self.response_buffer.lock().clear();

        // メッセージ本体を送信（改行なし）
        self.write_input(message.as_bytes())?;

        // 少し待機してからEnterを送信（自動応答と同じパターン）
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.write_input(b"\r")?;

        let now = chrono::Local::now();
        eprintln!("[{}] [PTY] send_message completed", now.format("%H:%M:%S%.3f"));
        Ok(())
    }

    /// 現在の出力バッファを取得
    pub fn get_output(&self) -> String {
        self.output_buffer.lock().clone()
    }

    /// 現在のレスポンス（最後のメッセージ送信以降の出力）を取得
    pub fn get_response(&self) -> String {
        self.response_buffer.lock().clone()
    }

    /// 出力バッファをクリア
    pub fn clear_output(&self) {
        self.output_buffer.lock().clear();
    }

    /// 画面出力を読み取り（レガシー - バッファから読み取る）
    pub fn read_output(&self, buffer: &mut [u8]) -> Result<usize> {
        let output = self.output_buffer.lock();
        let bytes = output.as_bytes();

        if bytes.is_empty() {
            Ok(0)
        } else {
            let len = std::cmp::min(buffer.len(), bytes.len());
            buffer[..len].copy_from_slice(&bytes[..len]);
            Ok(len)
        }
    }

    /// PTYが起動しているか確認
    pub fn is_running(&self) -> bool {
        self.pair.is_some()
    }

    /// 子プロセスが生きているか確認
    pub fn is_child_alive(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(None) => {
                    // プロセスはまだ実行中
                    true
                }
                Ok(Some(status)) => {
                    eprintln!("[PTY] Child process exited with status: {:?}", status);
                    false
                }
                Err(e) => {
                    eprintln!("[PTY] Error checking child status: {}", e);
                    false
                }
            }
        } else {
            false
        }
    }

    /// 子プロセスのPIDを取得
    pub fn child_pid(&self) -> Option<u32> {
        self.child_pid
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        self.stop_background_reader();
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// プロンプト検出・自動応答システム
// ============================================================================

/// 検出されたプロンプトの種類
#[derive(Debug, Clone, serde::Serialize)]
pub enum PromptType {
    /// ユーザー入力待ち（通常のプロンプト）
    InputReady,
    /// 選択肢付きプロンプト（自動応答可能）
    Choice { options: Vec<ChoiceOption> },
    /// 確認プロンプト（Yes/No等）
    Confirmation { message: String, auto_accept: bool },
    /// プロンプト検出中（選択肢待ち、イベント発火せず待機）
    PendingPrompt,
    /// 認証が必要（/login が必要）
    AuthenticationRequired { message: String },
    /// ユーザー入力が必要（質問など）
    UserInputRequired { message: String, prompt_text: String },
}

/// 選択肢
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChoiceOption {
    pub number: u8,
    pub label: String,
}

/// プロンプト検出器
pub struct PromptDetector;

impl PromptDetector {
    /// 出力を解析してプロンプトタイプを判定
    pub fn detect(output: &str) -> Option<PromptType> {
        let output_lower = output.to_lowercase();

        // 1. 認証エラー検出（最優先）
        if output_lower.contains("oauth token has expired")
            || output_lower.contains("authentication_error")
            || output_lower.contains("please run /login")
            || output_lower.contains("api error: 401")
        {
            eprintln!("[PromptDetector] Authentication required detected");
            return Some(PromptType::AuthenticationRequired {
                message: "Claude Codeの認証が必要です。/login を実行してください。".to_string(),
            });
        }

        // 2. Bypass Permissions 確認プロンプト
        if output_lower.contains("bypass permissions mode")
            || output_lower.contains("dangerously-skip-permissions")
        {
            let options = Self::extract_choices(output);
            eprintln!("[PromptDetector] Bypass permissions detected, options: {:?}", options);
            if !options.is_empty() {
                return Some(PromptType::Choice { options });
            }
            return Some(PromptType::PendingPrompt);
        }

        // 3. Trust verification プロンプト
        if output_lower.contains("trust this folder")
            || output_lower.contains("is this a project you created")
            || output_lower.contains("quick safety check")
        {
            let options = Self::extract_choices(output);
            eprintln!("[PromptDetector] Trust verification detected, options: {:?}", options);
            if !options.is_empty() {
                return Some(PromptType::Choice { options });
            }
            return Some(PromptType::PendingPrompt);
        }

        // 4. ユーザーへの質問検出（選択肢付き）
        // "Which option" や番号付き選択肢がある場合
        let options = Self::extract_choices(output);
        if !options.is_empty() && Self::is_input_prompt(output) {
            eprintln!("[PromptDetector] User choice required, options: {:?}", options);
            return Some(PromptType::UserInputRequired {
                message: "選択肢を選んでください。".to_string(),
                prompt_text: Self::extract_last_lines(output, 5),
            });
        }

        // 5. 通常の入力プロンプト（応答完了）
        if Self::is_input_prompt(output) {
            return Some(PromptType::InputReady);
        }

        None
    }

    /// 最後のN行を抽出（コンテキスト表示用）
    fn extract_last_lines(output: &str, n: usize) -> String {
        let lines: Vec<&str> = output.lines().collect();
        let start = lines.len().saturating_sub(n);
        lines[start..].join("\n")
    }

    /// 選択肢を抽出
    fn extract_choices(output: &str) -> Vec<ChoiceOption> {
        let mut options = Vec::new();

        for line in output.lines() {
            // "❯1.No,exit 2.Yes,Iaccept" のようなパターンを処理
            // 行全体から選択肢を抽出

            // "数字.ラベル" または "数字,ラベル" のパターンを検索
            let mut chars = line.chars().peekable();
            let mut i = 0;
            let line_str = line;

            while i < line_str.len() {
                // 数字を探す
                let remaining = &line_str[i..];
                if let Some(pos) = remaining.find(|c: char| c.is_ascii_digit()) {
                    let after_digit = &remaining[pos..];

                    // 連続する数字を取得
                    let num_end = after_digit
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(after_digit.len());
                    let num_str = &after_digit[..num_end];

                    if let Ok(number) = num_str.parse::<u8>() {
                        // 数字の後の区切り文字（. または ,）を探す
                        let after_num = &after_digit[num_end..];
                        let after_sep = after_num.trim_start_matches(|c| c == '.' || c == ',');

                        // 次の数字または行末までがラベル
                        let label_end = after_sep
                            .find(|c: char| c.is_ascii_digit())
                            .unwrap_or(after_sep.len());
                        let label = after_sep[..label_end].trim();

                        if !label.is_empty() && label.len() > 1 {
                            options.push(ChoiceOption {
                                number,
                                label: label.to_string(),
                            });
                        }

                        i += pos + num_end + after_num.len() - after_sep.len() + label_end;
                        continue;
                    }
                    i += pos + 1;
                } else {
                    break;
                }
            }
        }

        options
    }

    /// 自動応答すべきか判定し、応答内容を返す
    pub fn get_auto_response(prompt_type: &PromptType) -> Option<String> {
        match prompt_type {
            PromptType::Choice { options } => {
                // 自動選択すべき選択肢を探す
                for opt in options {
                    let label_lower = opt.label.to_lowercase();
                    // "Yes, I accept" パターン
                    if label_lower.contains("yes") && label_lower.contains("accept") {
                        return Some(format!("{}\n", opt.number));
                    }
                    // "Yes, I trust this folder" パターン
                    if label_lower.contains("yes") && label_lower.contains("trust") {
                        return Some(format!("{}\n", opt.number));
                    }
                    // proceed / continue パターン
                    if label_lower.contains("proceed") || label_lower.contains("continue") {
                        return Some(format!("{}\n", opt.number));
                    }
                }
                // デフォルト: 最初の選択肢（通常は "Yes"）
                if !options.is_empty() {
                    eprintln!("[PromptDetector] Using default option: {}", options[0].number);
                    return Some(format!("{}\n", options[0].number));
                }
                None
            }
            PromptType::Confirmation { auto_accept: true, .. } => {
                Some("1\n".to_string()) // 通常 "1" が "Yes"
            }
            _ => None,
        }
    }

    /// 通常の入力プロンプトかどうか
    fn is_input_prompt(output: &str) -> bool {
        let prompt_patterns = ["❯ ", "> "];

        for pattern in &prompt_patterns {
            if output.ends_with(pattern) {
                return true;
            }
        }

        if let Some(last_line) = output.lines().last() {
            let trimmed = last_line.trim();
            for pattern in &prompt_patterns {
                if trimmed == *pattern || trimmed.ends_with(pattern) {
                    return true;
                }
            }
        }

        false
    }
}

// ============================================================================
// ヘルパー関数（PtyManagerのメソッドから独立させ、スレッド内で使用可能に）
// ============================================================================

/// ANSIエスケープシーケンスを処理してプレーンテキストに変換
///
/// 処理内容:
/// - カーソル前方移動 (ESC[nC) → n個のスペースに変換
/// - 色・スタイル設定 (ESC[...m) → 削除
/// - その他の制御シーケンス → 削除
fn process_ansi(bytes: &[u8]) -> String {
    let input = String::from_utf8_lossy(bytes);
    let mut result = String::with_capacity(bytes.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC シーケンス開始
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next(); // consume '['

                    // CSI シーケンス: ESC [ params letter
                    let mut params = String::new();
                    let mut command_char = '\0';

                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch.is_ascii_alphabetic() || ch == '~' {
                            command_char = ch;
                            break;
                        } else {
                            params.push(ch);
                        }
                    }

                    // コマンドに応じた処理
                    match command_char {
                        'C' | 'a' => {
                            // カーソル前方移動 (CUF): ESC[nC
                            // n の分だけスペースを追加（デフォルト1）
                            let n: usize = if params.is_empty() {
                                1
                            } else {
                                params.split(';').next().unwrap_or("1").parse().unwrap_or(1)
                            };
                            for _ in 0..n {
                                result.push(' ');
                            }
                        }
                        'm' => {
                            // SGR (色・スタイル) - 無視
                        }
                        _ => {
                            // その他のCSIコマンド - 無視
                        }
                    }
                    continue;
                } else if next == ']' {
                    chars.next(); // consume ']'
                    // OSC シーケンス: ESC ] ... BEL/ST
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch == '\x07' || ch == '\x1b' {
                            if ch == '\x1b' {
                                // ST: ESC \
                                if let Some(&'\\') = chars.peek() {
                                    chars.next();
                                }
                            }
                            break;
                        }
                    }
                    continue;
                } else if next == '(' || next == ')' {
                    // 文字セット指定: ESC ( X
                    chars.next(); // consume '(' or ')'
                    if let Some(&_) = chars.peek() {
                        chars.next(); // consume character set designator
                    }
                    continue;
                }
            }
        } else if c == '\r' {
            // CR をスキップ
            continue;
        } else {
            result.push(c);
        }
    }

    result
}
