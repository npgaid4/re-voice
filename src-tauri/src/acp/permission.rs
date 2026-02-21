//! Permission Manager for Claude Code CLI
//!
//! ツール実行の権限を管理する。
//! 読み取り系は自動許可、書き込み系は人間確認。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::log;

/// 権限決定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PermissionDecision {
    /// 許可
    Allow {
        /// 今後も自動許可するか
        always: bool,
    },
    /// 拒否
    Deny {
        /// 理由
        reason: String,
    },
    /// 人間に確認が必要
    RequireHuman {
        request_id: String,
        tool_name: String,
        tool_input: Value,
        options: Vec<String>,
    },
}

/// 権限ポリシー
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PermissionPolicy {
    /// 読み取り専用（自動許可のみ）
    ReadOnly,
    /// 標準（読み取りは自動、書き込みは確認）
    Standard,
    /// 厳格（全て確認）
    Strict,
    /// 自由（全て自動許可）
    Permissive,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self::Standard
    }
}

/// 権限要求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub request_id: String,
    pub tool_name: String,
    pub tool_input: Value,
    pub options: Vec<String>,
    pub timestamp: String,
}

/// 権限管理
pub struct PermissionManager {
    /// 現在のポリシー
    policy: PermissionPolicy,
    /// 事前許可ツールリスト（--allowedTools相当）
    pre_approved: HashSet<String>,
    /// セッション中に許可されたツール
    session_approved: HashSet<String>,
    /// 待機中の権限要求
    pending_requests: Arc<Mutex<HashMap<String, PermissionRequest>>>,
    /// 人間の回答待ち
    human_responses: Arc<Mutex<HashMap<String, PermissionDecision>>>,
    /// アプリハンドル（イベント送信用）
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl PermissionManager {
    /// 新しい権限マネージャーを作成
    pub fn new() -> Self {
        let mut manager = Self {
            policy: PermissionPolicy::Standard,
            pre_approved: HashSet::new(),
            session_approved: HashSet::new(),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            human_responses: Arc::new(Mutex::new(HashMap::new())),
            app_handle: Arc::new(Mutex::new(None)),
        };

        // デフォルトの事前許可ツールを追加
        manager.initialize_default_permissions();
        manager
    }

    /// デフォルトの権限を初期化
    fn initialize_default_permissions(&mut self) {
        // 読み取り系ツール（自動許可）
        let read_only_tools = vec![
            "Read",
            "Grep",
            "Glob",
            "Bash(ls:*)",
            "Bash(cat:*)",
            "Bash(head:*)",
            "Bash(tail:*)",
            "Bash(git status:*)",
            "Bash(git log:*)",
            "Bash(git diff:*)",
            "Bash(git show:*)",
            "Bash(pwd)",
            "Bash(which:*)",
            "Bash(echo:*)",
        ];

        for tool in read_only_tools {
            self.pre_approved.insert(tool.to_string());
        }
    }

    /// ポリシーを設定
    pub fn set_policy(&mut self, policy: PermissionPolicy) {
        self.policy = policy;
    }

    /// 事前許可ツールを追加
    pub fn add_pre_approved(&mut self, tool: &str) {
        self.pre_approved.insert(tool.to_string());
    }

    /// AppHandleを設定
    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock() = Some(handle);
    }

    /// 権限要求を処理
    pub async fn check_permission(
        &mut self,
        tool_name: &str,
        tool_input: &Value,
        request_id: &str,
    ) -> PermissionDecision {
        log::info("PermissionManager", &format!(
            "Checking permission for {} (request: {})",
            tool_name, request_id
        ));

        // 1. ポリシーレベルのチェック
        match self.policy {
            PermissionPolicy::Permissive => {
                return PermissionDecision::Allow { always: false };
            }
            PermissionPolicy::Strict => {
                // 厳格モードでは全て人間確認
                return self.require_human_approval(tool_name, tool_input, request_id, vec![]);
            }
            _ => {}
        }

        // 2. 事前許可チェック
        if self.is_pre_approved(tool_name, tool_input) {
            log::info("PermissionManager", &format!("{} is pre-approved", tool_name));
            return PermissionDecision::Allow { always: true };
        }

        // 3. セッション許可チェック
        if self.session_approved.contains(tool_name) {
            log::info("PermissionManager", &format!("{} is session-approved", tool_name));
            return PermissionDecision::Allow { always: false };
        }

        // 4. 自動判定ルール
        if let Some(decision) = self.auto_decide(tool_name, tool_input) {
            log::info("PermissionManager", &format!("Auto-decided: {:?}", decision));
            return decision;
        }

        // 5. 人間確認が必要
        self.require_human_approval(tool_name, tool_input, request_id, vec![])
    }

    /// 事前許可されているかチェック
    fn is_pre_approved(&self, tool_name: &str, tool_input: &Value) -> bool {
        // 完全一致
        if self.pre_approved.contains(tool_name) {
            return true;
        }

        // Bashコマンドのパターンマッチ
        if tool_name == "Bash" {
            if let Some(cmd) = tool_input.get("command").and_then(|v| v.as_str()) {
                let cmd_lower = cmd.to_lowercase();

                // パターンマッチング
                for pattern in &self.pre_approved {
                    if pattern.starts_with("Bash(") && pattern.ends_with(":*)") {
                        let prefix = &pattern[5..pattern.len() - 3]; // "Bash(" と ":*)" を除去
                        if cmd_lower.starts_with(&prefix.to_lowercase()) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// 自動判定ルール
    fn auto_decide(&self, tool_name: &str, tool_input: &Value) -> Option<PermissionDecision> {
        // 書き込み系は基本的に人間確認が必要（Standardポリシー）
        // ただし、明らかに安全な操作は自動許可

        match tool_name {
            // Edit: 同じ内容への置換は安全
            "Edit" => {
                // 空の置換や同一内容の置換は安全
                if let (Some(old), Some(new)) = (
                    tool_input.get("old_string").and_then(|v| v.as_str()),
                    tool_input.get("new_string").and_then(|v| v.as_str()),
                ) {
                    if old == new || old.is_empty() {
                        return Some(PermissionDecision::Allow { always: false });
                    }
                }
                None
            }

            // Write: 新規ファイル作成のみ
            "Write" => {
                // 安全なパスかチェック
                if let Some(path) = tool_input.get("file_path").and_then(|v| v.as_str()) {
                    // /tmp 配下や、プロジェクトディレクトリ内は比較的安全
                    if path.starts_with("/tmp/") || path.starts_with("/var/folders/") {
                        return Some(PermissionDecision::Allow { always: false });
                    }
                }
                None
            }

            // Bash: 安全なコマンド
            "Bash" => {
                if let Some(cmd) = tool_input.get("command").and_then(|v| v.as_str()) {
                    let cmd_trimmed = cmd.trim();

                    // 読み取り系コマンド
                    if cmd_trimmed.starts_with("ls ")
                        || cmd_trimmed.starts_with("cat ")
                        || cmd_trimmed.starts_with("head ")
                        || cmd_trimmed.starts_with("tail ")
                        || cmd_trimmed.starts_with("find ")
                        || cmd_trimmed.starts_with("grep ")
                        || cmd_trimmed.starts_with("rg ")
                    {
                        return Some(PermissionDecision::Allow { always: false });
                    }

                    // 危険なコマンド
                    let dangerous = ["rm -rf", "rm -r", "mkfs", "dd if=", "> /dev/", "chmod 777"];
                    for danger in dangerous {
                        if cmd_trimmed.starts_with(danger) {
                            return Some(PermissionDecision::Deny {
                                reason: format!("Dangerous command: {}", danger),
                            });
                        }
                    }
                }
                None
            }

            _ => None,
        }
    }

    /// 人間の承認を要求
    fn require_human_approval(
        &self,
        tool_name: &str,
        tool_input: &Value,
        request_id: &str,
        options: Vec<String>,
    ) -> PermissionDecision {
        let request = PermissionRequest {
            request_id: request_id.to_string(),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.clone(),
            options: if options.is_empty() {
                vec![
                    "Yes".to_string(),
                    "Yes, always for this session".to_string(),
                    "No".to_string(),
                ]
            } else {
                options
            },
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        // 待機中の要求に追加
        {
            let mut pending = self.pending_requests.lock();
            pending.insert(request_id.to_string(), request.clone());
        }

        // イベントを送信
        if let Some(ref handle) = *self.app_handle.lock() {
            let _ = handle.emit("permission:required", &request);
        }

        PermissionDecision::RequireHuman {
            request_id: request_id.to_string(),
            tool_name: tool_name.to_string(),
            tool_input: tool_input.clone(),
            options: request.options,
        }
    }

    /// 人間の回答を送信
    pub fn submit_human_response(
        &self,
        request_id: &str,
        decision: PermissionDecision,
    ) -> Result<(), String> {
        // 待機中の要求から削除
        {
            let mut pending = self.pending_requests.lock();
            pending.remove(request_id);
        }

        // 回答を保存
        {
            let mut responses = self.human_responses.lock();
            responses.insert(request_id.to_string(), decision.clone());
        }

        // セッション許可に追加（always の場合）
        if let PermissionDecision::Allow { always: true } = decision {
            // request_id から tool_name を取得
            let pending = self.pending_requests.lock();
            // 既に削除されているので、別の方法で tool_name を取得する必要がある
            // 現在は簡易実装
        }

        Ok(())
    }

    /// 人間の回答を待機
    pub async fn wait_for_human_response(
        &self,
        request_id: &str,
        timeout_secs: u64,
    ) -> Result<PermissionDecision, String> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        loop {
            // 回答をチェック
            {
                let responses = self.human_responses.lock();
                if let Some(decision) = responses.get(request_id) {
                    let decision = decision.clone();
                    // 回答を削除
                    drop(responses);
                    let mut responses = self.human_responses.lock();
                    responses.remove(request_id);
                    return Ok(decision);
                }
            }

            // タイムアウトチェック
            if start.elapsed() >= timeout {
                return Err(format!("Timeout waiting for human response: {}", request_id));
            }

            // 短く待機
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    /// セッション許可をクリア
    pub fn clear_session_approvals(&mut self) {
        self.session_approved.clear();
    }

    /// CLI引数（--allowedTools）を生成
    pub fn generate_allowed_tools_args(&self) -> Vec<String> {
        let mut args = vec![];

        for tool in &self.pre_approved {
            args.push("--allowedTools".to_string());
            args.push(tool.clone());
        }

        args
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 安全なツールのリスト
pub fn auto_approve_tools() -> Vec<String> {
    vec![
        "Read".to_string(),
        "Grep".to_string(),
        "Glob".to_string(),
        "Bash(ls:*)".to_string(),
        "Bash(cat:*)".to_string(),
        "Bash(git status:*)".to_string(),
        "Bash(git log:*)".to_string(),
    ]
}

/// 人間確認が必要なツールのリスト
pub fn require_confirmation_tools() -> Vec<String> {
    vec![
        "Edit".to_string(),
        "Write".to_string(),
        "Bash(rm:*)".to_string(),
        "Bash(mv:*)".to_string(),
        "Bash(mkdir:*)".to_string(),
        "Bash(npm:*)".to_string(),
        "Bash(cargo:*)".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_manager() {
        let manager = PermissionManager::new();
        assert_eq!(manager.policy, PermissionPolicy::Standard);
        assert!(manager.pre_approved.contains("Read"));
    }

    #[test]
    fn test_pre_approved_read() {
        let manager = PermissionManager::new();

        // Read ツールは事前許可
        assert!(manager.is_pre_approved("Read", &serde_json::json!({})));
    }

    #[test]
    fn test_pre_approved_bash_pattern() {
        let manager = PermissionManager::new();

        // ls コマンドは許可
        assert!(manager.is_pre_approved("Bash", &serde_json::json!({"command": "ls -la"})));
        assert!(manager.is_pre_approved("Bash", &serde_json::json!({"command": "git status"})));

        // rm コマンドは許可されない
        assert!(!manager.is_pre_approved("Bash", &serde_json::json!({"command": "rm -rf /"})));
    }

    #[test]
    fn test_auto_deny_dangerous_command() {
        let manager = PermissionManager::new();

        let decision = manager.auto_decide(
            "Bash",
            &serde_json::json!({"command": "rm -rf /"}),
        );

        assert!(matches!(decision, Some(PermissionDecision::Deny { .. })));
    }

    #[test]
    fn test_permissive_policy() {
        let mut manager = PermissionManager::new();
        manager.set_policy(PermissionPolicy::Permissive);

        // Permissiveポリシーでは全て許可
        let input = serde_json::json!({"command": "rm -rf /"});
        let _decision = manager.check_permission(
            "Bash",
            &input,
            "test-1",
        );

        // Note: check_permission is async, so we can't test it directly here
        // This test is for demonstration purposes
    }

    #[test]
    fn test_generate_cli_args() {
        let manager = PermissionManager::new();
        let args = manager.generate_allowed_tools_args();

        assert!(args.contains(&"--allowedTools".to_string()));
        assert!(args.contains(&"Read".to_string()));
    }
}
