//! YouTube字幕ダウンローダー
//!
//! yt-dlpを使用してYouTube動画から字幕をダウンロードする。

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// 字幕ダウンロードエラー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum YoutubeError {
    /// yt-dlpが見つからない
    YtdlpNotFound,
    /// ダウンロード失敗
    DownloadFailed { message: String },
    /// 字幕が見つからない
    SubtitleNotFound { lang: String },
    /// ファイル保存失敗
    SaveFailed { message: String },
}

impl std::fmt::Display for YoutubeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YoutubeError::YtdlpNotFound => write!(f, "yt-dlpがインストールされていません"),
            YoutubeError::DownloadFailed { message } => write!(f, "ダウンロード失敗: {}", message),
            YoutubeError::SubtitleNotFound { lang } => write!(f, "{}の字幕が見つかりません", lang),
            YoutubeError::SaveFailed { message } => write!(f, "保存失敗: {}", message),
        }
    }
}

impl std::error::Error for YoutubeError {}

/// 字幕ダウンロード結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleDownloadResult {
    /// 保存されたファイルパス
    pub file_path: String,
    /// 動画タイトル
    pub title: String,
    /// 字幕言語
    pub lang: String,
    /// ファイルサイズ（バイト）
    pub size: u64,
}

/// YouTube字幕ダウンローダー
pub struct YoutubeDownloader {
    /// yt-dlpのパス
    ytdlp_path: String,
}

impl YoutubeDownloader {
    /// 新しいダウンローダーを作成
    pub fn new() -> Self {
        Self {
            ytdlp_path: "yt-dlp".to_string(),
        }
    }

    /// yt-dlpのパスを指定して作成
    pub fn with_path(ytdlp_path: &str) -> Self {
        Self {
            ytdlp_path: ytdlp_path.to_string(),
        }
    }

    /// yt-dlpがインストールされているか確認
    pub fn check_available(&self) -> Result<(), YoutubeError> {
        let output = Command::new(&self.ytdlp_path)
            .arg("--version")
            .output()
            .map_err(|_| YoutubeError::YtdlpNotFound)?;

        if output.status.success() {
            Ok(())
        } else {
            Err(YoutubeError::YtdlpNotFound)
        }
    }

    /// 字幕をダウンロード
    ///
    /// # Arguments
    /// * `url` - YouTube動画URL
    /// * `output_dir` - 出力ディレクトリ
    /// * `lang` - 字幕言語（en, ko, zh-CNなど）
    ///
    /// # Returns
    /// * `SubtitleDownloadResult` - ダウンロード結果
    pub fn download_subtitle(
        &self,
        url: &str,
        output_dir: &str,
        lang: &str,
    ) -> Result<SubtitleDownloadResult, YoutubeError> {
        crate::log::info("YoutubeDownloader", &format!("Downloading subtitle: {} [{}]", url, lang));

        // 出力ディレクトリを作成
        std::fs::create_dir_all(output_dir)
            .map_err(|e| YoutubeError::SaveFailed {
                message: e.to_string(),
            })?;

        // 出力テンプレート
        let output_template = format!("{}/%(title)s.{}.%(ext)s", output_dir, lang);

        // yt-dlpコマンド実行
        let output = Command::new(&self.ytdlp_path)
            .args([
                "--write-sub",
                "--write-auto-sub",  // 自動生成字幕も取得
                "--sub-lang", lang,
                "--skip-download",   // 動画はダウンロードしない
                "--sub-format", "vtt",
                "-o", &output_template,
                "--print", "%(title)s",  // タイトルを出力
                url,
            ])
            .output()
            .map_err(|e| YoutubeError::DownloadFailed {
                message: e.to_string(),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            crate::log::error("YoutubeDownloader", &format!("yt-dlp failed: {}", stderr));

            // 字幕が見つからない場合のエラーメッセージ
            if stderr.contains("Requested subtitles language") || stderr.contains("not available") {
                return Err(YoutubeError::SubtitleNotFound {
                    lang: lang.to_string(),
                });
            }

            return Err(YoutubeError::DownloadFailed {
                message: stderr.to_string(),
            });
        }

        // タイトルを取得
        let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
        crate::log::info("YoutubeDownloader", &format!("Video title: {}", title));

        // 保存されたファイルを探す
        let file_path = self.find_subtitle_file(output_dir, &title, lang)?;

        // ファイルサイズを取得
        let size = std::fs::metadata(&file_path)
            .map(|m| m.len())
            .unwrap_or(0);

        crate::log::info("YoutubeDownloader", &format!("Saved: {} ({} bytes)", file_path, size));

        Ok(SubtitleDownloadResult {
            file_path,
            title,
            lang: lang.to_string(),
            size,
        })
    }

    /// 保存された字幕ファイルを探す
    fn find_subtitle_file(
        &self,
        output_dir: &str,
        title: &str,
        lang: &str,
    ) -> Result<String, YoutubeError> {
        let dir = Path::new(output_dir);

        // ファイル名パターン: title.lang.vtt
        let expected_name = format!("{}.{}.vtt", title, lang);
        let expected_path = dir.join(&expected_name);

        if expected_path.exists() {
            return Ok(expected_path.to_string_lossy().to_string());
        }

        // ディレクトリ内の.vttファイルを探す
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "vtt" {
                        let name = path.file_name().unwrap().to_string_lossy();
                        if name.contains(lang) {
                            return Ok(path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        Err(YoutubeError::SaveFailed {
            message: format!("Subtitle file not found in {}", output_dir),
        })
    }

    /// 利用可能な字幕言語一覧を取得
    pub fn list_available_subs(&self, url: &str) -> Result<Vec<String>, YoutubeError> {
        let output = Command::new(&self.ytdlp_path)
            .args(["--list-subs", url])
            .output()
            .map_err(|e| YoutubeError::DownloadFailed {
                message: e.to_string(),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let langs: Vec<String> = stdout
            .lines()
            .filter_map(|line| {
                // "en    English" のようなフォーマットから言語コードを抽出
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[0].len() <= 5 {
                    Some(parts[0].to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(langs)
    }
}

impl Default for YoutubeDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_available() {
        let downloader = YoutubeDownloader::new();
        // yt-dlpがインストールされている場合のみ成功
        // CI環境ではスキップ
        if downloader.check_available().is_ok() {
            println!("yt-dlp is available");
        }
    }
}
