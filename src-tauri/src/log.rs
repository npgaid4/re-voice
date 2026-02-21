//! ログユーティリティ
//!
//! ログをファイルに出力し、デバッグしやすくする。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Local;

/// ロガー
pub struct Logger {
    log_dir: PathBuf,
    current_log: PathBuf,
    file: Option<Mutex<File>>,
}

impl Logger {
    /// ロガーを作成
    pub fn new() -> Self {
        let log_dir = PathBuf::from("logs");
        let current_log = log_dir.join("current.log");

        Self {
            log_dir,
            current_log,
            file: None,
        }
    }

    /// ログを初期化
    pub fn init(&mut self) -> std::io::Result<()> {
        // logsディレクトリを作成
        fs::create_dir_all(&self.log_dir)?;

        // 古いログをアーカイブ
        self.archive_old_log()?;

        // 新しいログファイルを作成
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.current_log)?;

        self.file = Some(Mutex::new(file));

        // 起動ログ
        self.log("LOGGER", "Log initialized");

        Ok(())
    }

    /// 古いログをアーカイブ
    fn archive_old_log(&self) -> std::io::Result<()> {
        if !self.current_log.exists() {
            return Ok(());
        }

        // archiveディレクトリを作成
        let archive_dir = self.log_dir.join("archive");
        fs::create_dir_all(&archive_dir)?;

        // タイムスタンプ付きファイル名
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let archived_name = format!("{}.log", timestamp);
        let archived_path = archive_dir.join(archived_name);

        // 移動
        fs::rename(&self.current_log, &archived_path)?;

        // 古いアーカイブを削除（7日以上前）
        self.cleanup_old_archives(&archive_dir)?;

        Ok(())
    }

    /// 古いアーカイブを削除
    fn cleanup_old_archives(&self, archive_dir: &PathBuf) -> std::io::Result<()> {
        let now = std::time::SystemTime::now();
        let seven_days = std::time::Duration::from_secs(7 * 24 * 60 * 60);

        if let Ok(entries) = fs::read_dir(archive_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = now.duration_since(modified) {
                            if elapsed > seven_days {
                                let _ = fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// ログを出力
    pub fn log(&self, tag: &str, message: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let log_line = format!("[{}] [{}] {}\n", timestamp, tag, message);

        // 標準エラー出力にも出力
        eprint!("{}", log_line);

        // ファイルに出力
        if let Some(ref file_mutex) = self.file {
            if let Ok(mut file) = file_mutex.lock() {
                let _ = file.write_all(log_line.as_bytes());
                let _ = file.flush();
            }
        }
    }

    /// デバッグログ
    pub fn debug(&self, tag: &str, message: &str) {
        self.log(&format!("DEBUG/{}", tag), message);
    }

    /// 情報ログ
    pub fn info(&self, tag: &str, message: &str) {
        self.log(&format!("INFO/{}", tag), message);
    }

    /// エラーログ
    pub fn error(&self, tag: &str, message: &str) {
        self.log(&format!("ERROR/{}", tag), message);
    }

    /// 警告ログ
    pub fn warn(&self, tag: &str, message: &str) {
        self.log(&format!("WARN/{}", tag), message);
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

// グローバルロガー
lazy_static::lazy_static! {
    static ref GLOBAL_LOGGER: Mutex<Logger> = Mutex::new(Logger::new());
}

/// グローバルロガーを初期化
pub fn init_logger() -> std::io::Result<()> {
    let mut logger = GLOBAL_LOGGER.lock().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    })?;
    logger.init()
}

/// ログを出力
pub fn log(tag: &str, message: &str) {
    if let Ok(logger) = GLOBAL_LOGGER.lock() {
        logger.log(tag, message);
    }
}

/// デバッグログ
pub fn debug(tag: &str, message: &str) {
    if let Ok(logger) = GLOBAL_LOGGER.lock() {
        logger.debug(tag, message);
    }
}

/// 情報ログ
pub fn info(tag: &str, message: &str) {
    if let Ok(logger) = GLOBAL_LOGGER.lock() {
        logger.info(tag, message);
    }
}

/// エラーログ
pub fn error(tag: &str, message: &str) {
    if let Ok(logger) = GLOBAL_LOGGER.lock() {
        logger.error(tag, message);
    }
}

/// 警告ログ
pub fn warn(tag: &str, message: &str) {
    if let Ok(logger) = GLOBAL_LOGGER.lock() {
        logger.warn(tag, message);
    }
}
