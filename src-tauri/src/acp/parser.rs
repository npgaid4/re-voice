//! Claude Code 出力パーサー
//!
//! tmuxキャプチャ出力からエージェントの状態を正確に検出する。

use regex::Regex;
use super::tmux::AgentStatus;

/// 出力パーサー
pub struct OutputParser {
    /// プロンプトパターン（入力待ち状態を示す）
    prompt_patterns: Vec<Regex>,
    /// 処理中パターン
    processing_patterns: Vec<Regex>,
    /// 質問パターン（最後の行が質問か判定）
    question_patterns: Vec<Regex>,
    /// エラーパターン
    error_patterns: Vec<Regex>,
    /// スピナーアニメーションパターン
    spinner_patterns: Vec<Regex>,
}

impl OutputParser {
    pub fn new() -> Self {
        Self {
            prompt_patterns: vec![
                // Claude Code プロンプト（❯ が含まれている行）
                Regex::new(r"❯").unwrap(),
                // 一般的なプロンプト（> で終わる行）
                Regex::new(r">\s*$").unwrap(),
                // 追加入力待ち（継続行）
                Regex::new(r"\.\.\.\s*$").unwrap(),
            ],
            processing_patterns: vec![
                // Thinking中
                Regex::new(r"(?i)Thinking[.。…]*").unwrap(),
                // Processing中
                Regex::new(r"(?i)Processing[.。…]*").unwrap(),
                // Working中
                Regex::new(r"(?i)Working[.。…]*").unwrap(),
                // 読み込み中
                Regex::new(r"(?i)Loading[.。…]*").unwrap(),
                // 実行中
                Regex::new(r"(?i)Executing[.。…]*").unwrap(),
                // 生成中
                Regex::new(r"(?i)Generating[.。…]*").unwrap(),
                // 日本語
                Regex::new(r"処理中[.。…]*").unwrap(),
                Regex::new(r"思考中[.。…]*").unwrap(),
                Regex::new(r"生成中[.。…]*").unwrap(),
            ],
            question_patterns: vec![
                // 疑問符で終わる
                Regex::new(r"[？?]\s*$").unwrap(),
                // 日本語の疑問語
                Regex::new(r"[ど何いかがれの][れのがを]?[？?]?").unwrap(),
                // 英語の疑問詞
                Regex::new(r"(?i)(which|what|how|where|when|who|why|should|would|could|can)\s+.*\??\s*$").unwrap(),
                // 選択肢を提示
                Regex::new(r"\d+\.\s+.+\n\d+\.\s+.+").unwrap(),
                // 確認を求める
                Regex::new(r"(?i)(continue|proceed|confirm|yes|no)\??\s*$").unwrap(),
                // 日本語で確認
                Regex::new(r"(よろしい|よろしければ|続けます|進めます|確認)[かですて]?[？?]?\s*$").unwrap(),
            ],
            error_patterns: vec![
                Regex::new(r"(?i)error[:：]\s*").unwrap(),
                Regex::new(r"(?i)failed[:：]\s*").unwrap(),
                Regex::new(r"(?i)exception[:：]\s*").unwrap(),
                Regex::new(r"(?i)fatal[:：]\s*").unwrap(),
                Regex::new(r"(?i)panic[:：]\s*").unwrap(),
                Regex::new(r"(?i)timeout[:：]\s*").unwrap(),
                Regex::new(r"(?i)denied[:：]\s*").unwrap(),
                Regex::new(r"(?i)not found[:：]\s*").unwrap(),
                Regex::new(r"エラー[:：]\s*").unwrap(),
                Regex::new(r"失敗[:：]\s*").unwrap(),
            ],
            spinner_patterns: vec![
                // Claude Codeのスピナー文字
                Regex::new(r"[✢✳✶✻✷✸✹✺·]+").unwrap(),
                // 一般的なスピナー
                Regex::new(r"[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]+").unwrap(),
                // 動詞+ing
                Regex::new(r"(?i)\w+ing[….]+").unwrap(),
            ],
        }
    }

    /// 画面キャプチャから状態を判定
    pub fn parse(&self, content: &str) -> AgentStatus {
        let content_trimmed = content.trim();

        // 空の場合はUnknown
        if content_trimmed.is_empty() {
            return AgentStatus::Unknown;
        }

        // 1. 全体的にProcessingパターンがあるかチェック（スピナー、Thinking等）
        if self.is_processing(content_trimmed) {
            return AgentStatus::Processing;
        }

        // 2. プロンプトがあるかチェック
        let has_prompt = self.has_prompt(content_trimmed);

        if has_prompt {
            // プロンプトの直前の出力を取得
            let last_output = self.extract_last_output(content_trimmed);

            // 3. エラーチェック
            if let Some(error_msg) = self.detect_error(&last_output) {
                return AgentStatus::Error {
                    message: error_msg,
                };
            }

            // 4. 質問チェック
            if self.is_question(&last_output) {
                return AgentStatus::WaitingForInput {
                    question: last_output.lines().last().unwrap_or(&last_output).to_string(),
                };
            }

            // 5. 完了と判定（プロンプトがあるが質問でもエラーでもない）
            return AgentStatus::Idle;
        }

        // 6. プロンプトがなく、Processingパターンもない → まだ処理中とみなす
        AgentStatus::Processing
    }

    /// 処理中かどうかを判定（より厳密に）
    fn is_processing(&self, content: &str) -> bool {
        // 最後の10行をチェック
        let last_lines: Vec<&str> = content.lines().rev().take(10).collect();

        for line in &last_lines {
            let trimmed = line.trim();

            // 空行や罫線のみの行はスキップ
            if trimmed.is_empty() || trimmed.starts_with('─') || trimmed.starts_with('═') {
                continue;
            }

            // Processingパターンの検出（行全体がパターンにマッチする場合のみ）
            for pattern in &self.processing_patterns {
                if pattern.is_match(trimmed) {
                    return true;
                }
            }

            // スピナーパターンの検出（短い行で、かつ明確なスピナー文字を含む場合のみ）
            if trimmed.len() < 30 {
                for pattern in &self.spinner_patterns {
                    if pattern.is_match(trimmed) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// プロンプトが表示されているか
    fn has_prompt(&self, content: &str) -> bool {
        // 最後の数行をチェック（Claude CodeのUIでは ❯ の下に余分な行がある場合がある）
        let last_lines: Vec<&str> = content.lines().rev().take(5).collect();
        for line in &last_lines {
            let trimmed = line.trim();
            for pattern in &self.prompt_patterns {
                if pattern.is_match(trimmed) {
                    return true;
                }
            }
        }

        false
    }

    /// 質問かどうかを判定
    fn is_question(&self, content: &str) -> bool {
        // 空の場合は質問ではない
        if content.trim().is_empty() {
            return false;
        }

        // 最後の数行をチェック
        let last_lines: Vec<&str> = content.lines().rev().take(5).collect();

        for line in last_lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            for pattern in &self.question_patterns {
                if pattern.is_match(trimmed) {
                    return true;
                }
            }
        }

        false
    }

    /// エラーを検出
    fn detect_error(&self, content: &str) -> Option<String> {
        for pattern in &self.error_patterns {
            if let Some(captures) = pattern.captures(content) {
                // エラーメッセージの周辺を抽出
                if let Some(m) = captures.get(0) {
                    let start = m.start().saturating_sub(20);
                    let end = (m.end() + 100).min(content.len());
                    return Some(content[start..end].to_string());
                }
            }
        }
        None
    }

    /// 最後のプロンプトの直前の出力を抽出
    fn extract_last_output(&self, content: &str) -> String {
        // 最後の "> " または "❯ " の前までを抽出
        let mut prompt_pos = None;

        for pattern in &self.prompt_patterns {
            if let Some(pos) = pattern.find(content) {
                match prompt_pos {
                    None => prompt_pos = Some(pos.start()),
                    Some(existing) if pos.start() > existing => prompt_pos = Some(pos.start()),
                    _ => {}
                }
            }
        }

        match prompt_pos {
            Some(pos) => content[..pos].to_string(),
            None => content.to_string(),
        }
    }

    /// ANSIエスケープシーケンスを除去
    pub fn strip_ansi(content: &str) -> String {
        // 基本的なANSIエスケープシーケンスを除去
        let ansi_regex = Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap();
        ansi_regex.replace_all(content, "").to_string()
    }

    /// 出力から意味のあるテキストのみを抽出
    pub fn extract_meaningful_content(&self, content: &str) -> String {
        let stripped = Self::strip_ansi(content);

        // 複数の空行を1つに
        let collapsed = Regex::new(r"\n{3,}")
            .unwrap()
            .replace_all(&stripped, "\n\n");

        // 行末・行頭の空白を削除
        let trimmed: String = collapsed
            .lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");

        trimmed.trim().to_string()
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
    fn test_detect_processing() {
        let parser = OutputParser::new();

        // Thinking パターン
        let content = "Some output\nThinking...\n";
        assert_eq!(parser.parse(content), AgentStatus::Processing);

        // スピナーパターン
        let content = "Processing request\n✢✳✶\n";
        assert_eq!(parser.parse(content), AgentStatus::Processing);
    }

    #[test]
    fn test_detect_idle() {
        let parser = OutputParser::new();

        let content = "Task completed successfully\n❯ ";
        assert_eq!(parser.parse(content), AgentStatus::Idle);

        let content = "Here is the result\n> ";
        assert_eq!(parser.parse(content), AgentStatus::Idle);
    }

    #[test]
    fn test_detect_question() {
        let parser = OutputParser::new();

        let content = "Which option do you prefer?\n❯ ";
        let result = parser.parse(content);
        match result {
            AgentStatus::WaitingForInput { question } => {
                assert!(question.contains("prefer"));
            }
            _ => panic!("Expected WaitingForInput"),
        }
    }

    #[test]
    fn test_detect_error() {
        let parser = OutputParser::new();

        let content = "Error: Something went wrong\n❯ ";
        let result = parser.parse(content);
        match result {
            AgentStatus::Error { message } => {
                assert!(message.contains("Error"));
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_strip_ansi() {
        let content = "\x1b[32mGreen text\x1b[0m";
        let stripped = OutputParser::strip_ansi(content);
        assert_eq!(stripped, "Green text");
    }
}
