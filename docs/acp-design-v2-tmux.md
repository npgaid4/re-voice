# ACP v2: tmux + プロンプト内マルチエージェント

## 設計方針

**ハイブリッドアプローチ:**
- **シンプルなタスク**: 1つのClaude Code内で「プロンプト内マルチエージェント」
- **複雑なタスク**: tmuxで複数のClaude Codeインスタンスを管理

```
┌─────────────────────────────────────────────────────────────┐
│                    Re-Voice App                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              TaskRouter                              │   │
│  │  タスクの複雑さを判定してルーティング                    │   │
│  └─────────────────────────────────────────────────────┘   │
│                            │                                │
│              ┌─────────────┴─────────────┐                 │
│              ▼                           ▼                 │
│  ┌─────────────────────┐   ┌─────────────────────────┐    │
│  │ シンプルタスク       │   │ 複雑なタスク             │    │
│  │                     │   │                         │    │
│  │ 単一Claude Code     │   │ tmuxマルチペイン         │    │
│  │ プロンプト内で完結   │   │ 複数エージェント連携      │    │
│  └─────────────────────┘   └─────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

---

## なぜハイブリッドか

### プロンプト内マルチエージェントの利点
- **リソース効率**: Claude Code 1つで済む
- **高速**: 通信オーバーヘッドなし
- **シンプル**: 実装が楽

### tmux統合の利点
- **視覚的デバッグ**: `tmux attach`で画面確認
- **柔軟性**: エージェントごとに異なる設定可能
- **拡張性**: 将来の並列処理に対応

---

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Re-Voice App                                │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    TmuxOrchestrator                            │  │
│  │  - セッション管理                                               │  │
│  │  - ペイン作成/破棄                                              │  │
│  │  - メッセージルーティング                                        │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                       │
│                              ▼                                       │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    tmux session: revoice                       │  │
│  │  ┌─────────────────┬─────────────────┬─────────────────┐      │  │
│  │  │ pane 0          │ pane 1          │ pane 2          │      │  │
│  │  │ claude-code-0   │ claude-code-1   │ codex           │      │  │
│  │  │ [翻訳担当]       │ [品質担当]       │ [コード生成]     │      │  │
│  │  │                 │                 │                 │      │  │
│  │  │ > _             │ > _             │ > _             │      │  │
│  │  └─────────────────┴─────────────────┴─────────────────┘      │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## TmuxOrchestrator 設計

```rust
// src-tauri/src/acp/tmux.rs

use std::collections::HashMap;
use std::process::Command;

/// tmuxベースのマルチエージェントオーケストレーター
pub struct TmuxOrchestrator {
    session_name: String,
    panes: HashMap<String, PaneInfo>,
    socket_path: Option<String>,
}

/// ペイン情報
#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub pane_id: String,           // tmuxペインID (%0, %1, ...)
    pub agent_id: String,          // 論理ID (claude-code-0)
    pub agent_type: AgentType,
    pub capabilities: Vec<String>,
    pub status: AgentStatus,
}

#[derive(Debug, Clone)]
pub enum AgentType {
    ClaudeCode,
    Codex,
    GenericShell,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,           // プロンプト待ち
    Busy,           // 処理中
    Error,          // エラー状態
    Unknown,
}

impl TmuxOrchestrator {
    pub fn new(session_name: &str) -> Self {
        Self {
            session_name: session_name.to_string(),
            panes: HashMap::new(),
            socket_path: None,
        }
    }

    /// tmuxセッションを作成
    pub fn create_session(&mut self) -> Result<(), TmuxError> {
        let output = Command::new("tmux")
            .args(["new-session", "-d", "-s", &self.session_name])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(TmuxError::SessionCreationFailed(
                String::from_utf8_lossy(&output.stderr).to_string()
            ));
        }

        // 最初のペインを登録
        let pane_id = self.get_first_pane_id()?;
        self.panes.insert("main".to_string(), PaneInfo {
            pane_id,
            agent_id: "main".to_string(),
            agent_type: AgentType::GenericShell,
            capabilities: vec![],
            status: AgentStatus::Idle,
        });

        Ok(())
    }

    /// 新しいペインを作成してエージェントを起動
    pub fn spawn_agent(
        &mut self,
        agent_id: &str,
        agent_type: AgentType,
        capabilities: Vec<String>,
    ) -> Result<String, TmuxError> {
        // 新しいペインを作成
        let output = Command::new("tmux")
            .args(["split-window", "-t", &self.session_name, "-P", "-F", "#{pane_id}"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // ペインレイアウトを調整
        Command::new("tmux")
            .args(["select-layout", "-t", &self.session_name, "tiled"])
            .output()
            .ok();

        // エージェントを起動
        let cmd = match agent_type {
            AgentType::ClaudeCode => "claude code",
            AgentType::Codex => "codex",
            AgentType::GenericShell => "bash",
        };

        self.send_keys(&pane_id, cmd)?;

        // ペイン情報を登録
        self.panes.insert(agent_id.to_string(), PaneInfo {
            pane_id: pane_id.clone(),
            agent_id: agent_id.to_string(),
            agent_type,
            capabilities,
            status: AgentStatus::Idle,
        });

        Ok(pane_id)
    }

    /// ペインにキー入力を送信
    pub fn send_keys(&self, pane_id: &str, text: &str) -> Result<(), TmuxError> {
        Command::new("tmux")
            .args(["send-keys", "-t", pane_id, text, "Enter"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        Ok(())
    }

    /// ペインの画面内容をキャプチャ
    pub fn capture_pane(&self, pane_id: &str) -> Result<String, TmuxError> {
        let output = Command::new("tmux")
            .args(["capture-pane", "-t", pane_id, "-p"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// エージェントの状態を検出（プロンプトパターンを探す）
    pub fn detect_status(&mut self, pane_id: &str) -> AgentStatus {
        if let Ok(content) = self.capture_pane(pane_id) {
            // Claude Codeのアイドル状態を検出
            if content.contains("> ") || content.contains("❯ ") {
                return AgentStatus::Idle;
            }
            if content.contains("Thinking...") || content.contains("Processing...") {
                return AgentStatus::Busy;
            }
        }
        AgentStatus::Unknown
    }

    /// 特定の能力を持つエージェントを検索
    pub fn discover_by_capability(&self, capability: &str) -> Vec<&PaneInfo> {
        self.panes.values()
            .filter(|p| p.capabilities.contains(&capability.to_string()))
            .collect()
    }

    /// メッセージをルーティング（ACPMessage → ペインへの送信）
    pub async fn route_message(&mut self, message: ACPMessage) -> Result<(), TmuxError> {
        let to_agent = &message.to;
        let pane = self.panes.get(to_agent)
            .ok_or_else(|| TmuxError::AgentNotFound(to_agent.clone()))?;

        // メッセージ内容をプロンプトとして送信
        self.send_keys(&pane.pane_id, &message.payload.content)?;

        // ステータス更新
        if let Some(p) = self.panes.get_mut(to_agent) {
            p.status = AgentStatus::Busy;
        }

        Ok(())
    }

    /// 全エージェントの状態を更新
    pub fn update_all_status(&mut self) {
        for (agent_id, pane) in self.panes.iter_mut() {
            pane.status = self.detect_status(&pane.pane_id);
        }
    }

    /// エージェントを終了
    pub fn kill_agent(&mut self, agent_id: &str) -> Result<(), TmuxError> {
        if let Some(pane) = self.panes.remove(agent_id) {
            Command::new("tmux")
                .args(["kill-pane", "-t", &pane.pane_id])
                .output()
                .ok();
        }
        Ok(())
    }

    /// セッションを終了
    pub fn destroy_session(&mut self) -> Result<(), TmuxError> {
        Command::new("tmux")
            .args(["kill-session", "-t", &self.session_name])
            .output()
            .ok();
        self.panes.clear();
        Ok(())
    }

    fn get_first_pane_id(&self) -> Result<String, TmuxError> {
        let output = Command::new("tmux")
            .args(["list-panes", "-t", &self.session_name, "-F", "#{pane_id}"])
            .output()
            .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

        Ok(String::from_utf8_lossy(&output.stdout).lines().next().unwrap_or("").to_string())
    }
}
```

---

## ACPMessage（シンプル化版）

```rust
// src-tauri/src/acp/message.rs

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// ACPメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ACPMessage {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub from: String,           // 送信元エージェントID
    pub to: String,             // 宛先エージェントID
    pub payload: MessagePayload,
    pub correlation_id: Option<String>,  // Request-Response相関
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePayload {
    pub content: String,        // プロンプトテキスト（そのままエージェントに送信）
    pub context: Option<SharedContext>,
}

/// 共有コンテキスト（エージェント間で共有）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SharedContext {
    pub conversation_summary: Option<String>,
    pub files: Vec<String>,
    pub metadata: serde_json::Value,
}
```

---

## エージェント間通信フロー

```
┌────────────────────────────────────────────────────────────────────┐
│                      TmuxOrchestrator                              │
│                                                                    │
│  User: "この字幕を翻訳して、品質チェックもして"                      │
│                                                                    │
│  Step 1: エージェント発見                                           │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │ translators = discover_by_capability("translation")          │  │
│  │ reviewers = discover_by_capability("quality-check")          │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                    │
│  Step 2: 翻訳依頼                                                   │
│  ┌─────────────────┐                                              │
│  │ pane: claude-0  │  send_keys: "以下を日本語に翻訳: ..."         │  │
│  │ [翻訳担当]       │ ─────────────────────────────────────────>  │  │
│  └─────────────────┘                                              │
│         │                                                          │
│         ▼ capture_pane + parse_output                              │
│  ┌─────────────────┐                                              │
│  │ 翻訳結果取得     │  "翻訳完了: ..."                             │  │
│  └─────────────────┘                                              │
│                                                                    │
│  Step 3: 品質チェック依頼（翻訳結果をコンテキストに含める）           │
│  ┌─────────────────┐                                              │
│  │ pane: claude-1  │  send_keys: "以下の翻訳を品質チェック: ..."   │  │
│  │ [品質担当]       │ ─────────────────────────────────────────>  │  │
│  └─────────────────┘                                              │
│         │                                                          │
│         ▼                                                          │
│  ┌─────────────────┐                                              │
│  │ 最終結果         │  "品質OK、完成"                              │  │
│  └─────────────────┘                                              │
└────────────────────────────────────────────────────────────────────┘
```

---

## 出力パーサー

各エージェントからの出力をパースして、状態を正確に検知する必要がある。

### 状態の定義

```rust
/// エージェントの詳細状態
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    /// 処理中
    Processing,

    /// タスク完了、次の指示待ち
    IdleReady,

    /// ユーザーへの質問待ち
    WaitingForInput { question: String },

    /// エラー発生
    Error { message: String },

    /// 不明
    Unknown,
}
```

### 状態検知の課題

Claude Codeの出力から以下を区別する必要がある:

| 状態 | 画面上の見た目 | 検知方法 |
|------|---------------|---------|
| 処理中 | "Thinking..." または出力が流れている | ストリーミング中 |
| 完了 | `> ` プロンプト表示 | + 出力が質問で終わっていない |
| 質問待ち | `> ` プロンプト表示 | + 直前の出力が `?` で終わる |
| エラー | `> ` プロンプト表示 | + "Error:" や "Failed:" を含む |

### 実装

```rust
// src-tauri/src/acp/parser.rs

use regex::Regex;

/// エージェント出力パーサー
pub struct OutputParser {
    /// プロンプトパターン
    prompt_patterns: Vec<Regex>,
    /// 質問パターン（最後の行が質問か判定）
    question_patterns: Vec<Regex>,
    /// エラーパターン
    error_patterns: Vec<Regex>,
    /// 処理中パターン
    processing_patterns: Vec<Regex>,
}

impl OutputParser {
    pub fn new() -> Self {
        Self {
            prompt_patterns: vec![
                Regex::new(r"(?m)^> $").unwrap(),
                Regex::new(r"(?m)^❯ $").unwrap(),
            ],
            question_patterns: vec![
                Regex::new(r"[？?]$").unwrap(),                    // 日本語/英語の疑問符
                Regex::new(r"\?$").unwrap(),
                Regex::new(r"ど[れの]|何[がを]|いかが|どう").unwrap(), // 日本語の疑問語
                Regex::new(r"(which|what|how|where|when|who)\s*\?$").unwrap(),
            ],
            error_patterns: vec![
                Regex::new(r"(?i)error:").unwrap(),
                Regex::new(r"(?i)failed:").unwrap(),
                Regex::new(r"(?i)exception:").unwrap(),
            ],
            processing_patterns: vec![
                Regex::new(r"Thinking").unwrap(),
                Regex::new(r"Processing").unwrap(),
                Regex::new(r"Working").unwrap(),
            ],
        }
    }

    /// 画面キャプチャから状態を判定
    pub fn parse(&self, content: &str) -> AgentState {
        // 1. まず処理中かチェック
        for pattern in &self.processing_patterns {
            if pattern.is_match(content) {
                return AgentState::Processing;
            }
        }

        // 2. プロンプトが表示されているか（入力待ち状態）
        let has_prompt = self.prompt_patterns.iter().any(|p| p.is_match(content));

        if has_prompt {
            // プロンプトの直前の内容を取得
            let last_output = self.extract_last_output(content);

            // 3. エラーチェック
            for pattern in &self.error_patterns {
                if pattern.is_match(&last_output) {
                    if let Some(m) = pattern.find(&last_output) {
                        return AgentState::Error {
                            message: m.as_str().to_string(),
                        };
                    }
                }
            }

            // 4. 質問チェック
            let trimmed = last_output.trim();
            for pattern in &self.question_patterns {
                if pattern.is_match(trimmed) {
                    return AgentState::WaitingForInput {
                        question: trimmed.to_string(),
                    };
                }
            }

            // 5. 完了と判定
            return AgentState::IdleReady;
        }

        // 6. まだ処理中
        AgentState::Processing
    }

    /// 最後のプロンプトの直前の出力を抽出
    fn extract_last_output(&self, content: &str) -> String {
        // 最後の "> " または "❯ " の前までを抽出
        if let Some(pos) = content.rfind("> ") {
            content[..pos].to_string()
        } else if let Some(pos) = content.rfind("❯ ") {
            content[..pos].to_string()
        } else {
            content.to_string()
        }
    }
}

impl Default for OutputParser {
    fn default() -> Self {
        Self::new()
    }
}
```

### 状態遷移図

```
┌─────────────────────────────────────────────────────────────────┐
│                        Agent State Machine                       │
│                                                                  │
│    ┌─────────────┐                                              │
│    │ Processing  │ ←─────────────────────────────────┐         │
│    └──────┬──────┘                                    │         │
│           │                                           │         │
│           ▼                                           │         │
│    ┌─────────────────────────────────────────┐        │         │
│    │         プロンプト表示検知               │        │         │
│    └─────────────────────────────────────────┘        │         │
│           │                                           │         │
│     ┌─────┴─────┬─────────────┐                       │         │
│     ▼           ▼             ▼                       │         │
│ ┌────────┐ ┌──────────┐ ┌───────────┐                 │         │
│ │ Error  │ │Question? │ │IdleReady  │                 │         │
│ └────────┘ └────┬─────┘ └─────┬─────┘                 │         │
│                 │             │                       │         │
│                 │ 回答送信     │ 新しいタスク送信       │         │
│                 │             └─────────────────────────┘         │
│                 ▼                                               │
│          ┌─────────────┐                                       │
│          │ Processing  │                                       │
│          └─────────────┘                                       │
└─────────────────────────────────────────────────────────────────┘
```

### 質問待ち状態の処理

```rust
// src-tauri/src/acp/orchestrator.rs

impl TmuxOrchestrator {
    /// エージェントの状態を確認し、必要に応じて自動応答または通知
    pub async fn check_and_handle_state(&mut self, agent_id: &str) -> Result<AgentState, TmuxError> {
        let pane = self.panes.get(agent_id)
            .ok_or_else(|| TmuxError::AgentNotFound(agent_id.to_string()))?;

        let content = self.capture_pane(&pane.pane_id)?;
        let state = self.parser.parse(&content);

        match &state {
            AgentState::WaitingForInput { question } => {
                // フロントエンドに通知（ユーザーに回答を促す）
                self.notify_question(agent_id, question);
            }
            AgentState::IdleReady => {
                // タスク完了を通知
                self.notify_complete(agent_id);
            }
            AgentState::Error { message } => {
                // エラーを通知
                self.notify_error(agent_id, message);
            }
            _ => {}
        }

        // ペインの状態を更新
        if let Some(p) = self.panes.get_mut(agent_id) {
            p.state = state.clone();
        }

        Ok(state)
    }

    /// ユーザーからの回答をエージェントに送信
    pub async fn answer_question(&mut self, agent_id: &str, answer: &str) -> Result<(), TmuxError> {
        let pane = self.panes.get(agent_id)
            .ok_or_else(|| TmuxError::AgentNotFound(agent_id.to_string()))?;

        self.send_keys(&pane.pane_id, answer)?;

        if let Some(p) = self.panes.get_mut(agent_id) {
            p.state = AgentState::Processing;
        }

        Ok(())
    }
}
```

### Tauriコマンド（追加）

```rust
/// エージェントの状態を確認
#[tauri::command]
fn acp_check_state(state: State<AppState>, agent_id: String) -> Result<AgentState, String> {
    let mut orch = state.orchestrator.lock();
    if let Some(ref mut o) = *orch {
        // 非同期で実行したいが、同期APIの場合はブロックする
        // 実際は async にする必要あり
        o.check_state(&agent_id).map_err(|e| e.to_string())
    } else {
        Err("Session not created".to_string())
    }
}

/// 質問に回答
#[tauri::command]
async fn acp_answer(state: State<'_, AppState>, agent_id: String, answer: String) -> Result<(), String> {
    let mut orch = state.orchestrator.lock();
    if let Some(ref mut o) = *orch {
        o.answer_question(&agent_id, &answer).await.map_err(|e| e.to_string())
    } else {
        Err("Session not created".to_string())
    }
}
```

### TypeScript 側の処理

```typescript
// src/acp/client.ts

export type AgentState =
  | { type: "processing" }
  | { type: "idle" }
  | { type: "waiting"; question: string }
  | { type: "error"; message: string };

export class ACPClient {
  // 状態監視（ポーリング）
  watchState(
    agentId: string,
    handler: (state: AgentState) => void,
    intervalMs = 500
  ): () => void {
    const interval = setInterval(async () => {
      try {
        const state = await invoke<AgentState>("acp_check_state", { agentId });
        handler(state);
      } catch (e) {
        console.error("State check error:", e);
      }
    }, intervalMs);

    return () => clearInterval(interval);
  }

  // 質問に回答
  async answer(agentId: string, answer: string): Promise<void> {
    await invoke("acp_answer", { agentId, answer });
  }
}

// 使用例
const acp = new ACPClient();
await acp.send("translator", "以下を翻訳: Hello, World!");

const unsubscribe = acp.watchState("translator", async (state) => {
  switch (state.type) {
    case "waiting":
      // UI に質問を表示
      showQuestionToUser(state.question);
      break;

    case "idle":
      // タスク完了
      const output = await acp.readOutput("translator");
      console.log("完了:", output);
      unsubscribe();
      break;

    case "error":
      console.error("エラー:", state.message);
      unsubscribe();
      break;
  }
});

// ユーザーが回答したら
async function onUserAnswer(answer: string) {
  await acp.answer("translator", answer);
}
```

---

## 質問の振り分け（AI回答 vs 人間回答）

質問が発生した際、**AIが回答できるか、人間の判断が必要か**を判定する仕組みが必要。

### アーキテクチャ

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Re-Voice App                                │
│                                                                     │
│  ┌──────────────┐     ┌──────────────────────────────────────────┐ │
│  │ Worker Agent │     │          Orchestrator Agent              │ │
│  │ (翻訳担当)    │     │  (まとめ役 = 質問分類 & 回答 or エスカレーション) │ │
│  └──────┬───────┘     └──────────────────────────────────────────┘ │
│         │                            │                              │
│         │ 質問発生                   │                              │
│         │ "どの日本語訳を使いますか？  │                              │
│         │  1. こんにちは              │                              │
│         │  2. やあ"                   │                              │
│         └────────────────────────────>│                              │
│                                      │                              │
│                              ┌───────┴───────┐                      │
│                              │ 質問分類       │                      │
│                              └───────┬───────┘                      │
│                                      │                              │
│                    ┌─────────────────┼─────────────────┐            │
│                    ▼                 ▼                 ▼            │
│            ┌──────────────┐  ┌──────────────┐  ┌──────────────┐    │
│            │ AUTO         │  │ SEMI_AUTO    │  │ HUMAN        │    │
│            │ (AIが回答)    │  │ (AIが提案    │  │ (人間のみ)    │    │
│            │              │  │  人間が確認)  │  │              │    │
│            └──────┬───────┘  └──────┬───────┘  └──────┬───────┘    │
│                   │                 │                 │            │
│                   ▼                 ▼                 ▼            │
│            自動回答           AI回答+人間確認      人間に通知        │
│                   │                 │                 │            │
│                   └─────────────────┴─────────────────┘            │
│                                     │                              │
│                                     ▼                              │
│                              Worker Agent                          │
│                              (回答を受信)                           │
└─────────────────────────────────────────────────────────────────────┘
```

### 質問分類

```rust
// src-tauri/src/acp/question_classifier.rs

/// 質問の種類
#[derive(Debug, Clone, PartialEq)]
pub enum QuestionType {
    /// AIが自動で回答可能
    AutoAnswerable,

    /// AIが回答を提案、人間が確認
    SemiAuto,

    /// 人間の判断が必須
    HumanOnly,

    /// 不明（人間に確認）
    Unknown,
}

/// 質問分類器
pub struct QuestionClassifier {
    /// 自動回答可能なパターン
    auto_patterns: Vec<Regex>,
    /// 人間必須パターン
    human_patterns: Vec<Regex>,
}

impl QuestionClassifier {
    pub fn new() -> Self {
        Self {
            auto_patterns: vec![
                // 技術的な選択（AIが判断可能）
                Regex::new(r"どの(ファイル|パス|オプション|設定)").unwrap(),
                Regex::new(r"(format|method|approach).*\?").unwrap(),
                Regex::new(r"continue\?").unwrap(),
            ],
            human_patterns: vec![
                // 人間の好み・判断が必要
                Regex::new(r"どの(スタイル|トーン|雰囲気|好み)").unwrap(),
                Regex::new(r"好きな|お好み").unwrap(),
                // 権限・承認が必要
                Regex::new(r"(承認|許可|確認).*必要").unwrap(),
                Regex::new(r"(削除|上書き|実行).*[？?]").unwrap(),
                // 予算・リソース
                Regex::new(r"(予算|コスト|費用)").unwrap(),
            ],
        }
    }

    /// 質問を分類
    pub fn classify(&self, question: &str) -> QuestionType {
        // 人間必須を先にチェック
        for pattern in &self.human_patterns {
            if pattern.is_match(question) {
                return QuestionType::HumanOnly;
            }
        }

        // 自動回答可能をチェック
        for pattern in &self.auto_patterns {
            if pattern.is_match(question) {
                return QuestionType::AutoAnswerable;
            }
        }

        // その他は人間に確認（安全側）
        QuestionType::SemiAuto
    }
}
```

### Orchestrator Agent（まとめ役）

```rust
// src-tauri/src/acp/orchestrator.rs

/// オーケストレーター（まとめ役AI）
pub struct OrchestratorAgent {
    /// 質問分類器
    classifier: QuestionClassifier,
    /// 自身のClaude Codeインスタンス（質問分類・回答生成用）
    claude_pane: Option<String>,
    /// Worker Agent一覧
    workers: HashMap<String, WorkerInfo>,
}

impl OrchestratorAgent {
    /// Workerからの質問を処理
    pub async fn handle_question(
        &mut self,
        worker_id: &str,
        question: &str,
    ) -> Result<QuestionHandling, OrchestratorError> {
        let qtype = self.classifier.classify(question);

        match qtype {
            QuestionType::AutoAnswerable => {
                // AIが回答を生成
                let answer = self.generate_answer(question).await?;
                self.send_answer(worker_id, &answer).await?;

                Ok(QuestionHandling::AutoAnswered { answer })
            }

            QuestionType::SemiAuto => {
                // AIが回答を提案
                let suggestion = self.generate_answer(question).await?;

                // 人間に確認
                let human_response = self.ask_human(question, &suggestion).await?;

                match human_response {
                    HumanResponse::Approved => {
                        self.send_answer(worker_id, &suggestion).await?;
                        Ok(QuestionHandling::SemiAutoApproved { suggestion })
                    }
                    HumanResponse::Modified(modified) => {
                        self.send_answer(worker_id, &modified).await?;
                        Ok(QuestionHandling::SemiAutoModified { suggestion, modified })
                    }
                    HumanResponse::Custom(answer) => {
                        self.send_answer(worker_id, &answer).await?;
                        Ok(QuestionHandling::HumanAnswered { answer })
                    }
                }
            }

            QuestionType::HumanOnly => {
                // 人間に直接質問
                let answer = self.ask_human_direct(question).await?;
                self.send_answer(worker_id, &answer).await?;

                Ok(QuestionHandling::HumanOnlyAnswered { answer })
            }

            QuestionType::Unknown => {
                // 安全のため人間に確認
                let answer = self.ask_human_direct(question).await?;
                self.send_answer(worker_id, &answer).await?;

                Ok(QuestionHandling::HumanOnlyAnswered { answer })
            }
        }
    }

    /// AIが回答を生成
    async fn generate_answer(&self, question: &str) -> Result<String, OrchestratorError> {
        // Orchestrator自身のClaude Codeに質問を投げて回答を得る
        let prompt = format!(
            "以下の質問に対して、最も適切だと思われる回答を1つ選んでください。\n\
             短く回答してください。\n\n\
             質問: {}\n\n\
             回答:",
            question
        );

        // Claude Codeに送信して回答を取得
        // (実装は TmuxOrchestrator 経由で行う)
        todo!("Claude Codeへの送信・回答取得")
    }

    /// 人間に確認（提案付き）
    async fn ask_human(
        &self,
        question: &str,
        suggestion: &str,
    ) -> Result<HumanResponse, OrchestratorError> {
        // フロントエンドに通知して応答を待つ
        // Tauriイベントでフロントエンドに送信
        todo!("人間への確認通知と応答待機")
    }

    /// 人間に直接質問
    async fn ask_human_direct(&self, question: &str) -> Result<String, OrchestratorError> {
        // フロントエンドに通知して応答を待つ
        todo!("人間への質問通知と応答待機")
    }
}

/// 質問処理結果
#[derive(Debug, Clone)]
pub enum QuestionHandling {
    AutoAnswered { answer: String },
    SemiAutoApproved { suggestion: String },
    SemiAutoModified { suggestion: String, modified: String },
    HumanAnswered { answer: String },
    HumanOnlyAnswered { answer: String },
}

/// 人間の応答
#[derive(Debug, Clone)]
pub enum HumanResponse {
    Approved,
    Modified(String),
    Custom(String),
}
```

### Tauriコマンド（追加）

```rust
/// 質問の分類
#[tauri::command]
fn acp_classify_question(question: String) -> Result<String, String> {
    let classifier = QuestionClassifier::new();
    let qtype = classifier.classify(&question);
    Ok(format!("{:?}", qtype))
}

/// Orchestratorが質問を処理
#[tauri::command]
async fn acp_handle_question(
    state: State<'_, AppState>,
    worker_id: String,
    question: String,
) -> Result<QuestionHandling, String> {
    let mut orch = state.orchestrator.lock();
    if let Some(ref mut o) = *orch {
        o.handle_question(&worker_id, &question).await
            .map_err(|e| e.to_string())
    } else {
        Err("Session not created".to_string())
    }
}

/// 人間が質問に回答（フロントエンドから呼ばれる）
#[tauri::command]
async fn acp_human_answer(
    state: State<'_, AppState>,
    question_id: String,
    answer: String,
) -> Result<(), String> {
    // 保留中の質問に回答をセット
    let orch = state.orchestrator.lock();
    if let Some(ref o) = *orch {
        o.submit_human_answer(&question_id, &answer)
            .map_err(|e| e.to_string())
    } else {
        Err("Session not created".to_string())
    }
}
```

### TypeScript 側の処理

```typescript
// src/acp/client.ts

export type QuestionType = "auto" | "semi-auto" | "human" | "unknown";

export interface Question {
  id: string;
  workerId: string;
  question: string;
  type: QuestionType;
  suggestion?: string; // AIからの提案
}

export class ACPClient {
  private pendingQuestions: Map<string, (answer: string) => void> = new Map();

  /// 質問を処理（AI or 人間への振り分け含む）
  async handleQuestion(workerId: string, question: string): Promise<void> {
    const result = await invoke<QuestionHandling>("acp_handle_question", {
      workerId,
      question,
    });

    switch (result.type) {
      case "AutoAnswered":
        // AIが自動回答済み、何もしない
        break;

      case "SemiAutoApproved":
        // AIが提案して自動承認済み
        break;

      case "SemiAutoModified":
      case "HumanAnswered":
      case "HumanOnlyAnswered":
        // 既に回答済み
        break;
    }
  }

  /// 人間が質問に回答
  async submitAnswer(questionId: string, answer: string): Promise<void> {
    await invoke("acp_human_answer", { questionId, answer });
  }

  /// 質問イベントを監視
  onQuestion(
    handler: (q: Question) => Promise<string>
  ): () => void {
    // Tauriイベントリスナーを設定
    const unlisten = listen<Question>("acp:question", async (event) => {
      const answer = await handler(event.payload);
      await this.submitAnswer(event.payload.id, answer);
    });

    return () => { unlisten.then(f => f()); };
  }
}

// 使用例
const acp = new ACPClient();

// 質問イベントを監視
acp.onQuestion(async (q) => {
  if (q.suggestion) {
    // AIからの提案がある場合
    const userChoice = await showConfirmDialog({
      title: "確認",
      message: q.question,
      suggestion: q.suggestion,
      options: ["提案を採用", "修正して回答", "自分で回答"],
    });

    switch (userChoice) {
      case "提案を採用":
        return q.suggestion;
      case "修正して回答":
        return await showInputWithDefault(q.suggestion);
      case "自分で回答":
        return await showInput();
    }
  } else {
    // 人間のみ回答
    return await showInput({ message: q.question });
  }
});
```
```

---

## Tauriコマンド

```rust
// src-tauri/src/lib.rs に追加

mod acp;

use acp::tmux::TmuxOrchestrator;
use acp::message::ACPMessage;
use parking_lot::Mutex;
use std::sync::Arc;

pub struct AppState {
    pty: Arc<Mutex<PtyManager>>,
    orchestrator: Arc<Mutex<Option<TmuxOrchestrator>>>,
}

/// ACP: tmuxセッションを作成
#[tauri::command]
fn acp_create_session(state: State<AppState>) -> Result<(), String> {
    let mut orch = state.orchestrator.lock();
    let mut orchestrator = TmuxOrchestrator::new("revoice");
    orchestrator.create_session().map_err(|e| e.to_string())?;
    *orch = Some(orchestrator);
    Ok(())
}

/// ACP: エージェントを起動
#[tauri::command]
fn acp_spawn_agent(
    state: State<AppState>,
    agent_id: String,
    agent_type: String,
    capabilities: Vec<String>,
) -> Result<String, String> {
    let mut orch = state.orchestrator.lock();
    if let Some(ref mut o) = *orch {
        let atype = match agent_type.as_str() {
            "claude-code" => AgentType::ClaudeCode,
            "codex" => AgentType::Codex,
            _ => AgentType::GenericShell,
        };
        o.spawn_agent(&agent_id, atype, capabilities).map_err(|e| e.to_string())
    } else {
        Err("Session not created".to_string())
    }
}

/// ACP: エージェント一覧を取得
#[tauri::command]
fn acp_list_agents(state: State<AppState>) -> Result<Vec<AgentInfo>, String> {
    let orch = state.orchestrator.lock();
    if let Some(ref o) = *orch {
        Ok(o.list_agents())
    } else {
        Ok(vec![])
    }
}

/// ACP: メッセージを送信
#[tauri::command]
async fn acp_send(state: State<'_, AppState>, message: ACPMessage) -> Result<(), String> {
    let mut orch = state.orchestrator.lock();
    if let Some(ref mut o) = *orch {
        o.route_message(message).await.map_err(|e| e.to_string())
    } else {
        Err("Session not created".to_string())
    }
}

/// ACP: エージェントの出力を取得
#[tauri::command]
fn acp_read_output(state: State<AppState>, agent_id: String) -> Result<String, String> {
    let orch = state.orchestrator.lock();
    if let Some(ref o) = *orch {
        if let Some(pane) = o.get_pane(&agent_id) {
            o.capture_pane(&pane.pane_id).map_err(|e| e.to_string())
        } else {
            Err("Agent not found".to_string())
        }
    } else {
        Err("Session not created".to_string())
    }
}

/// ACP: 能力でエージェントを検索
#[tauri::command]
fn acp_discover(state: State<AppState>, capability: String) -> Result<Vec<AgentInfo>, String> {
    let orch = state.orchestrator.lock();
    if let Some(ref o) = *orch {
        Ok(o.discover_by_capability(&capability)
            .into_iter()
            .map(|p| AgentInfo::from(p))
            .collect())
    } else {
        Ok(vec![])
    }
}
```

---

## TypeScript クライアント

```typescript
// src/acp/client.ts

import { invoke } from "@tauri-apps/api/core";

export interface AgentInfo {
  id: string;
  type: "claude-code" | "codex" | "shell";
  capabilities: string[];
  status: "idle" | "busy" | "error" | "unknown";
}

export interface ACPMessage {
  id: string;
  from: string;
  to: string;
  content: string;
  correlationId?: string;
}

export class ACPClient {
  private pollInterval: number | null = null;

  /// セッション作成
  async createSession(): Promise<void> {
    await invoke("acp_create_session");
  }

  /// エージェント起動
  async spawnAgent(
    id: string,
    type: "claude-code" | "codex" | "shell",
    capabilities: string[]
  ): Promise<string> {
    return invoke("acp_spawn_agent", { agentId: id, agentType: type, capabilities });
  }

  /// エージェント一覧
  async listAgents(): Promise<AgentInfo[]> {
    return invoke("acp_list_agents");
  }

  /// 能力検索
  async discover(capability: string): Promise<AgentInfo[]> {
    return invoke("acp_discover", { capability });
  }

  /// メッセージ送信
  async send(to: string, content: string): Promise<void> {
    const message: ACPMessage = {
      id: crypto.randomUUID(),
      from: "user",
      to,
      content,
    };
    await invoke("acp_send", { message });
  }

  /// 出力取得
  async readOutput(agentId: string): Promise<string> {
    return invoke("acp_read_output", { agentId });
  }

  /// 出力監視開始
  subscribe(agentId: string, handler: (output: string) => void, intervalMs = 1000): () => void {
    this.pollInterval = window.setInterval(async () => {
      try {
        const output = await this.readOutput(agentId);
        handler(output);
      } catch (e) {
        console.error("Poll error:", e);
      }
    }, intervalMs);

    return () => {
      if (this.pollInterval) {
        clearInterval(this.pollInterval);
      }
    };
  }
}
```

---

## 使用例

```typescript
const acp = new ACPClient();

// セッション作成
await acp.createSession();

// エージェント起動
await acp.spawnAgent("translator", "claude-code", ["translation", "japanese"]);
await acp.spawnAgent("reviewer", "claude-code", ["quality-check"]);

// 翻訳依頼
await acp.send("translator", "以下を日本語に翻訳してください:\n\nHello, World!");

// 出力監視
const unsubscribe = acp.subscribe("translator", (output) => {
  console.log("Output:", output);
  // 完了を検知したら次のステップへ
  if (output.includes("> ")) {
    // 品質チェックへ
    acp.send("reviewer", `以下の翻訳をチェック:\n\n${extractTranslation(output)}`);
    unsubscribe();
  }
});
```

---

## 代替案の比較

| 手法 | メリット | デメリット |
|------|---------|-----------|
| **tmux統合** | 視覚的デバッグ可能、セッション永続化、標準ツール | tmux依存、プラットフォーム制限 |
| **複数PTY** | tmux不要、軽量 | レイアウト管理なし、デバッグ困難 |
| **単一インスタンス + プロンプト** | 最もシンプル | 並列処理不可、コンテキスト制限 |

---

## プロンプト内マルチエージェント

シンプルなタスクでは、1つのClaude Code内で複数の役割を演じさせることができる。

### 仕組み

```
┌─────────────────────────────────────────────────────────────┐
│                  Claude Code Instance                        │
│                                                              │
│  プロンプト:                                                  │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ あなたは以下の3つの役割を順番に果たしてください:           ││
│  │                                                          ││
│  │ 1. 翻訳者: 英語→日本語に翻訳                              ││
│  │ 2. レビュアー: 翻訳品質をチェック                          ││
│  │ 3. 編集者: 最終調整                                       ││
│  │                                                          ││
│  │ 入力テキスト: Hello, World!                               ││
│  │                                                          ││
│  │ 各役割の結果を以下の形式で出力してください:                ││
│  │ [翻訳者] ...                                              ││
│  │ [レビュアー] ...                                          ││
│  │ [編集者] ...                                              ││
│  │ [最終結果] ...                                            ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  出力:                                                       │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ [翻訳者] こんにちは、世界！                               ││
│  │ [レビュアー] 品質OK、問題なし                              ││
│  │ [編集者] 修正不要                                         ││
│  │ [最終結果] こんにちは、世界！                              ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

### 実装

```rust
// src-tauri/src/acp/prompt_agent.rs

/// プロンプト内マルチエージェント
pub struct PromptMultiAgent {
    roles: Vec<AgentRole>,
}

#[derive(Debug, Clone)]
pub struct AgentRole {
    pub name: String,
    pub description: String,
    pub instructions: String,
}

impl PromptMultiAgent {
    pub fn new() -> Self {
        Self {
            roles: Vec::new(),
        }
    }

    /// 役割を追加
    pub fn add_role(&mut self, role: AgentRole) {
        self.roles.push(role);
    }

    /// マルチエージェントプロンプトを生成
    pub fn build_prompt(&self, task: &str) -> String {
        let mut prompt = String::new();

        prompt.push_str("あなたは以下の役割を順番に果たしてください:\n\n");

        for (i, role) in self.roles.iter().enumerate() {
            prompt.push_str(&format!(
                "{}. {}: {}\n   指示: {}\n\n",
                i + 1, role.name, role.description, role.instructions
            ));
        }

        prompt.push_str("---\n\n");
        prompt.push_str(&format!("タスク: {}\n\n", task));
        prompt.push_str("各役割の結果を以下の形式で出力してください:\n");

        for role in &self.roles {
            prompt.push_str(&format!("[{}] ...\n", role.name));
        }
        prompt.push_str("[最終結果] ...\n");

        prompt
    }
}

impl Default for PromptMultiAgent {
    fn default() -> Self {
        Self::new()
    }
}
```

### 使用例

```rust
// 翻訳→品質チェックのパイプライン
let mut agent = PromptMultiAgent::new();

agent.add_role(AgentRole {
    name: "翻訳者".to_string(),
    description: "英語を日本語に翻訳する".to_string(),
    instructions: "自然な日本語に翻訳してください".to_string(),
});

agent.add_role(AgentRole {
    name: "レビュアー".to_string(),
    description: "翻訳品質をチェックする".to_string(),
    instructions: "誤訳や不自然な表現があれば指摘してください".to_string(),
});

let prompt = agent.build_prompt("Hello, World! This is a test.");
// このプロンプトをClaude Codeに送信
```

---

## TaskRouter（タスク振り分け）

シンプルなタスクと複雑なタスクを自動で振り分ける。

```rust
// src-tauri/src/acp/router.rs

/// タスクの複雑さを判定してルーティング
pub struct TaskRouter {
    /// シンプルタスクの判定閾値
    simple_threshold: usize,
}

impl TaskRouter {
    pub fn new() -> Self {
        Self {
            simple_threshold: 500, // 文字数
        }
    }

    /// タスクの種類を判定
    pub fn classify(&self, task: &str) -> TaskType {
        let char_count = task.chars().count();

        // 複雑さの判定基準
        let has_multiple_steps = task.contains("して、") || task.contains("その後");
        let is_long = char_count > self.simple_threshold;
        let needs_parallel = task.contains("同時に") || task.contains("並列");

        if needs_parallel || (has_multiple_steps && is_long) {
            TaskType::Complex
        } else {
            TaskType::Simple
        }
    }

    /// 適切なハンドラにルーティング
    pub async fn route(
        &self,
        task: &str,
        prompt_agent: &mut PromptMultiAgent,
        tmux_orchestrator: &mut TmuxOrchestrator,
    ) -> Result<String, RouterError> {
        match self.classify(task) {
            TaskType::Simple => {
                // プロンプト内マルチエージェントで処理
                let prompt = prompt_agent.build_prompt(task);
                // 単一Claude Codeに送信
                todo!("単一エージェントに送信")
            }
            TaskType::Complex => {
                // tmuxで複数エージェントを起動して処理
                todo!("tmuxオーケストレーターで処理")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskType {
    Simple,   // プロンプト内で完結
    Complex,  // tmuxマルチエージェント必要
}
```

---

## レビュー指摘事項と改善案

### 高優先度（実装前に必須）

#### 1. コマンドインジェクション対策

**問題:** 現在の実装はシェルインジェクションに対して脆弱

```rust
// 危険な実装
Command::new("sh")
    .arg("-c")
    .arg(format!("yt-dlp --list-subs \"{}\"", url))  // インジェクション可能
```

**改善案:**

```rust
// 安全な実装
fn validate_url(url: &str) -> Result<String, ValidationError> {
    let parsed = url::Url::parse(url)?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err(ValidationError::InvalidScheme);
    }
    // YouTubeのドメインのみ許可
    let host = parsed.host_str().ok_or(ValidationError::InvalidDomain)?;
    if !host.ends_with("youtube.com") && !host.ends_with("youtu.be") {
        return Err(ValidationError::InvalidDomain);
    }
    Ok(url.to_string())
}

// シェルを経由しない
let output = std::process::Command::new("yt-dlp")
    .arg("--list-subs")
    .arg(&validated_url)
    .env("PATH", &extended_path)
    .output()
    .map_err(|e| e.to_string())?;
```

**tmuxコマンドの安全化:**

```rust
fn send_keys(&self, pane_id: &str, text: &str) -> Result<(), TmuxError> {
    // ペインIDの形式チェック
    if !pane_id.starts_with('%') || !pane_id[1..].chars().all(|c| c.is_numeric()) {
        return Err(TmuxError::InvalidPaneId);
    }

    // 入力のサニタイズ
    let sanitized = self.sanitize_tmux_input(text);

    Command::new("tmux")
        .args(["send-keys", "-t", pane_id, "-l", &sanitized])  // -l でリテラルモード
        .output()
        .map_err(|e| TmuxError::CommandFailed(e.to_string()))?;

    Ok(())
}
```

#### 2. タイムアウト処理

**問題:** タイムアウトが未定義

**改善案:**

```rust
pub struct TimeoutConfig {
    pub agent_startup: Duration,     // 30秒
    pub task_execution: Duration,    // 5分
    pub question_response: Duration, // 24時間
    pub idle_timeout: Duration,      // 1時間
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            agent_startup: Duration::from_secs(30),
            task_execution: Duration::from_secs(300),
            question_response: Duration::from_secs(86400),
            idle_timeout: Duration::from_secs(3600),
        }
    }
}

impl TmuxOrchestrator {
    pub async fn monitor_timeouts(&mut self) {
        loop {
            for (agent_id, pane) in &self.panes {
                if let Some(task_started) = pane.task_started_at {
                    if Instant::now().duration_since(task_started) > self.config.task_execution {
                        self.handle_timeout(agent_id).await;
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    async fn handle_timeout(&mut self, agent_id: &str) {
        // 1. 現在のタスクをキャンセル（Ctrl+C）
        self.send_keys(&self.panes[agent_id].pane_id, "\x03");

        // 2. 状態をTimeoutに変更
        if let Some(p) = self.panes.get_mut(agent_id) {
            p.state = AgentState::Timeout {
                task_id: p.current_task_id.clone().unwrap_or_default(),
                elapsed_seconds: 300,
            };
        }

        // 3. ユーザーに通知
        self.notify_timeout(agent_id);
    }
}
```

#### 3. エラー分類とリカバリー

**問題:** エラーの重要度分類がない

**改善案:**

```rust
pub enum ErrorSeverity {
    /// 自動リトライ可能
    Transient { max_retries: u32, backoff: BackoffStrategy },
    /// Orchestratorでの処理が必要
    RequiresOrchestrator,
    /// 人間の介入が必要
    RequiresHumanIntervention,
    /// 致命的（セッション終了）
    Fatal,
}

pub enum BackoffStrategy {
    Fixed { delay_ms: u64 },
    Exponential { initial_ms: u64, max_ms: u64 },
    None,
}

impl TmuxOrchestrator {
    pub async fn handle_error(
        &mut self,
        agent_id: &str,
        error: &AgentError,
    ) -> Result<RecoveryAction, OrchestratorError> {
        match error.severity {
            ErrorSeverity::Transient { max_retries, backoff } => {
                if self.retry_count(agent_id) < max_retries {
                    self.apply_backoff(backoff);
                    self.retry_last_task(agent_id).await
                } else {
                    self.escalate_to_orchestrator(agent_id, error).await
                }
            }
            ErrorSeverity::RequiresOrchestrator => {
                self.escalate_to_orchestrator(agent_id, error).await
            }
            ErrorSeverity::RequiresHumanIntervention => {
                self.notify_human(agent_id, error).await
            }
            ErrorSeverity::Fatal => {
                self.graceful_shutdown(agent_id).await
            }
        }
    }
}
```

#### 4. 非同期Mutex設計

**問題:** `parking_lot::Mutex` を保持したまま `.await` でデッドロックリスク

```rust
// 問題のあるコード
let mut orch = state.orchestrator.lock();  // 同期Mutex
o.handle_question(&worker_id, &question).await  // 非同期メソッド
```

**改善案:**

```rust
// tokio::sync::Mutex を使用
use tokio::sync::Mutex;

pub struct AppState {
    pty: Arc<Mutex<PtyManager>>,
    orchestrator: Arc<Mutex<Option<TmuxOrchestrator>>>,
}

// Tauriコマンド
#[tauri::command]
async fn acp_handle_question(
    state: State<'_, AppState>,
    worker_id: String,
    question: String,
) -> Result<QuestionHandling, String> {
    let mut orch = state.orchestrator.lock().await;  // 非同期ロック
    if let Some(ref mut o) = *orch {
        o.handle_question(&worker_id, &question).await
            .map_err(|e| e.to_string())
    } else {
        Err("Session not created".to_string())
    }
}
```

### 中優先度

#### 5. 状態管理の拡充

**追加すべき状態:**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    /// 起動中
    Initializing,
    /// 処理中
    Processing { started_at: DateTime<Utc> },
    /// タスク完了、次の指示待ち
    IdleReady { last_task_id: Option<String> },
    /// ユーザーへの質問待ち
    WaitingForInput {
        question: String,
        question_id: String,
        asked_at: DateTime<Utc>,
    },
    /// エラー発生
    Error {
        message: String,
        error_code: Option<String>,
        recoverable: bool,
    },
    /// タイムアウト
    Timeout {
        task_id: String,
        elapsed_seconds: u64,
    },
    /// 復旧中
    Recovering,
    /// 終了処理中
    ShuttingDown,
    /// 不明
    Unknown,
}
```

#### 6. TaskRouterの判定基準改善

```rust
pub struct TaskAnalysis {
    pub task_type: TaskCategory,
    pub estimated_complexity: Complexity,
    pub required_capabilities: Vec<String>,
    pub dependencies: Vec<TaskDependency>,
    pub parallelizable: bool,
}

pub enum TaskCategory {
    Translation,
    QualityCheck,
    CodeGeneration,
    VideoProcessing,
    MultiStep,
}

pub enum Complexity {
    Simple,   // 単一ステップ、短いテキスト
    Medium,   // 複数ステップ、通常のテキスト
    Complex,  // 並列処理必要、長いテキスト
}

impl TaskRouter {
    pub fn analyze(&self, task: &str) -> TaskAnalysis {
        let mut analysis = TaskAnalysis::default();

        // 1. タスクタイプを判定
        analysis.task_type = self.detect_category(task);

        // 2. 複雑さを推定
        analysis.estimated_complexity = self.estimate_complexity(task);

        // 3. 必要な能力を抽出
        analysis.required_capabilities = self.extract_capabilities(task);

        // 4. 並列化可能性を判定
        analysis.parallelizable = self.can_parallelize(task);

        analysis
    }
}
```

#### 7. 入力バリデーション

```rust
pub struct InputValidator {
    max_length: usize,
}

impl InputValidator {
    pub fn validate(&self, input: &str) -> Result<ValidatedInput, ValidationError> {
        // 1. 長さチェック
        if input.len() > self.max_length {
            return Err(ValidationError::TooLong);
        }

        // 2. 制御文字のサニタイズ
        let sanitized: String = input
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect();

        Ok(ValidatedInput { content: sanitized })
    }
}
```

#### 8. リソース管理

```rust
pub struct ResourceManager {
    max_agents: usize,
    current_usage: ResourceUsage,
    limits: ResourceLimits,
}

pub struct ResourceLimits {
    max_concurrent_agents: usize,  // 例: 5
    max_memory_per_agent: u64,     // 例: 512MB
}

impl TmuxOrchestrator {
    pub fn can_spawn_agent(&self) -> bool {
        self.resource_manager.current_usage.agent_count
            < self.resource_manager.limits.max_concurrent_agents
    }
}
```

---

## 推奨アプローチ

**採用: tmux + プロンプト内マルチエージェント（ハイブリッド）**

```
タスク受信
    │
    ▼
TaskRouter.classify()
    │
    ├── シンプル → PromptMultiAgent（単一Claude Code）
    │
    └── 複雑 → TmuxOrchestrator（複数ペイン）
```

### 実装フェーズ（小さく作るアプローチ）

#### Level 0: 基盤（1-2日）
**目標: tmuxで単一のClaude Codeを起動できる**

```
┌─────────────────────────────────────────┐
│ Re-Voice App                            │
│  ┌─────────────────────────────────────┐│
│  │ tmux session作成                    ││
│  │ claude code 起動                    ││
│  │ "Hello" 送信                        ││
│  │ 画面キャプチャで確認                 ││
│  └─────────────────────────────────────┘│
└─────────────────────────────────────────┘
```

**実装内容:**
- `TmuxOrchestrator::create_session()`
- `TmuxOrchestrator::spawn_agent()`
- `TmuxOrchestrator::send_keys()`
- `TmuxOrchestrator::capture_pane()`

**検証方法:**
```bash
# tmux attach で実際にClaude Codeが見える
tmux attach -t revoice
```

**ファイル:**
- `src-tauri/src/acp/mod.rs`
- `src-tauri/src/acp/tmux.rs`

---

#### Level 1: 状態検知（1-2日）
**目標: Claude Codeの状態（処理中/完了/質問中）を検知できる**

```
処理中 → "Thinking..." → AgentState::Processing
完了   → "> "         → AgentState::IdleReady
質問   → "> ?"        → AgentState::WaitingForInput
```

**実装内容:**
- `OutputParser::parse()`
- `AgentState` enum
- ポーリングでの状態監視

**検証方法:**
- Claude Codeに質問させるプロンプトを送信
- WaitingForInput が検知できるか確認

**ファイル:**
- `src-tauri/src/acp/parser.rs`
- `src-tauri/src/acp/state.rs`

---

#### Level 2: フロントエンド連携（1日）
**目標: React UIからClaude Codeを操作できる**

```
┌─────────────────────────────────────────┐
│ React UI                                │
│  [メッセージ入力] [送信]                 │
│  ┌─────────────────────────────────────┐│
│  │ 出力表示エリア                       ││
│  │ Claude Codeからの応答がここに表示    ││
│  └─────────────────────────────────────┘│
└─────────────────────────────────────────┘
```

**実装内容:**
- TypeScript `ACPClient`
- Tauriコマンドのバインド
- 状態監視のポーリング

**ファイル:**
- `src/acp/client.ts`
- `src/acp/types.ts`
- `src/App.tsx` (ACPタブ追加)

---

#### Level 3: 質問処理（1-2日）
**目標: Claude Codeからの質問に回答できる**

```
Claude Code: "どのファイルを使いますか？"
     │
     ▼
検知: AgentState::WaitingForInput
     │
     ▼
UI通知: 質問ダイアログ表示
     │
     ▼
ユーザー回答
     │
     ▼
回答送信 → Claude Code
```

**実装内容:**
- 質問イベントの通知（Tauriイベント）
- UI側での質問ダイアログ
- 回答の送信

**ファイル:**
- `src-tauri/src/acp/events.rs`
- `src/components/QuestionDialog.tsx`

---

#### Level 4: マルチエージェント（2-3日）
**目標: 複数のClaude Codeを同時に操作できる**

```
┌─────────────────┬─────────────────┐
│ pane 0          │ pane 1          │
│ claude-code     │ claude-code     │
│ [翻訳担当]       │ [品質担当]       │
└─────────────────┴─────────────────┘
```

**実装内容:**
- 複数ペイン管理
- エージェント一覧取得
- エージェント選択UI

**ファイル:**
- `src-tauri/src/acp/registry.rs`
- `src/components/AgentList.tsx`

---

#### Level 5: エージェント間通信（2日）
**目標: 翻訳→品質チェックのパイプラインを自動化**

```
翻訳完了
    │
    ▼
翻訳結果を品質担当に送信
    │
    ▼
品質チェック完了
    │
    ▼
最終結果をユーザーに提示
```

**実装内容:**
- `SharedContext` (エージェント間でデータ共有)
- パイプライン定義
- 順次実行の制御

**ファイル:**
- `src-tauri/src/acp/pipeline.rs`
- `src/acp/pipeline.ts`

---

#### Level 6: 自動化（2-3日）
**目標: タスクの自動振り分け、質問の自動分類**

**実装内容:**
- `TaskRouter` (シンプル/複雑判定)
- `QuestionClassifier` (AI/人間判定)
- Orchestrator Agent (まとめ役)

**ファイル:**
- `src-tauri/src/acp/router.rs`
- `src-tauri/src/acp/classifier.rs`
- `src-tauri/src/acp/orchestrator.rs`

---

#### Level 7: 安定化（2-3日）
**目標: エラー処理、タイムアウト、リソース管理**

**実装内容:**
- エラー分類とリカバリー
- タイムアウト監視
- リソース制限

**ファイル:**
- `src-tauri/src/acp/recovery.rs`
- `src-tauri/src/acp/resource.rs`

---

### 開発ロードマップ

```
Week 1: Level 0-1-2 (基盤 + 状態検知 + UI連携)
   → 動くデモができる

Week 2: Level 3-4 (質問処理 + マルチエージェント)
   → 複数Claude Codeが操作できる

Week 3: Level 5-6 (エージェント間通信 + 自動化)
   → パイプライン処理ができる

Week 4: Level 7 (安定化)
   → 本格運用可能
```

### 各Levelの完成定義（Definition of Done）

| Level | 動作確認方法 |
|-------|-------------|
| 0 | `tmux attach` でClaude Codeが見える |
| 0.5 | 技術リスク検証完了レポート |
| 1 | 状態が正しくログに出る |
| 2 | UIにClaude Codeの出力が表示される |
| 3 | 質問ダイアログが表示され、回答できる（★MVP★） |
| 4 | 2つのClaude Codeが同時に見える |
| 5 | 翻訳→品質チェックが自動で流れる |
| 6 | タスクが自動で振り分けられる |
| 7 | エラー時に自動リカバリーする |

---

## アジャイルレビューでの指摘事項

### 高優先度の改善

#### Level 0.5: 技術リスク検証（半日）

**目標: 最も不確実性の高い部分を早期に検証**

```
検証項目:
1. tmux capture-pane でANSIエスケープシーケンスを含む出力を正しく取得できるか
2. Claude Codeのプロンプトパターンを正確に検知できるか
3. tmux send-keys で日本語を正しく送信できるか

検証方法:
- 最小限のRustコードで tmux セッション作成
- claude code を起動して画面キャプチャ
- 正規表現でプロンプトを検知

成果物:
- 検証レポート
- 動作確認済みのパターン定義
```

#### MVPの明確化

**Level 3完了時点をMVPとする**

```
ユーザー価値:
- YouTube URLを入力して字幕をダウンロード
- Claude Codeに翻訳を依頼
- Claude Codeからの質問に回答
- 翻訳結果を取得

これで「翻訳のために人間がClaude Codeを操作する」のではなく、
「アプリ経由で翻訳を依頼できる」という最小限の価値が提供できる。
```

#### 並列開発の可能性

```
                    ┌─────────────────────────┐
                    │ Level 0: 基盤           │
                    └───────────┬─────────────┘
                                │
              ┌─────────────────┼─────────────────┐
              ▼                 ▼                 ▼
    ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
    │ Level 0.5       │ │ Level 1         │ │ Level 2         │
    │ 技術リスク検証   │ │ 状態検知        │ │ UI連携          │
    └─────────────────┘ └─────────────────┘ └─────────────────┘
                                │
                                ▼
                    ┌─────────────────────────┐
                    │ Level 3: 質問処理 ★MVP★ │
                    └───────────┬─────────────┘
                                │
              ┌─────────────────┼─────────────────┐
              ▼                 ▼                 ▼
    ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
    │ Level 4         │ │ Level 5         │ │ Level 6         │
    │ マルチエージェント│ │ エージェント間通信│ │ 自動化          │
    └─────────────────┘ └─────────────────┘ └─────────────────┘
                                │
                                ▼
                    ┌─────────────────────────┐
                    │ Level 7: 安定化         │
                    └─────────────────────────┘
```

### 現実的なスケジュール（5週間）

```
Week 1: Level 0-0.5-1 (基盤 + リスク検証 + 状態検知)
Week 2: Level 2-3 (UI連携 + 質問処理) ★MVPデモ★
Week 3: Level 4-5 (マルチエージェント + 通信)
Week 4: Level 6-7 (自動化 + 安定化)
Week 5: バッファ（統合テスト、バグ修正、リリース準備）
```

### テスト戦略

| Level | テスト種類 |
|-------|-----------|
| 0 | ユニットテスト: tmuxコマンドのモック |
| 1 | ユニットテスト: OutputParser のパターンマッチング |
| 2-3 | E2Eテスト: UI操作をテスト |
| 4-7 | 統合テスト: エージェント間通信 |

### 移行戦略

```
現在の実装:
- PTY経由でのClaude Code起動 (portable-pty使用)
- 字幕ダウンロード機能

ACP設計:
- tmuxベースのオーケストレーター

移行戦略:
1. 既存の PtyManager を維持しつつ TmuxOrchestrator を並行実装
2. 機能完成後に tmux に完全移行
3. フォールバック: tmux利用不可時はPTYを使用
```

---

## v3: CLIベース移行 (2026-02-22)

tmux画面キャプチャベースのパーサーには根本的な問題があったため、CLIベース（`--print --output-format stream-json`）に移行した。

### v2（tmuxベース）の問題点

| 問題 | 説明 |
|------|------|
| 画面スクロール | 古いマーカーが残り、誤検出の原因 |
| 入出力の区別不可 | 自分の入力とClaude Codeの出力が混在 |
| 状態検出の不確実性 | `@DONE@`のみに依存、権限プロンプト検出が不安定 |

### v3（CLIベース）の解決策

| 問題 | 解決方法 |
|------|----------|
| 入出力の区別 | stdin/stdoutが明確に分離 |
| 状態検出 | JSONイベントで全状態が明示される |
| 権限プロンプト | `tool_result`のエラーで検出 + 自動/手動応答 |
| 完了検出 | `result`イベントで確実に検出 |

### アーキテクチャ

```
┌─────────────────────────────────────────────────────────┐
│                    Re-Voice App                          │
│                                                          │
│  ┌────────────────────────────────────────────────────┐ │
│  │              ClaudeCodeExecutor                     │ │
│  │                                                     │ │
│  │   子プロセス → JSON Parser → State Manager         │ │
│  │       ↑           ↓             ↓                  │ │
│  │    stdin      Parsed Events   AgentState          │ │
│  └────────────────────────────────────────────────────┘ │
│                                                          │
│  ┌────────────────────────────────────────────────────┐ │
│  │            Permission Manager                       │ │
│  │   - 事前許可リスト (--allowedTools)                 │ │
│  │   - 実行時権限要求処理                              │ │
│  │   - 人間へのエスカレーション                        │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### 新規ファイル

| ファイル | 内容 |
|---------|------|
| `executor.rs` | ClaudeCodeExecutor - 子プロセス管理 |
| `stream_parser.rs` | stream-jsonパーサー |
| `state_machine.rs` | 状態マシン |
| `permission.rs` | 権限管理 |

### stream-jsonイベントと状態の対応

| イベントタイプ | 検出される状態 |
|---------------|---------------|
| `system/init` | Idle |
| `assistant` | Processing |
| `tool_use` | Processing (ツール実行中) |
| `tool_result` (is_error=true) | WaitingForPermission または Error |
| `result` | Completed |

### 権限ポリシー

```rust
pub enum PermissionPolicy {
    ReadOnly,    // 読み取り専用（自動許可のみ）
    Standard,    // 標準（読み取りは自動、書き込みは確認）
    Strict,      // 厳格（全て確認）
    Permissive,  // 自由（全て自動許可）
}
```

**デフォルト許可ツール（読み取り系）:**
- Read, Grep, Glob
- Bash(ls:*), Bash(cat:*), Bash(git status:*)

**人間確認が必要なツール（書き込み系）:**
- Edit, Write
- Bash(rm:*), Bash(mv:*), Bash(npm:*)

### 新規Tauriコマンド

| コマンド | 説明 |
|---------|------|
| `executor_start` | CLIエグゼキューター起動 |
| `executor_execute` | タスクを実行 |
| `executor_stop` | 停止 |
| `executor_get_state` | 現在の状態を取得 |
| `executor_submit_permission` | 権限要求に回答 |
| `executor_is_running` | 起動状態確認 |

### レガシーファイル（廃止予定）

- `tmux.rs` - tmuxベースのオーケストレーター
- `poller.rs` - ステータスポーラー
- `parser.rs` - 画面キャプチャベースのパーサー
