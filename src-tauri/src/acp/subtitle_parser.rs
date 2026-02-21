//! VTT字幕パーサー
//!
//! WebVTT形式の字幕ファイルをパースし、翻訳処理用のデータ構造に変換する。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// パースエラー
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Invalid VTT format: {0}")]
    InvalidFormat(String),

    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// 字幕セグメント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleSegment {
    /// セグメントインデックス
    pub index: u32,
    /// 開始時刻（ミリ秒）
    pub start_ms: u64,
    /// 終了時刻（ミリ秒）
    pub end_ms: u64,
    /// 字幕テキスト
    pub text: String,
}

impl SubtitleSegment {
    /// 新しいセグメントを作成
    pub fn new(index: u32, start_ms: u64, end_ms: u64, text: String) -> Self {
        Self {
            index,
            start_ms,
            end_ms,
            text,
        }
    }

    /// 継続時間（ミリ秒）
    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

/// VTTパーサー
pub struct VttParser;

impl VttParser {
    /// VTTコンテンツをパース
    pub fn parse(content: &str) -> Result<Vec<SubtitleSegment>, ParseError> {
        let mut segments = Vec::new();
        let mut index: u32 = 0;

        // WEBVTTヘッダーチェック
        let content = content.trim_start();
        if !content.starts_with("WEBVTT") {
            return Err(ParseError::InvalidFormat(
                "Missing WEBVTT header".to_string(),
            ));
        }

        // ヘッダー以降を処理
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        // ヘッダーをスキップ
        while i < lines.len() && !lines[i].contains("-->") {
            i += 1;
        }

        // セグメントをパース
        while i < lines.len() {
            let line = lines[i].trim();

            // タイムスタンプ行を探す
            if line.contains("-->") {
                let (start_ms, end_ms) = Self::parse_timestamp(line)?;

                // テキストを収集
                let mut text_lines = Vec::new();
                i += 1;

                while i < lines.len() {
                    let text_line = lines[i].trim();
                    if text_line.is_empty() || text_line.contains("-->") {
                        break;
                    }
                    // タグを除去
                    let clean_text = Self::strip_vtt_tags(text_line);
                    if !clean_text.is_empty() {
                        text_lines.push(clean_text);
                    }
                    i += 1;
                }

                if !text_lines.is_empty() {
                    let text = text_lines.join("\n");
                    segments.push(SubtitleSegment::new(index, start_ms, end_ms, text));
                    index += 1;
                }

                continue;
            }

            i += 1;
        }

        Ok(segments)
    }

    /// タイムスタンプをパース
    /// 形式: "00:00:00.000 --> 00:00:00.000" または "00:00.000 --> 00:00.000"
    fn parse_timestamp(line: &str) -> Result<(u64, u64), ParseError> {
        let parts: Vec<&str> = line.split("-->").collect();
        if parts.len() != 2 {
            return Err(ParseError::InvalidTimestamp(line.to_string()));
        }

        let start = Self::parse_time(parts[0].trim())?;
        let end = Self::parse_time(parts[1].split_whitespace().next().unwrap_or("0"))?;

        Ok((start, end))
    }

    /// 単一時刻をパース
    /// 形式: "HH:MM:SS.mmm" または "MM:SS.mmm"
    fn parse_time(time_str: &str) -> Result<u64, ParseError> {
        // 追加パラメータを除去（position:...など）
        let time_str = time_str.split_whitespace().next().unwrap_or("0");

        let parts: Vec<&str> = time_str.split(':').collect();

        let (hours, minutes, seconds) = match parts.len() {
            3 => {
                // HH:MM:SS.mmm
                let hours: u64 = parts[0].parse().unwrap_or(0);
                let minutes: u64 = parts[1].parse().unwrap_or(0);
                let seconds: u64 = Self::parse_seconds(parts[2])?;
                (hours, minutes, seconds)
            }
            2 => {
                // MM:SS.mmm
                let minutes: u64 = parts[0].parse().unwrap_or(0);
                let seconds: u64 = Self::parse_seconds(parts[1])?;
                (0, minutes, seconds)
            }
            1 => {
                // SS.mmm
                let seconds: u64 = Self::parse_seconds(parts[0])?;
                (0, 0, seconds)
            }
            _ => return Err(ParseError::InvalidTimestamp(time_str.to_string())),
        };

        let total_ms = hours * 3600000 + minutes * 60000 + seconds;
        Ok(total_ms)
    }

    /// 秒部分をパース（SS.mmm形式）
    fn parse_seconds(seconds_str: &str) -> Result<u64, ParseError> {
        let parts: Vec<&str> = seconds_str.split('.').collect();

        let seconds: u64 = parts.get(0).unwrap_or(&"0").parse().unwrap_or(0);
        let millis: u64 = if parts.len() > 1 {
            let ms_str = parts[1];
            // 3桁にパディング
            let padded = format!("{:0<3}", &ms_str.chars().take(3).collect::<String>());
            padded.parse().unwrap_or(0)
        } else {
            0
        };

        Ok(seconds * 1000 + millis)
    }

    /// VTTタグを除去
    fn strip_vtt_tags(text: &str) -> String {
        let mut result = text.to_string();
        // <b>, </b>, <i>, </i>, <u>, </u>, <c.color>, etc.
        let tag_patterns = [
            (r"</?b>", ""),
            (r"</?i>", ""),
            (r"</?u>", ""),
            (r"</?c[^>]*>", ""),
            (r"<\d+:\d+:\d+\.?\d*>", ""), // タイミングタグ
            (r"</?\w+>", ""),              // その他のタグ
        ];

        for (pattern, replacement) in tag_patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                result = re.replace_all(&result, replacement).to_string();
            }
        }

        // 文字参照をデコード
        result = result.replace("&nbsp;", " ");
        result = result.replace("&amp;", "&");
        result = result.replace("&lt;", "<");
        result = result.replace("&gt;", ">");

        result.trim().to_string()
    }

    /// セグメントを翻訳用テキストに変換
    /// 各セグメントをインデックス付きでリスト化
    pub fn to_translation_text(segments: &[SubtitleSegment]) -> String {
        segments
            .iter()
            .map(|s| format!("[{}] {}", s.index, s.text))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// 翻訳済みテキストからVTTを再構築
    /// translated_texts: 各セグメントの翻訳テキスト
    pub fn rebuild_vtt(original: &[SubtitleSegment], translated: &[String]) -> String {
        let mut vtt = String::new();
        vtt.push_str("WEBVTT\n\n");

        for (i, segment) in original.iter().enumerate() {
            let translated_text = translated.get(i).unwrap_or(&segment.text);

            // タイムスタンプ
            let start_time = Self::format_time(segment.start_ms);
            let end_time = Self::format_time(segment.end_ms);
            vtt.push_str(&format!("{} --> {}\n", start_time, end_time));

            // テキスト
            vtt.push_str(translated_text);
            vtt.push_str("\n\n");
        }

        vtt
    }

    /// ミリ秒をVTT時刻形式に変換
    fn format_time(ms: u64) -> String {
        let hours = ms / 3600000;
        let minutes = (ms % 3600000) / 60000;
        let seconds = (ms % 60000) / 1000;
        let millis = ms % 1000;

        format!(
            "{:02}:{:02}:{:02}.{:03}",
            hours, minutes, seconds, millis
        )
    }

    /// VTTファイルを読み込んでパース
    pub fn parse_file(path: &str) -> Result<Vec<SubtitleSegment>, ParseError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// セグメントからテキストのみを抽出（翻訳用）
    pub fn extract_texts(segments: &[SubtitleSegment]) -> Vec<String> {
        segments.iter().map(|s| s.text.clone()).collect()
    }

    /// テキストリストをセグメントに適用（翻訳結果を反映）
    pub fn apply_translations(
        original: &[SubtitleSegment],
        translated_texts: &[String],
    ) -> Vec<SubtitleSegment> {
        original
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let text = translated_texts.get(i).cloned().unwrap_or_else(|| s.text.clone());
                SubtitleSegment::new(s.index, s.start_ms, s.end_ms, text)
            })
            .collect()
    }
}

/// 翻訳テキストをパースして各セグメントに分割
/// 形式: "[0] テキスト\n\n[1] テキスト..."
pub fn parse_translated_text(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\[(\d+)\]\s*").unwrap();
    let mut translations = Vec::new();
    let mut current_text = String::new();

    for line in text.lines() {
        if let Some(_) = re.captures(line) {
            // 新しいセグメントの開始
            if !current_text.is_empty() {
                translations.push(current_text.trim().to_string());
                current_text = String::new();
            }
            // インデックスを除去してテキストを追加
            current_text.push_str(re.replace(line, "").trim());
            current_text.push(' ');
        } else if !line.trim().is_empty() {
            current_text.push_str(line.trim());
            current_text.push(' ');
        }
    }

    // 最後のセグメント
    if !current_text.is_empty() {
        translations.push(current_text.trim().to_string());
    }

    translations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_vtt() {
        let vtt = r#"WEBVTT

00:00:01.000 --> 00:00:04.000
Hello, world!

00:00:05.000 --> 00:00:08.000
This is a test.
"#;

        let segments = VttParser::parse(vtt).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello, world!");
        assert_eq!(segments[0].start_ms, 1000);
        assert_eq!(segments[0].end_ms, 4000);
        assert_eq!(segments[1].text, "This is a test.");
    }

    #[test]
    fn test_parse_timestamp() {
        let (start, end) = VttParser::parse_timestamp("00:01:30.500 --> 00:02:45.250").unwrap();
        assert_eq!(start, 90500);
        assert_eq!(end, 165250);
    }

    #[test]
    fn test_parse_time_short() {
        let ms = VttParser::parse_time("01:30.500").unwrap();
        assert_eq!(ms, 90500);
    }

    #[test]
    fn test_format_time() {
        let time = VttParser::format_time(90500);
        assert_eq!(time, "00:01:30.500");
    }

    #[test]
    fn test_to_translation_text() {
        let segments = vec![
            SubtitleSegment::new(0, 0, 1000, "Hello".to_string()),
            SubtitleSegment::new(1, 1000, 2000, "World".to_string()),
        ];

        let text = VttParser::to_translation_text(&segments);
        assert!(text.contains("[0] Hello"));
        assert!(text.contains("[1] World"));
    }

    #[test]
    fn test_rebuild_vtt() {
        let segments = vec![
            SubtitleSegment::new(0, 0, 1000, "Hello".to_string()),
        ];
        let translated = vec!["こんにちは".to_string()];

        let vtt = VttParser::rebuild_vtt(&segments, &translated);
        assert!(vtt.starts_with("WEBVTT"));
        assert!(vtt.contains("00:00:00.000 --> 00:00:01.000"));
        assert!(vtt.contains("こんにちは"));
    }

    #[test]
    fn test_strip_vtt_tags() {
        let text = "<b>Hello</b> <i>world</i>!";
        let clean = VttParser::strip_vtt_tags(text);
        assert_eq!(clean, "Hello world!");
    }

    #[test]
    fn test_parse_translated_text() {
        let text = "[0] こんにちは\n\n[1] 世界";
        let translations = parse_translated_text(text);
        assert_eq!(translations.len(), 2);
        assert_eq!(translations[0], "こんにちは");
        assert_eq!(translations[1], "世界");
    }
}
