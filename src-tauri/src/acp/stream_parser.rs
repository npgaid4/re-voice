//! Stream JSON Parser for Claude Code CLI
//!
//! `--print --output-format stream-json` の出力をパースする。
//! 各行は独立したJSONオブジェクト。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use thiserror::Error;

use crate::log;
use super::state_machine::StateEvent;

/// パーサーエラー
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid event format: {0}")]
    InvalidFormat(String),
}

/// stream-json のイベントタイプ（Claude Code CLI出力）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// システム初期化
    System {
        subtype: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        tools: Vec<String>,
        #[serde(default)]
        permission_mode: Option<String>,
    },

    /// ユーザーメッセージ（エコーバック）
    User {
        message: UserMessage,
    },

    /// アシスタントメッセージ
    Assistant {
        message: AssistantMessage,
    },

    /// ツール使用
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },

    /// ツール結果
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },

    /// 最終結果
    Result {
        /// subtype: "success" or "error"
        #[serde(default)]
        subtype: Option<String>,
        /// 結果テキスト（直接文字列の場合）
        #[serde(default)]
        result: Option<String>,
        /// is_error フラグ
        #[serde(default)]
        is_error: bool,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        cost_usd: Option<f64>,
        #[serde(default)]
        duration_ms: Option<u64>,
        #[serde(default)]
        duration_api_ms: Option<u64>,
        #[serde(default)]
        num_turns: Option<u32>,
        #[serde(default)]
        total_cost_usd: Option<f64>,
        #[serde(default)]
        permission_denials: Vec<Value>,
    },

    /// エラー
    Error {
        error: ErrorDetail,
    },
}

/// ユーザーメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub role: String,
    pub content: String,
}

/// アシスタントメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub role: String,
    pub content: Vec<ContentBlock>,
    pub model: String,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// コンテンツブロック
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Thinking { thinking: String, #[serde(default)] signature: String },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, content: String, #[serde(default)] is_error: bool },
}

/// 使用量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
}

/// エラー詳細
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

/// パース結果
#[derive(Debug, Clone)]
pub enum ParsedEvent {
    /// 状態遷移イベント
    StateChange(StateEvent),
    /// 生のテキスト出力（ストリーミング）
    TextOutput(String),
    /// ツール実行情報
    ToolExecution {
        name: String,
        input: Value,
        result: Option<String>,
        is_error: bool,
    },
    /// 進捗情報
    Progress {
        message: String,
        percentage: Option<u8>,
    },
}

/// Stream JSON Parser
pub struct StreamParser {
    /// 現在処理中のツールID
    current_tool_id: Option<String>,
    /// 現在のツール名
    current_tool_name: Option<String>,
}

impl StreamParser {
    pub fn new() -> Self {
        Self {
            current_tool_id: None,
            current_tool_name: None,
        }
    }

    /// 1行のJSONをパースしてイベントを生成
    pub fn parse_line(&mut self, line: &str) -> Result<Vec<ParsedEvent>, ParseError> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(vec![]);
        }

        let event: StreamEvent = serde_json::from_str(line)?;

        let events = self.process_event(&event)?;
        Ok(events)
    }

    /// ストリームからイベントを読み込む
    pub fn parse_stream<R: Read>(
        &mut self,
        reader: R,
        mut callback: impl FnMut(ParsedEvent),
    ) -> Result<(), ParseError> {
        let buf_reader = BufReader::new(reader);

        for line in buf_reader.lines() {
            let line = line?;
            let events = self.parse_line(&line)?;

            for event in events {
                callback(event);
            }
        }

        Ok(())
    }

    /// StreamEventを処理してParsedEventに変換
    fn process_event(&mut self, event: &StreamEvent) -> Result<Vec<ParsedEvent>, ParseError> {
        match event {
            StreamEvent::System { subtype, session_id, model, .. } => {
                if subtype == "init" {
                    log::info("StreamParser", &format!(
                        "Initialized: session={:?}, model={:?}",
                        session_id, model
                    ));
                    return Ok(vec![ParsedEvent::StateChange(StateEvent::Initialized)]);
                }
                Ok(vec![])
            }

            StreamEvent::User { .. } => {
                // ユーザーメッセージはエコーバックなので無視
                Ok(vec![])
            }

            StreamEvent::Assistant { message } => {
                let mut events = vec![];

                // アシスタントメッセージ開始 = Processing
                if !message.content.is_empty() {
                    events.push(ParsedEvent::StateChange(StateEvent::TaskStarted {
                        prompt: String::new(),
                    }));
                }

                // テキストコンテンツを抽出
                for block in &message.content {
                    if let ContentBlock::Text { text } = block {
                        if !text.is_empty() {
                            events.push(ParsedEvent::TextOutput(text.clone()));
                        }
                    }
                }

                Ok(events)
            }

            StreamEvent::ToolUse { id, name, input } => {
                self.current_tool_id = Some(id.clone());
                self.current_tool_name = Some(name.clone());

                log::info("StreamParser", &format!("Tool use: {} ({})", name, id));

                Ok(vec![
                    ParsedEvent::StateChange(StateEvent::ToolUseStarted {
                        tool_name: name.clone(),
                    }),
                    ParsedEvent::ToolExecution {
                        name: name.clone(),
                        input: input.clone(),
                        result: None,
                        is_error: false,
                    },
                ])
            }

            StreamEvent::ToolResult { tool_use_id, content, is_error } => {
                log::info("StreamParser", &format!(
                    "Tool result for {}: error={}, len={}",
                    tool_use_id, is_error, content.len()
                ));

                // 権限エラーかどうかチェック
                if *is_error && self.is_permission_error(content) {
                    // 権限エラーの場合
                    let tool_name = self.current_tool_name.clone().unwrap_or_else(|| "unknown".to_string());
                    let tool_input = serde_json::json!({});

                    return Ok(vec![ParsedEvent::StateChange(
                        StateEvent::PermissionRequired {
                            tool_name,
                            tool_input,
                            request_id: tool_use_id.clone(),
                        },
                    )]);
                }

                // 通常のツール完了
                let tool_name = self.current_tool_name.clone();

                // ツール情報をクリア
                if self.current_tool_id.as_deref() == Some(tool_use_id) {
                    self.current_tool_id = None;
                    self.current_tool_name = None;
                }

                let mut events = vec![ParsedEvent::StateChange(StateEvent::ToolUseCompleted {
                    tool_name: tool_name.unwrap_or_else(|| "unknown".to_string()),
                    success: !is_error,
                })];

                // エラーの場合
                if *is_error {
                    events.push(ParsedEvent::StateChange(StateEvent::ErrorOccurred {
                        message: content.clone(),
                        recoverable: true,
                    }));
                }

                events.push(ParsedEvent::ToolExecution {
                    name: "result".to_string(),
                    input: serde_json::json!({}),
                    result: Some(content.clone()),
                    is_error: *is_error,
                });

                Ok(events)
            }

            StreamEvent::Result { subtype, result, is_error, session_id, cost_usd, duration_ms, permission_denials, .. } => {
                log::info("StreamParser", &format!(
                    "Result: subtype={:?}, session={:?}, cost={:?}, duration={:?}ms, is_error={}, denials={}",
                    subtype, session_id, cost_usd, duration_ms, is_error, permission_denials.len()
                ));

                // 結果テキストを取得
                let output = result.clone().unwrap_or_default();

                // 権限拒否がある場合
                if !permission_denials.is_empty() {
                    log::info("StreamParser", &format!("Permission denials: {:?}", permission_denials));
                }

                // エラーの場合
                if *is_error || subtype.as_deref() == Some("error") {
                    return Ok(vec![
                        ParsedEvent::StateChange(StateEvent::ErrorOccurred {
                            message: output.clone(),
                            recoverable: true,
                        }),
                        ParsedEvent::Progress {
                            message: format!("Error after {:?}ms", duration_ms),
                            percentage: Some(0),
                        },
                    ]);
                }

                Ok(vec![
                    ParsedEvent::StateChange(StateEvent::TaskCompleted {
                        output: output.clone(),
                    }),
                    ParsedEvent::Progress {
                        message: format!("Completed in {:?}ms", duration_ms),
                        percentage: Some(100),
                    },
                ])
            }

            StreamEvent::Error { error } => {
                log::error("StreamParser", &format!("Error: {} - {}", error.error_type, error.message));

                Ok(vec![ParsedEvent::StateChange(StateEvent::ErrorOccurred {
                    message: error.message.clone(),
                    recoverable: !error.error_type.contains("fatal"),
                })])
            }
        }
    }

    /// 権限エラーかどうかを判定
    fn is_permission_error(&self, content: &str) -> bool {
        // Claude Codeの権限エラーパターン
        content.contains("requires approval") ||
        content.contains("Do you want to proceed") ||
        content.contains("permission denied") ||
        content.contains("not allowed")
    }
}

impl Default for StreamParser {
    fn default() -> Self {
        Self::new()
    }
}

/// 許可要求を検出してパース
pub fn parse_permission_request(content: &str) -> Option<PermissionRequest> {
    // Claude Codeの権限プロンプトパターン
    // 例:
    // "This tool requires approval: Bash"
    // "Do you want to proceed?"
    // "1. Yes"
    // "2. No"

    let lines: Vec<&str> = content.lines().collect();

    // ツール名を抽出
    let tool_name = lines
        .iter()
        .find(|line| line.contains("requires approval"))
        .and_then(|line| {
            // "Bash requires approval" または "requires approval: Bash"
            if let Some(pos) = line.find("requires approval") {
                let after = &line[pos + 17..].trim_start_matches(':').trim();
                if !after.is_empty() {
                    return Some(after.to_string());
                }
                let before = &line[..pos].trim();
                if !before.is_empty() && !before.contains("This") {
                    return Some(before.to_string());
                }
            }
            None
        })?;

    // 選択肢を抽出
    let options: Vec<String> = lines
        .iter()
        .filter_map(|line| {
            let line = line.trim();
            // "1. Yes" または "❯ 1. Yes" パターン
            if line.starts_with(|c: char| c.is_ascii_digit()) || line.starts_with("❯") {
                // 数字とピリオドを除去
                let cleaned = line
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .trim_start_matches('.')
                    .trim_start_matches("❯")
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .trim_start_matches('.')
                    .trim();
                if !cleaned.is_empty() {
                    return Some(cleaned.to_string());
                }
            }
            None
        })
        .collect();

    Some(PermissionRequest {
        tool_name,
        options,
        request_id: uuid::Uuid::new_v4().to_string(),
    })
}

/// 権限要求情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub options: Vec<String>,
    pub request_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_system_init() {
        let mut parser = StreamParser::new();
        let line = r#"{"type":"system","subtype":"init","session_id":"test-123"}"#;

        let events = parser.parse_line(line).unwrap();
        assert_eq!(events.len(), 1);

        match &events[0] {
            ParsedEvent::StateChange(StateEvent::Initialized) => {}
            _ => panic!("Expected Initialized event"),
        }
    }

    #[test]
    fn test_parse_tool_use() {
        let mut parser = StreamParser::new();
        let line = r#"{"type":"tool_use","id":"tool-1","name":"Read","input":{"file_path":"/test.txt"}}"#;

        let events = parser.parse_line(line).unwrap();
        assert!(events.len() >= 1);

        let found = events.iter().any(|e| {
            matches!(
                e,
                ParsedEvent::StateChange(StateEvent::ToolUseStarted { tool_name }) if tool_name == "Read"
            )
        });
        assert!(found);
    }

    #[test]
    fn test_parse_result() {
        let mut parser = StreamParser::new();
        // 実際のClaude Code出力形式に合わせて更新
        let line = r#"{"type":"result","subtype":"success","result":"Done!","session_id":"test-123"}"#;

        let events = parser.parse_line(line).unwrap();

        let found = events.iter().any(|e| {
            matches!(
                e,
                ParsedEvent::StateChange(StateEvent::TaskCompleted { output }) if output == "Done!"
            )
        });
        assert!(found);
    }

    #[test]
    fn test_parse_permission_request() {
        let content = r#"Bash requires approval

Do you want to proceed?
1. Yes
2. No"#;

        let request = parse_permission_request(content);
        assert!(request.is_some());

        let request = request.unwrap();
        assert_eq!(request.tool_name, "Bash");
        assert!(request.options.contains(&"Yes".to_string()));
    }
}
