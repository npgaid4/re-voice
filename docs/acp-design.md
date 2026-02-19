# ACP: Agent Communication Protocol

## Context

Re-Voiceおよび今後開発するすべてのAIアプリの基盤となる、マルチエージェント対応の汎用通信プロトコル。

**設計方針:**
- シンプルなインターフェース: 入力プロンプト → 出力プロンプトのみ
- マルチエージェント: 複数のClaude Codeインスタンスと同時通信
- エージェント間通信: Claude Code ↔ Codex間の通信にも使用可能

---

## プロトコル概要

### エージェント識別子

```
acp://<agent-id>@<host>/<instance>

例:
- acp://claude-code@localhost/main
- acp://codex@192.168.1.100/worker-3
```

### エージェントカード (Agent Card)

```typescript
interface AgentCard {
  id: string;                    // "claude-code@localhost"
  protocol: "ACP/1.0";
  name: string;                  // "Claude Code"
  capabilities: Capability[];    // 能力宣言
  endpoint: string;              // 接続先
  transport: "pty" | "stdio" | "websocket" | "http";
}

interface Capability {
  id: string;                    // "translation"
  name: string;                  // "Translation"
  tags?: string[];               // ["japanese", "english"]
}
```

### メッセージフォーマット

```typescript
interface ACPMessage {
  id: string;                    // UUID
  timestamp: string;             // ISO 8601

  // ルーティング
  from: string;                  // 送信元アドレス
  to: string | string[];         // 宛先（ユニキャスト/ブロードキャスト）

  // メッセージタイプ
  type: "prompt" | "response" | "broadcast" | "discover" | "advertise" | "error";

  // ペイロード（シンプルなテキスト）
  payload: {
    content: string;             // プロンプト/レスポンステキスト
    data?: Record<string, unknown>;  // オプション: 構造化データ
  };

  // メタデータ
  metadata?: {
    priority?: "low" | "normal" | "high";
    ttl?: number;                // Time-to-live (秒)
    correlationId?: string;      // Request-Response相関
  };
}
```

---

## メッセージフロー

```
Agent A (Claude)                    Agent B (Codex)
       │                                  │
       │  1. discover (broadcast)         │
       │ ────────────────────────────────>│
       │                                  │
       │  2. advertise (capabilities)     │
       │ <────────────────────────────────│
       │                                  │
       │  3. prompt                       │
       │ ────────────────────────────────>│
       │  { from: "claude@...",           │
       │    to: "codex@...",              │
       │    payload: { content: "..." } } │
       │                                  │
       │  4. response                     │
       │ <────────────────────────────────│
       │  { correlationId: "...",         │
       │    payload: { content: "..." } } │
```

---

## トランスポート

### PTYトランスポート（既存実装を拡張）

```
メッセージフレーミング:
<ACP>{"id":"uuid","type":"prompt",...}</ACP>
```

### サポートトランスポート

| タイプ | 用途 |
|--------|------|
| pty | TUIアプリ（Claude Code等） |
| stdio | ローカルプロセス |
| websocket | リモート通信 |
| http | フォールバック |

---

## API設計

### Rust Tauriコマンド

```rust
#[tauri::command]
async fn acp_register_agent(card: AgentCard) -> Result<(), String>;

#[tauri::command]
async fn acp_discover_agents(query: DiscoveryQuery) -> Result<Vec<AgentCard>, String>;

#[tauri::command]
async fn acp_send_message(message: ACPMessage) -> Result<(), String>;

#[tauri::command]
async fn acp_broadcast(payload: MessagePayload, capabilities: Option<Vec<String>>) -> Result<(), String>;

#[tauri::command]
fn acp_read_messages() -> Result<Vec<ACPMessage>, String>;
```

### TypeScript クライアント

```typescript
class ACPClient {
  async registerAgent(card: AgentCard): Promise<void>;
  async discoverAgents(query: DiscoveryQuery): Promise<AgentCard[]>;
  async send(to: string, content: string): Promise<void>;
  async broadcast(content: string, filter?: { capabilities?: string[] }): Promise<void>;
  async readMessages(): Promise<ACPMessage[]>;
  subscribe(handler: (message: ACPMessage) => void): () => void;
}
```

---

## 使用例

### Re-Voiceでの翻訳リクエスト

```typescript
const acp = new ACPClient(invoke);

// 翻訳能力を持つエージェントを発見
const translators = await acp.discoverAgents({
  capabilities: ['translation', 'japanese']
});

// 最初の翻訳エージェントに送信
await acp.send(translators[0].endpoint,
  `以下の字幕を日本語に翻訳してください:\n\n${subtitleText}`);

// 応答を待機
const response = await waitForResponse();
```

### ブロードキャスト

```typescript
// 全コーディングエージェントにコードレビューを依頼
await acp.broadcast(
  'このコードをレビューしてください:\n```rust\nfn main() { ... }\n```',
  { capabilities: ['code-review'] }
);
```

---

## 実装計画

### ディレクトリ構造

```
src-tauri/src/
├── acp/
│   ├── mod.rs           # ACPモジュール
│   ├── message.rs       # メッセージ定義
│   ├── agent.rs         # エージェントカード
│   ├── registry.rs      # レジストリ
│   └── transport/
│       ├── mod.rs
│       └── pty.rs       # PTYトランスポート
├── lib.rs               # Tauriコマンド追加
└── pty.rs               # 既存（拡張）

src/
├── acp/
│   ├── index.ts         # ACPクライアント
│   ├── message.ts       # 型定義
│   └── transport/
│       └── tauri.ts     # Tauri IPC
└── App.tsx              # 統合
```

### 実装フェーズ

**Phase 1: MVP**
- 基本メッセージ型
- PTYトランスポート
- 送受信機能

**Phase 2: レジストリ**
- エージェント発見
- ハートビート

**Phase 3: 拡張**
- ブロードキャスト
- WebSocket

**Phase 4: Re-Voice統合**
- 翻訳エージェント連携

---

## Agent Adapter（プロトコル変換レイヤー）

Claude CodeやCodexはACPをネイティブに理解しないため、**Agent Adapter**がACPメッセージとネイティブCLI I/Oを変換する。

### アーキテクチャ

```
┌─────────────────────────────────────────────────────────────────┐
│                        Re-Voice App                             │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    ACP Orchestrator                      │   │
│  │         (メッセージルーティング・マルチエージェント管理)      │   │
│  └───────────────────────┬─────────────────────────────────┘   │
│                          │ ACP Message                          │
│          ┌───────────────┼───────────────┐                      │
│          ▼               ▼               ▼                      │
│  ┌───────────────┐ ┌───────────────┐ ┌───────────────┐         │
│  │ Agent Adapter │ │ Agent Adapter │ │ Agent Adapter │         │
│  │  (Claude #1)  │ │  (Claude #2)  │ │   (Codex)     │         │
│  │ ┌───────────┐ │ │ ┌───────────┐ │ │ ┌───────────┐ │         │
│  │ │InputConv. │ │ │ │InputConv. │ │ │ │InputConv. │ │         │
│  │ └─────┬─────┘ │ │ └─────┬─────┘ │ │ └─────┬─────┘ │         │
│  │       ▼       │ │       ▼       │ │       ▼       │         │
│  │ ┌───────────┐ │ │ ┌───────────┐ │ │ ┌───────────┐ │         │
│  │ │    PTY    │ │ │ │    PTY    │ │ │ │   PTY     │ │         │
│  │ └─────┬─────┘ │ │ └─────┬─────┘ │ │ └─────┬─────┘ │         │
│  │       ▼       │ │       ▼       │ │       ▼       │         │
│  │ ┌───────────┐ │ │ ┌───────────┐ │ │ ┌───────────┐ │         │
│  │ │OutputConv.│ │ │ │OutputConv.│ │ │ │OutputConv.│ │         │
│  │ └───────────┘ │ │ └───────────┘ │ │ └───────────┘ │         │
│  └───────┬───────┘ └───────┬───────┘ └───────┬───────┘         │
└──────────┼─────────────────┼─────────────────┼──────────────────┘
           ▼                 ▼                 ▼
    ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
    │ Claude Code  │  │ Claude Code  │  │    Codex     │
    │  Instance 1  │  │  Instance 2  │  │              │
    └──────────────┘  └──────────────┘  └──────────────┘
```

### AgentAdapter Trait

```rust
// src-tauri/src/acp/adapter.rs

use async_trait::async_trait;
use tokio::sync::mpsc;
use uuid::Uuid;

/// エージェントの実行状態
#[derive(Debug, Clone)]
pub enum AgentStatus {
    Idle,
    Busy { task_id: Uuid },
    Error { message: String },
    Shutdown,
}

/// エージェント情報
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub capabilities: Vec<Capability>,
}

/// タスクリクエスト
#[derive(Debug, Clone)]
pub struct TaskRequest {
    pub task_id: Uuid,
    pub payload: TaskPayload,
    pub context: Option<SharedContext>,
}

/// タスクペイロード（ACPメッセージから抽出）
#[derive(Debug, Clone)]
pub struct TaskPayload {
    pub content: String,
    pub data: Option<serde_json::Value>,
}

/// 共有コンテキスト（マルチエージェント用）
#[derive(Debug, Clone, Default)]
pub struct SharedContext {
    pub conversation_history: Vec<ContextEntry>,
    pub shared_files: Vec<String>,
    pub metadata: serde_json::Value,
}

/// アダプタからのコールバックイベント
#[derive(Debug, Clone)]
pub enum AdapterEvent {
    OutputChunk { task_id: Uuid, chunk: StreamChunk },
    TaskComplete { task_id: Uuid, result: TaskResult },
    Error { task_id: Uuid, error: String },
}

/// ストリーム出力チャンク
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub text: String,
    pub is_final: bool,
}

/// タスク完了結果
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub output: String,
    pub metadata: Option<serde_json::Value>,
}

/// エージェントアダプタのtrait
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    /// エージェント情報を取得
    fn agent_info(&self) -> &AgentInfo;

    /// 能力一覧を取得
    fn capabilities(&self) -> &[Capability];

    /// エージェントを初期化（PTY起動など）
    async fn initialize(&mut self) -> Result<(), AdapterError>;

    /// エージェントを終了
    async fn shutdown(&mut self) -> Result<(), AdapterError>;

    /// タスクを実行（非同期ストリーミング）
    async fn execute_task(&mut self, request: TaskRequest) -> Result<mpsc::Receiver<AdapterEvent>, AdapterError>;

    /// タスクをキャンセル
    async fn cancel_task(&mut self, task_id: Uuid) -> Result<(), AdapterError>;

    /// 現在の状態を取得
    async fn status(&self) -> AgentStatus;

    /// 他のエージェントからのコンテキストを受信
    async fn receive_context(&mut self, context: SharedContext) -> Result<(), AdapterError>;
}
```

### InputConverter / OutputConverter Traits

```rust
// src-tauri/src/acp/adapter.rs

/// 入力変換: ACP → ネイティブCLI入力
pub trait InputConverter: Send + Sync {
    /// ACPタスクをネイティブ入力に変換
    fn convert_input(&self, task: &TaskPayload) -> Result<String, AdapterError>;

    /// 共有コンテキストをプロンプトに埋め込む
    fn embed_context(&self, prompt: &str, context: &SharedContext) -> String;
}

/// 出力変換: ネイティブCLI出力 → ACP
pub trait OutputConverter: Send + Sync {
    /// 生の出力をパース
    fn parse_output(&self, raw_output: &str) -> Result<Vec<ParsedOutput>, AdapterError>;

    /// パース結果をストリームチャンクに変換
    fn to_stream_chunk(&self, parsed: &ParsedOutput) -> Option<StreamChunk>;

    /// プロンプト完了を検知
    fn is_prompt_complete(&self, output: &str) -> bool;
}

/// パース済み出力
#[derive(Debug, Clone)]
pub struct ParsedOutput {
    pub content: String,
    pub content_type: OutputContentType,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum OutputContentType {
    Text,
    CodeBlock { language: String },
    Thinking,
    ToolUse { tool_name: String },
    ErrorMessage,
}
```

### ClaudeCodeAdapter 実装

```rust
// src-tauri/src/acp/adapters/claude_code.rs

use crate::acp::adapter::*;
use crate::pty::PtyManager;

/// Claude Code用アダプタ
pub struct ClaudeCodeAdapter {
    info: AgentInfo,
    pty: PtyManager,
    input_converter: Box<dyn InputConverter>,
    output_converter: Box<dyn OutputConverter>,
    status: AgentStatus,
}

impl ClaudeCodeAdapter {
    pub fn new(instance_id: &str) -> Self {
        Self {
            info: AgentInfo {
                id: format!("claude-code-{}", instance_id),
                name: format!("Claude Code ({})", instance_id),
                endpoint: format!("acp://claude-code@localhost/{}", instance_id),
                capabilities: vec![
                    Capability { id: "translation".into(), name: "Translation".into(), tags: None },
                    Capability { id: "code-generation".into(), name: "Code Generation".into(), tags: None },
                    Capability { id: "analysis".into(), name: "Analysis".into(), tags: None },
                ],
            },
            pty: PtyManager::new(),
            input_converter: Box::new(ClaudeCodeInputConverter),
            output_converter: Box::new(ClaudeCodeOutputConverter::new()),
            status: AgentStatus::Idle,
        }
    }
}

/// Claude Code用入力変換
pub struct ClaudeCodeInputConverter;

impl InputConverter for ClaudeCodeInputConverter {
    fn convert_input(&self, task: &TaskPayload) -> Result<String, AdapterError> {
        // ACPメッセージをそのままプロンプトとして使用
        // Claude Codeは自然言語プロンプトを直接理解可能
        Ok(task.content.clone())
    }

    fn embed_context(&self, prompt: &str, context: &SharedContext) -> String {
        if context.conversation_history.is_empty() && context.shared_files.is_empty() {
            return prompt.to_string();
        }

        let mut full_prompt = String::new();

        // 共有ファイルがあれば追加
        if !context.shared_files.is_empty() {
            full_prompt.push_str("## 関連ファイル\n\n");
            for file in &context.shared_files {
                full_prompt.push_str(&format!("- {}\n", file));
            }
            full_prompt.push_str("\n");
        }

        // 会話履歴があれば追加
        if !context.conversation_history.is_empty() {
            full_prompt.push_str("## これまでの会話\n\n");
            for entry in &context.conversation_history {
                full_prompt.push_str(&format!("> {}\n\n", entry.summary));
            }
        }

        full_prompt.push_str("---\n\n");
        full_prompt.push_str(prompt);

        full_prompt
    }
}

/// Claude Code用出力変換
pub struct ClaudeCodeOutputConverter {
    ansi_regex: regex::Regex,
    completion_patterns: Vec<regex::Regex>,
}

impl ClaudeCodeOutputConverter {
    pub fn new() -> Self {
        use regex::Regex;

        Self {
            // ANSIエスケープシーケンスを除去
            ansi_regex: Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap(),

            // 完了パターン（Claude Codeのプロンプトに戻ったことを検知）
            completion_patterns: vec![
                Regex::new(r"> $").unwrap(),  // 標準プロンプト
                Regex::new(r"❯ $").unwrap(),  // カスタムプロンプト
            ],
        }
    }
}

impl OutputConverter for ClaudeCodeOutputConverter {
    fn parse_output(&self, raw_output: &str) -> Result<Vec<ParsedOutput>, AdapterError> {
        // 1. ANSIエスケープシーケンスを除去
        let clean_output = self.ansi_regex.replace_all(raw_output, "");

        // 2. コードブロックとテキストを分離
        let mut results = Vec::new();
        let mut current_text = String::new();

        // 簡易パーサー（実際はより高度な実装が必要）
        for line in clean_output.lines() {
            if line.starts_with("```") {
                // コードブロック開始/終了
                if !current_text.is_empty() {
                    results.push(ParsedOutput {
                        content: current_text.clone(),
                        content_type: OutputContentType::Text,
                        metadata: None,
                    });
                    current_text.clear();
                }
            } else {
                current_text.push_str(line);
                current_text.push('\n');
            }
        }

        if !current_text.is_empty() {
            results.push(ParsedOutput {
                content: current_text,
                content_type: OutputContentType::Text,
                metadata: None,
            });
        }

        Ok(results)
    }

    fn to_stream_chunk(&self, parsed: &ParsedOutput) -> Option<StreamChunk> {
        if parsed.content.is_empty() {
            None
        } else {
            Some(StreamChunk {
                text: parsed.content.clone(),
                is_final: false,
            })
        }
    }

    fn is_prompt_complete(&self, output: &str) -> bool {
        // Claude Codeが入力待ち状態に戻ったかチェック
        for pattern in &self.completion_patterns {
            if pattern.is_match(output) {
                return true;
            }
        }
        false
    }
}

#[async_trait]
impl AgentAdapter for ClaudeCodeAdapter {
    fn agent_info(&self) -> &AgentInfo {
        &self.info
    }

    fn capabilities(&self) -> &[Capability] {
        &self.info.capabilities
    }

    async fn initialize(&mut self) -> Result<(), AdapterError> {
        // PTYでClaude Codeを起動
        self.pty.spawn_claude_code()
            .map_err(|e| AdapterError::InitializationFailed(e.to_string()))?;

        // 起動完了まで待機
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        self.status = AgentStatus::Idle;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), AdapterError> {
        self.status = AgentStatus::Shutdown;
        Ok(())
    }

    async fn execute_task(&mut self, request: TaskRequest) -> Result<mpsc::Receiver<AdapterEvent>, AdapterError> {
        let (tx, rx) = mpsc::channel(100);

        // 入力変換
        let prompt = self.input_converter.convert_input(&request.payload)?;

        // PTYに送信
        self.pty.send_message(&prompt)
            .map_err(|e| AdapterError::CommunicationFailed(e.to_string()))?;

        self.status = AgentStatus::Busy { task_id: request.task_id };

        // 出力読み取り（非同期）
        let task_id = request.task_id;
        let output_converter = self.output_converter.clone(); // FIXME: Box<dyn>はCloneできないので要修正

        // tokioタスクで出力を監視
        tokio::spawn(async move {
            // PTYから出力を読み取り、イベントを送信
            // 実装は省略
            let _ = tx.send(AdapterEvent::TaskComplete {
                task_id,
                result: TaskResult {
                    output: "Completed".to_string(),
                    metadata: None,
                },
            }).await;
        });

        Ok(rx)
    }

    async fn cancel_task(&mut self, task_id: Uuid) -> Result<(), AdapterError> {
        // Ctrl+Cを送信
        self.pty.send_message("\x03")
            .map_err(|e| AdapterError::CommunicationFailed(e.to_string()))?;

        self.status = AgentStatus::Idle;
        Ok(())
    }

    async fn status(&self) -> AgentStatus {
        self.status.clone()
    }

    async fn receive_context(&mut self, context: SharedContext) -> Result<(), AdapterError> {
        // コンテキストを保持（次回execute_taskで使用）
        let _ = context;
        Ok(())
    }
}
```

---

## マルチエージェント（Agent Orchestrator）

### Orchestrator の役割

```rust
// src-tauri/src/acp/orchestrator.rs

use std::collections::HashMap;
use uuid::Uuid;

/// マルチエージェントオーケストレーター
pub struct AgentOrchestrator {
    adapters: HashMap<String, Box<dyn AgentAdapter>>,
    shared_context: SharedContext,
    message_queue: Vec<ACPMessage>,
}

impl AgentOrchestrator {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
            shared_context: SharedContext::default(),
            message_queue: Vec::new(),
        }
    }

    /// エージェントを登録
    pub fn register_agent(&mut self, adapter: Box<dyn AgentAdapter>) {
        let id = adapter.agent_info().id.clone();
        self.adapters.insert(id, adapter);
    }

    /// 能力でエージェントを検索
    pub fn discover_by_capability(&self, capability: &str) -> Vec<&AgentInfo> {
        self.adapters.values()
            .filter(|a| a.capabilities().iter().any(|c| c.id == capability))
            .map(|a| a.agent_info())
            .collect()
    }

    /// メッセージをルーティング
    pub async fn route_message(&mut self, message: ACPMessage) -> Result<(), OrchestratorError> {
        match &message.to {
            ACPAddress::Single(to_id) => {
                // ユニキャスト
                if let Some(adapter) = self.adapters.get_mut(to_id) {
                    self.deliver_to_adapter(adapter, message).await?;
                }
            }
            ACPAddress::Multiple(to_ids) => {
                // マルチキャスト
                for to_id in to_ids {
                    if let Some(adapter) = self.adapters.get_mut(to_id) {
                        self.deliver_to_adapter(adapter, message.clone()).await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn deliver_to_adapter(
        &mut self,
        adapter: &mut Box<dyn AgentAdapter>,
        message: ACPMessage,
    ) -> Result<(), OrchestratorError> {
        // 共有コンテキストを渡す
        adapter.receive_context(self.shared_context.clone()).await?;

        // タスク実行
        let request = TaskRequest {
            task_id: Uuid::new_v4(),
            payload: TaskPayload {
                content: message.payload.content,
                data: message.payload.data,
            },
            context: Some(self.shared_context.clone()),
        };

        let mut rx = adapter.execute_task(request).await?;

        // 結果を待機して共有コンテキストに追加
        while let Some(event) = rx.recv().await {
            match event {
                AdapterEvent::TaskComplete { result, .. } => {
                    self.shared_context.conversation_history.push(ContextEntry {
                        agent_id: adapter.agent_info().id.clone(),
                        summary: result.output,
                        timestamp: chrono::Utc::now(),
                    });
                }
                _ => {}
            }
        }

        Ok(())
    }
}
```

### マルチエージェントフロー

```
┌──────────────────────────────────────────────────────────────┐
│                      Orchestrator                             │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │                   Shared Context                         │ │
│  │  - 会話履歴                                              │ │
│  │  - 共有ファイル                                          │ │
│  │  - メタデータ                                            │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                               │
│  Task: "字幕を翻訳して、品質チェックもして"                     │
│                                                               │
│  ┌────────────┐     ┌────────────┐     ┌────────────┐       │
│  │  Claude #1 │     │  Claude #2 │     │   Codex    │       │
│  │ (翻訳担当) │     │ (品質担当) │     │ (コード生成)│       │
│  └─────┬──────┘     └─────┬──────┘     └─────┬──────┘       │
│        │                  │                  │               │
│        │ 翻訳実行          │                  │               │
│        │ ─────────────────┼──────────────────┤               │
│        │                  │                  │               │
│        │ 結果をShared      │                  │               │
│        │ Contextに追加     │                  │               │
│        │ ─────────────────┼──────────────────┤               │
│        │                  │                  │               │
│        │                  │ Shared Context   │               │
│        │                  │ から翻訳結果取得  │               │
│        │                  │ 品質チェック実行  │               │
│        │                  │ ─────────────────┤               │
│        │                  │                  │               │
│        │                  │ 結果を追加        │               │
│        │                  │ ─────────────────┤               │
│        │                  │                  │               │
│        ▼                  ▼                  ▼               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │              Final Output                             │   │
│  │  - 翻訳されたテキスト                                   │   │
│  │  - 品質チェック結果                                     │   │
│  └──────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

---

## Claude Code内でのマルチエージェント

Claude Code単体でもマルチエージェント的な処理が可能：

```
┌─────────────────────────────────────────────────────────────┐
│                    Claude Code Instance                      │
│                                                              │
│  プロンプト:                                                  │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ あなたは以下の3つの役割を持つエージェントです:            ││
│  │                                                          ││
│  │ 1. 翻訳者: 英語→日本語                                   ││
│  │ 2. レビュアー: 翻訳品質をチェック                         ││
│  │ 3. 編集者: 最終調整                                       ││
│  │                                                          ││
│  │ [翻訳対象テキスト]                                        ││
│  │ ...                                                      ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  出力:                                                       │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ [翻訳者] ...                                             ││
│  │ [レビュアー] ...                                         ││
│  │ [編集者] ...                                             ││
│  │ [最終結果] ...                                           ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  これは「プロンプト内マルチエージェント」と呼ばれる           │
│  単一Claude Codeインスタンス内で完結                         │
└─────────────────────────────────────────────────────────────┘
```

### 2つのアプローチの使い分け

| アプローチ | メリット | デメリット | 適用ケース |
|-----------|---------|-----------|-----------|
| **複数インスタンス** | 並列処理、独立性高い | リソース消費大 | 大規模タスク、並列処理 |
| **プロンプト内** | シンプル、低リソース | 並列不可、コンテキスト制限 | 単純なタスク、プロトタイプ |

---

## 検証方法

1. `pnpm tauri dev` でアプリ起動
2. エージェント登録テスト
3. メッセージ送受信テスト
4. 複数エージェントでのブロードキャストテスト
5. Re-Voiceで翻訳機能テスト
6. マルチエージェント連携テスト（翻訳→品質チェック）
