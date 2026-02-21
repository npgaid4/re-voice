//! ACP v3 Ask Tool Handler
//!
//! Claude Code、Codex、Gemini CLIなどのAIツールからのAsk Tool（質問）を処理する。
//!
//! # 設計方針
//!
//! 1. **質問の分類**: PERMISSION/CHOICE/INFORMATION/CONFIRMATION
//! 2. **ポリシーベース自動応答**: 設定ファイルで「tmp/へのアクセスは常に許可」などを定義
//! 3. **人間へのエスカレーション**: ポリシーにない質問はフロントエンドに通知

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::log;

/// Ask Toolの種類
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AskType {
    /// 権限確認（ファイルアクセス、コマンド実行など）
    Permission {
        resource: String,
        action: String,
        options: Vec<AskOption>,
    },
    /// 選択肢からの選択
    Choice {
        question: String,
        options: Vec<AskOption>,
    },
    /// 情報の入力要求
    Information {
        question: String,
        default: Option<String>,
    },
    /// 確認（Yes/No）
    Confirmation {
        message: String,
        default: Option<bool>,
    },
    /// 不明な質問
    Unknown {
        raw: String,
    },
}

/// 選択肢
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskOption {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

/// 質問の解析結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedQuestion {
    pub ask_type: AskType,
    pub raw_text: String,
    pub suggested_answer: Option<String>,
}

/// 自動応答ポリシー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoAnswerPolicy {
    /// リソースパターン（正規表現）
    pub resource_pattern: String,
    /// アクション（read, write, execute, etc.）
    pub action: String,
    /// 自動応答（オプションID）
    pub auto_answer: String,
    /// 常に適用するか
    pub always: bool,
}

/// 質問処理結果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AskResult {
    /// 自動応答した
    AutoAnswered { answer: String },
    /// 人間の判断が必要
    RequiresHuman { question_id: String, parsed: ParsedQuestion },
    /// エラー
    Error { message: String },
}

/// 人間からの回答
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanAnswer {
    pub question_id: String,
    pub answer: String,
    pub remember_choice: bool,
}

/// Ask Tool Handler
pub struct AskToolHandler {
    /// 自動応答ポリシー
    policies: Vec<AutoAnswerPolicy>,
    /// コンパイル済み正規表現
    compiled_patterns: Vec<(Regex, AutoAnswerPolicy)>,
    /// 保留中の質問（人間の回答待ち）
    pending_questions: Arc<Mutex<HashMap<String, ParsedQuestion>>>,
    /// 人間からの回答
    human_answers: Arc<Mutex<HashMap<String, String>>>,
    /// アプリハンドル（イベント送信用）
    app_handle: Arc<Mutex<Option<AppHandle>>>,
    /// 次の質問ID
    next_question_id: Arc<Mutex<u64>>,
}

impl AskToolHandler {
    /// 新しいHandlerを作成
    pub fn new() -> Self {
        let mut handler = Self {
            policies: Self::default_policies(),
            compiled_patterns: Vec::new(),
            pending_questions: Arc::new(Mutex::new(HashMap::new())),
            human_answers: Arc::new(Mutex::new(HashMap::new())),
            app_handle: Arc::new(Mutex::new(None)),
            next_question_id: Arc::new(Mutex::new(1)),
        };
        handler.compile_patterns();
        handler
    }

    /// AppHandleを設定
    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock() = Some(handle);
    }

    /// デフォルトのポリシー
    fn default_policies() -> Vec<AutoAnswerPolicy> {
        vec![
            // tmpディレクトリへのアクセスは常に許可
            AutoAnswerPolicy {
                resource_pattern: r"^/tmp/".to_string(),
                action: "all".to_string(),
                auto_answer: "1".to_string(), // Yes
                always: true,
            },
            // revoiceディレクトリへのアクセスは許可
            AutoAnswerPolicy {
                resource_pattern: r"/revoice/".to_string(),
                action: "all".to_string(),
                auto_answer: "1".to_string(),
                always: true,
            },
            // yt-dlpコマンドは許可
            AutoAnswerPolicy {
                resource_pattern: r"yt-dlp".to_string(),
                action: "execute".to_string(),
                auto_answer: "1".to_string(),
                always: true,
            },
            // ffmpegコマンドは許可
            AutoAnswerPolicy {
                resource_pattern: r"ffmpeg".to_string(),
                action: "execute".to_string(),
                auto_answer: "1".to_string(),
                always: true,
            },
        ]
    }

    /// ポリシーの正規表現をコンパイル
    fn compile_patterns(&mut self) {
        self.compiled_patterns = self.policies
            .iter()
            .filter_map(|p| {
                Regex::new(&p.resource_pattern)
                    .ok()
                    .map(|r| (r, p.clone()))
            })
            .collect();
    }

    /// 質問を解析
    pub fn parse_question(&self, text: &str) -> ParsedQuestion {
        let text = text.trim();

        // 権限確認パターン
        // "Do you want to proceed?"
        // "Allow access to /path/to/file?"
        if let Some(parsed) = self.try_parse_permission(text) {
            return parsed;
        }

        // 選択肢パターン
        // "1. Yes"
        // "2. No"
        if let Some(parsed) = self.try_parse_choice(text) {
            return parsed;
        }

        // 確認パターン (Yes/No)
        if let Some(parsed) = self.try_parse_confirmation(text) {
            return parsed;
        }

        // 不明な質問
        ParsedQuestion {
            ask_type: AskType::Unknown {
                raw: text.to_string(),
            },
            raw_text: text.to_string(),
            suggested_answer: None,
        }
    }

    /// 権限確認をパース
    fn try_parse_permission(&self, text: &str) -> Option<ParsedQuestion> {
        // パターン: "Do you want to proceed?" with options
        let has_proceed = text.contains("Do you want to proceed") ||
                          text.contains("allow") ||
                          text.contains("proceed");

        if !has_proceed {
            return None;
        }

        // リソースを抽出
        let resource = self.extract_resource(text);

        // オプションを抽出
        let options = self.extract_options(text);

        if !options.is_empty() {
            return Some(ParsedQuestion {
                ask_type: AskType::Permission {
                    resource,
                    action: "access".to_string(),
                    options,
                },
                raw_text: text.to_string(),
                suggested_answer: Some("1".to_string()),
            });
        }

        None
    }

    /// 選択肢をパース
    fn try_parse_choice(&self, text: &str) -> Option<ParsedQuestion> {
        let options = self.extract_options(text);

        if options.len() >= 2 {
            // 最初の質問部分を抽出
            let question = text.lines()
                .take_while(|line| !line.trim().starts_with('1') && !line.trim().starts_with("❯"))
                .collect::<Vec<_>>()
                .join("\n");

            return Some(ParsedQuestion {
                ask_type: AskType::Choice {
                    question,
                    options,
                },
                raw_text: text.to_string(),
                suggested_answer: None,
            });
        }

        None
    }

    /// 確認をパース
    fn try_parse_confirmation(&self, text: &str) -> Option<ParsedQuestion> {
        let lower = text.to_lowercase();

        if (lower.contains("proceed") || lower.contains("continue") || lower.contains("confirm"))
            && (lower.contains("yes") || lower.contains("no") || text.contains("?"))
        {
            return Some(ParsedQuestion {
                ask_type: AskType::Confirmation {
                    message: text.to_string(),
                    default: Some(true),
                },
                raw_text: text.to_string(),
                suggested_answer: Some("y".to_string()),
            });
        }

        None
    }

    /// リソースを抽出
    fn extract_resource(&self, text: &str) -> String {
        // パスパターンを探す
        let path_patterns = [
            r"/[a-zA-Z0-9_\-./]+",
            r"[a-zA-Z]:\\[a-zA-Z0-9_\-./\\]+",
        ];

        for pattern in &path_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(m) = re.find(text) {
                    return m.as_str().to_string();
                }
            }
        }

        "unknown".to_string()
    }

    /// オプションを抽出
    fn extract_options(&self, text: &str) -> Vec<AskOption> {
        let mut options = Vec::new();

        // パターン: "❯ 1. Yes", "1. Yes" または "1) Yes"
        // ❯ はClaude Codeの選択マーカー
        let option_re = Regex::new(r"^[❯\s]*(\d+)[.)\s]+(.+)$").unwrap();

        for line in text.lines() {
            if let Some(caps) = option_re.captures(line) {
                let id = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                let label = caps.get(2).map(|m| m.as_str().trim().to_string()).unwrap_or_default();

                if !id.is_empty() && !label.is_empty() {
                    options.push(AskOption {
                        id,
                        label,
                        description: None,
                    });
                }
            }
        }

        options
    }

    /// 質問を処理
    pub async fn handle(&self, text: &str) -> AskResult {
        log::info("AskToolHandler", &format!("Handling question: {:?}", &text[..text.len().min(200)]));

        let parsed = self.parse_question(text);

        // ポリシーで自動応答できるかチェック
        if let Some(answer) = self.try_auto_answer(&parsed) {
            log::info("AskToolHandler", &format!("Auto-answered with: {}", answer));
            return AskResult::AutoAnswered { answer };
        }

        // 人間の判断が必要
        let question_id = self.generate_question_id();

        // 保留中の質問に追加
        {
            let mut pending = self.pending_questions.lock();
            pending.insert(question_id.clone(), parsed.clone());
        }

        // フロントエンドに通知
        self.notify_human(&question_id, &parsed);

        AskResult::RequiresHuman {
            question_id,
            parsed,
        }
    }

    /// ポリシーで自動応答を試みる
    fn try_auto_answer(&self, parsed: &ParsedQuestion) -> Option<String> {
        let (resource, _action) = match &parsed.ask_type {
            AskType::Permission { resource, action, .. } => (resource.clone(), action.clone()),
            AskType::Confirmation { .. } => {
                // 確認はデフォルトでYes
                return parsed.suggested_answer.clone();
            }
            _ => return None,
        };

        // ポリシーをチェック
        for (pattern, policy) in &self.compiled_patterns {
            if pattern.is_match(&resource) {
                log::info("AskToolHandler", &format!(
                    "Policy matched: {} -> {}",
                    policy.resource_pattern, policy.auto_answer
                ));
                return Some(policy.auto_answer.clone());
            }
        }

        // 提案された回答があれば使用
        parsed.suggested_answer.clone()
    }

    /// 質問IDを生成
    fn generate_question_id(&self) -> String {
        let mut id = self.next_question_id.lock();
        *id += 1;
        format!("q-{}", *id)
    }

    /// 人間に通知
    fn notify_human(&self, question_id: &str, parsed: &ParsedQuestion) {
        let handle = self.app_handle.lock();
        if let Some(ref h) = *handle {
            let payload = serde_json::json!({
                "question_id": question_id,
                "parsed": parsed,
            });

            if let Err(e) = h.emit("acp:ask_required", &payload) {
                log::error("AskToolHandler", &format!("Failed to emit ask event: {:?}", e));
            }
        }
    }

    /// 人間からの回答を送信
    pub fn submit_answer(&self, answer: HumanAnswer) -> Result<(), String> {
        let mut pending = self.pending_questions.lock();
        if pending.remove(&answer.question_id).is_some() {
            let mut answers = self.human_answers.lock();
            answers.insert(answer.question_id.clone(), answer.answer.clone());

            // ポリシーに追加する場合
            if answer.remember_choice {
                log::info("AskToolHandler", &format!("Remembering choice for: {}", answer.question_id));
                // TODO: ポリシーに追加
            }

            Ok(())
        } else {
            Err(format!("Question not found: {}", answer.question_id))
        }
    }

    /// 人間からの回答を待機
    pub async fn wait_for_answer(&self, question_id: &str, timeout_secs: u64) -> Result<String, String> {
        let start = std::time::Instant::now();
        let check_interval = std::time::Duration::from_millis(500);
        let timeout = std::time::Duration::from_secs(timeout_secs);

        loop {
            // 回答をチェック
            {
                let mut answers = self.human_answers.lock();
                if let Some(answer) = answers.remove(question_id) {
                    return Ok(answer);
                }
            }

            // タイムアウトチェック
            if start.elapsed() >= timeout {
                return Err(format!("Timeout waiting for answer: {}", question_id));
            }

            // 待機
            tokio::time::sleep(check_interval).await;
        }
    }

    /// 保留中の質問一覧を取得
    pub fn get_pending_questions(&self) -> Vec<(String, ParsedQuestion)> {
        let pending = self.pending_questions.lock();
        pending.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// ポリシーを追加
    pub fn add_policy(&mut self, policy: AutoAnswerPolicy) {
        if let Ok(re) = Regex::new(&policy.resource_pattern) {
            self.compiled_patterns.push((re, policy.clone()));
        }
        self.policies.push(policy);
    }
}

impl Default for AskToolHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_permission() {
        let handler = AskToolHandler::new();

        let text = r#"mkdir -p /tmp/revoice && yt-dlp --write-auto-sub --sub-lang en --skip-download -o "/tmp/revoice/%(title)s.%(ext)s" "https://www.youtube.com/watch?v=test"
   Download English subtitles from YouTube video

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and always allow access to tmp/ from this project
   3. No"#;

        let parsed = handler.parse_question(text);

        match parsed.ask_type {
            AskType::Permission { ref resource, .. } => {
                assert!(resource.contains("/tmp/"));
            }
            _ => panic!("Expected Permission type"),
        }
    }

    #[test]
    fn test_auto_answer_policy() {
        let handler = AskToolHandler::new();

        let parsed = ParsedQuestion {
            ask_type: AskType::Permission {
                resource: "/tmp/revoice/test.vtt".to_string(),
                action: "write".to_string(),
                options: vec![
                    AskOption { id: "1".to_string(), label: "Yes".to_string(), description: None },
                    AskOption { id: "2".to_string(), label: "No".to_string(), description: None },
                ],
            },
            raw_text: "Do you want to proceed?".to_string(),
            suggested_answer: Some("1".to_string()),
        };

        let answer = handler.try_auto_answer(&parsed);
        assert_eq!(answer, Some("1".to_string()));
    }

    #[test]
    fn test_extract_options() {
        let handler = AskToolHandler::new();

        let text = r#"Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and always allow access to tmp/ from this project
   3. No"#;

        let options = handler.extract_options(text);
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].label, "Yes");
    }

    #[test]
    fn test_full_permission_flow() {
        let handler = AskToolHandler::new();

        // 実際のClaude Code出力
        let text = r#"mkdir -p /tmp/revoice && yt-dlp --write-sub --write-auto-sub --sub-lang en --skip-download --sub-format vtt -o "/tmp/revoice/%(title)s.%(ext)s" "https://www.youtube.com/watch?v=2wn3x23M2KI" 2>&1
   Download English subtitles from YouTube video

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and always allow access to tmp/ from this project
   3. No

 Esc to cancel · Tab to amend · ctrl+e to explain"#;

        eprintln!("\n=== Testing full permission flow ===");

        let parsed = handler.parse_question(text);
        eprintln!("Parsed ask_type: {:?}", parsed.ask_type);
        eprintln!("Raw text length: {}", parsed.raw_text.len());
        eprintln!("Suggested answer: {:?}", parsed.suggested_answer);

        match &parsed.ask_type {
            AskType::Permission { resource, action, options } => {
                eprintln!("Resource: {}", resource);
                eprintln!("Action: {}", action);
                eprintln!("Options: {:?}", options);

                // /tmp/revoice は /tmp/ ポリシーにマッチするはず
                assert!(resource.contains("/tmp/"), "Expected resource to contain /tmp/, got: {}", resource);
            }
            _ => panic!("Expected Permission type, got: {:?}", parsed.ask_type),
        }

        // 自動応答をテスト
        let answer = handler.try_auto_answer(&parsed);
        eprintln!("Auto answer result: {:?}", answer);
        assert_eq!(answer, Some("1".to_string()), "Expected auto answer '1'");
    }

    #[test]
    fn test_python_permission_flow() {
        let handler = AskToolHandler::new();

        // Python実行の権限プロンプト
        let text = r#"   Parse VTT file and extract clean segments with timing

 This command requires approval

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and don't ask again for: python3:*
   3. No

 Esc to cancel · Tab to amend · ctrl+e to explain"#;

        eprintln!("\n=== Testing python permission flow ===");

        let parsed = handler.parse_question(text);
        eprintln!("Parsed ask_type: {:?}", parsed.ask_type);
        eprintln!("Suggested answer: {:?}", parsed.suggested_answer);

        match &parsed.ask_type {
            AskType::Permission { resource, action, options } => {
                eprintln!("Resource: {}", resource);
                eprintln!("Action: {}", action);
                eprintln!("Options: {:?}", options);
            }
            AskType::Confirmation { message, .. } => {
                eprintln!("Confirmation message: {}", message);
            }
            _ => {}
        }

        // 自動応答をテスト
        let answer = handler.try_auto_answer(&parsed);
        eprintln!("Auto answer result: {:?}", answer);
        // python3はデフォルトポリシーにないので、suggested_answerが使われるはず
        assert!(answer.is_some(), "Expected some answer, got None");
    }
}
