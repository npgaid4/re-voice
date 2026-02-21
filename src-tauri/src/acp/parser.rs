//! Claude Code 出力パーサー（状態遷移ベース版）
//!
//! 送信前後の画面変化を検出して状態を判定する。

use regex::Regex;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use super::tmux::AgentStatus;

/// 画面のハッシュ値を計算
pub fn content_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// 出力パーサー（状態遷移ベース）
pub struct OutputParser {
    /// マーカー検出用正規表現
    done_marker: Regex,
    waiting_marker: Regex,
    ask_marker: Regex,
    error_marker: Regex,
    file_marker: Regex,
    /// Claude Codeの処理中表示パターン
    tool_execution: Regex,
    spinner_pattern: Regex,
    thinking_pattern: Regex,
}

impl OutputParser {
    pub fn new() -> Self {
        Self {
            // マーカー
            done_marker: Regex::new(r"@DONE@").unwrap(),
            waiting_marker: Regex::new(r"@WAITING@").unwrap(),
            ask_marker: Regex::new(r"@ASK@").unwrap(),
            error_marker: Regex::new(r"@ERROR@").unwrap(),
            file_marker: Regex::new(r"@FILE:([^@]+)@").unwrap(),
            // Claude Codeの処理中表示
            tool_execution: Regex::new(r"⏺\s*(Bash|Read|Write|Edit|Grep|Glob|Task)").unwrap(),
            spinner_pattern: Regex::new(r"[✢✳✶✻✷✸✹✺·⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]").unwrap(),
            thinking_pattern: Regex::new(r"(?i)(Thinking|Processing|Working|Generating)[.。…]*").unwrap(),
        }
    }

    /// Claude Codeの権限プロンプト（AskTool）を検出
    fn is_permission_prompt(&self, content: &str) -> bool {
        // Claude Codeの権限プロンプトの特徴的なパターン
        // - "Do you want to proceed?"
        // - "❯ 1. Yes" (選択肢の先頭)
        // - "Esc to cancel" (操作ヒント)
        let has_proceed = content.contains("Do you want to proceed") ||
                          content.contains("requires approval");
        let has_option = content.contains("❯ 1.") ||
                         Regex::new(r"^\s*❯\s*1\.\s*Yes").unwrap().is_match(content);
        let has_hint = content.contains("Esc to cancel") ||
                       content.contains("Tab to amend");

        // パターン1: "Do you want to proceed?" + "❯ 1. Yes"
        // パターン2: "requires approval" + "❯ 1. Yes"
        // パターン3: "Esc to cancel" + "❯ 1. Yes"
        (has_proceed && has_option) || (has_hint && has_option)
    }

    /// 画面変化を検出して状態を判定
    ///
    /// # Arguments
    /// * `current_content` - 現在の画面内容
    /// * `previous_hash` - 送信前の画面ハッシュ（None=初回または送信前記録なし）
    ///
    /// # Returns
    /// * (AgentStatus, content_hash) - 状態と現在のハッシュ
    ///
    /// ## 判定ロジック
    /// - @DONE@ がある → Idle
    /// - @WAITING@/@ASK@ がある → WaitingForInput
    /// - @ERROR@ がある → Error
    /// - ツール実行中(⏺) → Processing
    /// - スピナー/Thinking → Processing
    /// - それ以外 → Processing（マーカーがない限り完了とみなさない）
    pub fn parse_with_change_detection(
        &self,
        current_content: &str,
        previous_hash: Option<u64>,
    ) -> (AgentStatus, u64) {
        let current_hash = content_hash(current_content);
        let content_trimmed = current_content.trim();

        // 空の場合はUnknown
        if content_trimmed.is_empty() {
            return (AgentStatus::Unknown, current_hash);
        }

        // 1. マーカーベース判定（最優先）

        // AskTool（権限プロンプト）検出 - @DONE@より優先
        if self.is_permission_prompt(content_trimmed) {
            let question = content_trimmed.to_string();
            return (AgentStatus::WaitingForInput { question }, current_hash);
        }

        // エラーマーカー
        if self.error_marker.is_match(content_trimmed) {
            let error_msg = self.extract_error_message(content_trimmed);
            return (AgentStatus::Error { message: error_msg }, current_hash);
        }

        // 入力待ちマーカー
        if self.waiting_marker.is_match(content_trimmed) || self.ask_marker.is_match(content_trimmed) {
            let question = self.extract_question(content_trimmed);
            return (AgentStatus::WaitingForInput { question }, current_hash);
        }

        // 完了マーカー
        if self.done_marker.is_match(content_trimmed) {
            return (AgentStatus::Idle, current_hash);
        }

        // 2. 処理中の判定

        // ツール実行中表示
        if self.tool_execution.is_match(content_trimmed) {
            return (AgentStatus::Processing, current_hash);
        }

        // スピナー/Thinking表示
        if self.spinner_pattern.is_match(content_trimmed) || self.thinking_pattern.is_match(content_trimmed) {
            return (AgentStatus::Processing, current_hash);
        }

        // 3. @DONE@がない限り、Processingとみなす
        // （以前はプロンプトがあればIdleとしていたが、これは誤判定の原因だった）
        (AgentStatus::Processing, current_hash)
    }

    /// 従来のパースメソッド（後方互換用）
    pub fn parse(&self, content: &str) -> AgentStatus {
        let (status, _) = self.parse_with_change_detection(content, None);
        status
    }

    /// ウェルカム画面かどうか
    fn is_welcome_screen(&self, content: &str) -> bool {
        content.contains("Claude Code")
            && content.contains("❯")
            && (content.contains("for shortcuts")
                || content.contains("Try \"")
                || content.contains("model to try"))
    }

    /// まだ処理中かどうか（プロンプトがある場合の追加チェック）
    fn is_still_processing(&self, content: &str) -> bool {
        self.tool_execution.is_match(content)
            || self.spinner_pattern.is_match(content)
            || self.thinking_pattern.is_match(content)
    }

    /// ファイルパスを抽出
    pub fn extract_files(&self, content: &str) -> Vec<String> {
        self.file_marker
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// エラーメッセージを抽出
    fn extract_error_message(&self, content: &str) -> String {
        if let Some(pos) = content.find("@ERROR@") {
            let after = &content[pos + 7..];
            after
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string()
        } else {
            "Unknown error".to_string()
        }
    }

    /// 質問内容を抽出
    fn extract_question(&self, content: &str) -> String {
        let marker_pos = content.find("@WAITING@")
            .or_else(|| content.find("@ASK@"));

        if let Some(pos) = marker_pos {
            let before = &content[..pos];
            before
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string()
        } else {
            String::new()
        }
    }

    /// 意味のあるコンテンツを抽出（マーカー除去版）
    pub fn extract_meaningful_content(&self, content: &str) -> String {
        let stripped = Self::strip_ansi(content);

        // 最後の❯より前の内容を取得
        let lines: Vec<&str> = stripped.lines().collect();
        let mut last_prompt_idx = None;
        for (i, line) in lines.iter().enumerate().rev() {
            if line.trim().starts_with("❯") || line.trim().starts_with('>') {
                last_prompt_idx = Some(i);
                break;
            }
        }

        let content_lines: Vec<&str> = if let Some(idx) = last_prompt_idx {
            stripped.lines().take(idx).collect()
        } else {
            stripped.lines().collect()
        };

        // マーカーを除去してクリーンなテキストを返す
        let clean_text = content_lines
            .iter()
            .map(|line| {
                let line = self.done_marker.replace_all(line, "");
                let line = self.waiting_marker.replace_all(&line, "");
                let line = self.ask_marker.replace_all(&line, "");
                let line = self.error_marker.replace_all(&line, "");
                let line = self.file_marker.replace_all(&line, "");
                line.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");

        // 複数の空行を1つに
        let collapsed = Regex::new(r"\n{3,}")
            .unwrap()
            .replace_all(&clean_text, "\n\n");

        collapsed.trim().to_string()
    }

    /// ANSIエスケープシーケンスを除去
    pub fn strip_ansi(content: &str) -> String {
        let ansi_regex = Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap();
        ansi_regex.replace_all(content, "").to_string()
    }
}

impl Default for OutputParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_different() {
        let hash1 = content_hash("content 1");
        let hash2 = content_hash("content 2");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_content_hash_same() {
        let hash1 = content_hash("same content");
        let hash2 = content_hash("same content");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_no_change_returns_processing() {
        let parser = OutputParser::new();
        let content = "Some content\n❯ ";
        let hash = content_hash(content);

        // 同じハッシュで呼び出すとProcessing（変化なし）
        let (status, _) = parser.parse_with_change_detection(content, Some(hash));
        assert_eq!(status, AgentStatus::Processing);
    }

    #[test]
    fn test_change_with_done_marker_returns_idle() {
        let parser = OutputParser::new();
        let old_hash = content_hash("old content");

        let content = "Task done\n@DONE@\n❯ ";
        let (status, _) = parser.parse_with_change_detection(content, Some(old_hash));
        assert_eq!(status, AgentStatus::Idle);
    }

    #[test]
    fn test_change_with_tool_execution_returns_processing() {
        let parser = OutputParser::new();
        let old_hash = content_hash("old content");

        let content = "⏺ Bash(some command)\nRunning...";
        let (status, _) = parser.parse_with_change_detection(content, Some(old_hash));
        assert_eq!(status, AgentStatus::Processing);
    }

    #[test]
    fn test_welcome_screen_without_marker_returns_processing() {
        // 新しいロジック: @DONE@がない限りProcessing
        let parser = OutputParser::new();
        let content = r#"Claude Code v2.1.50
❯ Try "how do I log an error?"
  ? for shortcuts"#;
        let hash = content_hash(content);

        // ウェルカム画面でも@DONE@がなければProcessing
        let (status, _) = parser.parse_with_change_detection(content, Some(hash));
        assert_eq!(status, AgentStatus::Processing);
    }

    #[test]
    fn test_extract_files() {
        let parser = OutputParser::new();
        let content = "Saved\n@FILE:/tmp/test.vtt@\n@DONE@";
        let files = parser.extract_files(content);
        assert_eq!(files, vec!["/tmp/test.vtt"]);
    }

    #[test]
    fn test_permission_prompt_detection() {
        let parser = OutputParser::new();

        // Claude Codeの権限プロンプト
        let content = r#"mkdir -p /tmp/revoice
   Create directory

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and always allow access to tmp/
   3. No

 Esc to cancel · Tab to amend"#;

        let old_hash = content_hash("old content");
        let (status, _) = parser.parse_with_change_detection(content, Some(old_hash));

        match status {
            AgentStatus::WaitingForInput { .. } => {},
            _ => panic!("Expected WaitingForInput, got {:?}", status),
        }
    }

    #[test]
    fn test_permission_prompt_with_requires_approval() {
        let parser = OutputParser::new();

        // "requires approval"パターン
        let content = r#"   Execute bash command

 This command requires approval

 Do you want to proceed?
 ❯ 1. Yes
   2. No"#;

        assert!(parser.is_permission_prompt(content));
    }

    #[test]
    fn test_permission_prompt_not_detected_for_normal_output() {
        let parser = OutputParser::new();

        // 通常の出力
        let content = "Processing your request...\n⏺ Bash(ls -la)\nDone.";

        assert!(!parser.is_permission_prompt(content));
    }
}
