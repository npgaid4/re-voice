mod pty;

use parking_lot::Mutex;
use pty::PtyManager;
use std::sync::Arc;
use tauri::State;

/// アプリケーション状態
pub struct AppState {
    pty: Arc<Mutex<PtyManager>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            pty: Arc::new(Mutex::new(PtyManager::new())),
        }
    }
}

// Tauriコマンド

/// Claude Codeを起動
#[tauri::command]
fn spawn_claude(state: State<AppState>) -> Result<String, String> {
    let mut pty = state.pty.lock();
    pty.spawn_claude_code().map_err(|e| e.to_string())?;
    Ok("Claude Code started".to_string())
}

/// Claude Codeにメッセージを送信
#[tauri::command]
fn send_to_claude(state: State<AppState>, message: String) -> Result<(), String> {
    let pty = state.pty.lock();
    pty.send_message(&message).map_err(|e| e.to_string())
}

/// Claude Codeから出力を読み取り
#[tauri::command]
fn read_from_claude(state: State<AppState>) -> Result<String, String> {
    let pty = state.pty.lock();
    let mut buffer = [0u8; 4096];
    match pty.read_output(&mut buffer) {
        Ok(n) if n > 0 => {
            // UTF-8としてデコード
            String::from_utf8(buffer[..n].to_vec()).map_err(|e| e.to_string())
        }
        Ok(_) => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}

/// Claude Codeが起動しているか確認
#[tauri::command]
fn is_claude_running(state: State<AppState>) -> bool {
    let pty = state.pty.lock();
    pty.is_running()
}

/// テスト用: 汎用コマンドを実行
#[tauri::command]
fn execute_command(state: State<AppState>, command: String) -> Result<String, String> {
    let pty = state.pty.lock();

    // まだPTYが起動していない場合はbashを起動
    if !pty.is_running() {
        // 一時的にbashでテスト
        drop(pty);

        // PATHを設定（Homebrewのパスを含める）
        let path = std::env::var("PATH").unwrap_or_default();
        let extended_path = format!(
            "/opt/homebrew/bin:/usr/local/bin:{}",
            path
        );

        // 簡易的なコマンド実行（テスト用）
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

/// 字幕情報を取得
#[tauri::command]
fn get_available_subtitles(url: String) -> Result<String, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let extended_path = format!("/opt/homebrew/bin:/usr/local/bin:{}", path);

    // --list-subs で利用可能な字幕一覧を取得
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

    // 字幕をダウンロード（vtt形式）
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

    // 自動生成字幕をダウンロード
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            spawn_claude,
            send_to_claude,
            read_from_claude,
            is_claude_running,
            execute_command,
            get_available_subtitles,
            download_subtitles,
            download_auto_subtitles,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
