# ACP (Agent Communication Protocol) 実装ドキュメント

## 概要

Re-Voiceおよび今後開発するすべてのAIアプリの基盤となる、マルチエージェント対応の汎用通信プロトコルを実装した。

**2026-02-21 更新**: A2A (Agent-to-Agent) プロトコル準拠化完了

## A2A Protocol 準拠 (v3)

A2AはGoogleが策定し、Linux Foundationが管理するエージェント間通信の業界標準プロトコル。
- **参照**: https://github.com/google/A2A
- **プロトコルバージョン**: `0.3.0`

### A2A Agent Card 構造

Agent Cardはエージェントの「デジタル名刺」として機能し、以下のパスでホストされる:
```
https://<agent-base-url>/.well-known/agent.json
```

```typescript
interface AgentCard {
  // A2A 必須フィールド
  name: string;                    // エージェント表示名
  url: string;                     // 通信エンドポイント
  version: string;                 // エージェントバージョン
  protocolVersion: "0.3.0";        // A2Aプロトコルバージョン

  // A2A オプションフィールド
  description?: string;
  provider?: { organization: string; url?: string };
  capabilities?: {
    streaming: boolean;
    pushNotifications: boolean;
    stateTransitionHistory: boolean;
  };
  authentication?: { schemes: string[] };
  defaultInputModes?: string[];    // ["text/plain"]
  defaultOutputModes?: string[];   // ["text/plain", "application/json"]
  skills?: Skill[];                // エージェントが実行できるタスク

  // 内部フィールド（A2A外）
  id?: string;                     // 内部管理用ID
  transport?: TransportType;       // トランスポート種別
}
```

### Skill構造（旧Capability）

```typescript
interface Skill {
  id: string;                      // スキルID
  name: string;                    // 表示名
  description?: string;
  tags?: string[];                 // 検索用タグ
  examples?: string[];             // 使用例
  inputSchema?: JSONSchema;        // 入力定義
  outputSchema?: JSONSchema;       // 出力定義
  inputModes?: string[];
  outputModes?: string[];
}
```

### A2A vs 旧形式の対応

| 旧フィールド | A2Aフィールド | 説明 |
|-------------|--------------|------|
| `capabilities` | `skills` | エージェントが実行できるタスク |
| `id` | `id` (Optional) | 内部管理用（A2A外） |
| `protocol` | `protocolVersion` | プロトコルバージョン |
| - | `capabilities` | 技術的機能（streaming等） |
| - | `provider` | 組織情報 |
| - | `authentication` | 認証スキーム |

---

## ACP v3 新機能

### 1. Pipeline実行 (`pipeline.rs`)

複数エージェントを順次実行するパイプライン機能:

```rust
let pipeline = PipelineDefinition::new("translate-pipeline")
    .add_stage(PipelineStage::new("translate", AgentAddress::new("translator@local")))
    .add_stage(PipelineStage::new("review", AgentAddress::new("reviewer@local")));

let execution = executor.start_execution(&pipeline_id)?;
executor.complete_stage(&execution_id, output)?;
```

### 2. Broadcast機能 (`tmux.rs`)

複数エージェントへの一斉送信:

```rust
let filter = CapabilityFilter::new()
    .with_capabilities(vec!["translation".into()]);
let (success, failures) = orch.broadcast_message(&content, Some(&filter))?;
```

### 3. AddressType拡張

```rust
pub enum AddressType {
    Single { address: AgentAddress },
    Multiple { addresses: Vec<AgentAddress> },
    Broadcast { filter: Option<CapabilityFilter> },
    Pipeline { stages: Vec<PipelineStage> },
}
```

### 4. 新メッセージタイプ

```rust
pub enum MessageType {
    // 基本
    Prompt, Response, Stream, Error,
    // エージェント管理
    Discover, Advertise, Heartbeat,
    // 制御
    Cancel, Question, Answer,
    // パイプライン
    PipelineStart, PipelineStage, PipelineEnd,
}
```

---

## CLIベース実装 (v3.5)

### 1. AgentState (`state_machine.rs`)

```rust
pub enum AgentState {
    /// 初期化中
    Initializing,
    /// アイドル（次のタスク待ち）
    Idle,
    /// 処理中
    Processing { current_tool: Option<String>, started_at: DateTime<Utc> },
    /// 権限要求中
    WaitingForPermission { tool_name: String, tool_input: Value, request_id: String },
    /// ユーザー入力待ち（AskTool）
    WaitingForInput { question: String, options: Vec<String> },
    /// エラー
    Error { message: String, recoverable: bool },
    /// 完了
    Completed { output: String },
}
```

### 2. 状態遷移イベント

```rust
pub enum StateEvent {
    Initialized,
    TaskStarted { prompt: String },
    ToolUseStarted { tool_name: String },
    ToolUseCompleted { tool_name: String, success: bool },
    PermissionRequired { tool_name: String, tool_input: Value, request_id: String },
    PermissionGranted { request_id: String },
    PermissionDenied { request_id: String, reason: String },
    InputRequired { question: String, options: Vec<String> },
    InputReceived { answer: String },
    ErrorOccurred { message: String, recoverable: bool },
    TaskCompleted { output: String },
}
```

### 3. StreamEvent (`stream_parser.rs`)

Claude Code CLIの`--print --output-format stream-json`出力をパース:

```rust
pub enum StreamEvent {
    /// システム初期化
    System { subtype: String, session_id: Option<String>, model: Option<String>, ... },
    /// アシスタントメッセージ
    Assistant { message: AssistantMessage },
    /// ツール使用
    ToolUse { id: String, name: String, input: Value },
    /// ツール結果
    ToolResult { tool_use_id: String, content: String, is_error: bool },
    /// 最終結果
    Result { subtype: Option<String>, result: Option<String>, is_error: bool, ... },
    /// エラー
    Error { error: ErrorDetail },
}
```

### 4. 権限管理 (`permission.rs`)

```rust
pub enum PermissionPolicy {
    ReadOnly,    // 読み取り専用（自動許可のみ）
    Standard,    // 標準（読み取りは自動、書き込みは確認）
    Strict,      // 厳格（全て確認）
    Permissive,  // 自由（全て自動許可）
}

// デフォルト許可ツール（読み取り系）
fn auto_approve_tools() -> Vec<String> {
    vec!["Read", "Grep", "Glob", "Bash(ls:*)", "Bash(git status:*)", ...]
}

// 人間確認が必要なツール（書き込み系）
fn require_confirmation_tools() -> Vec<String> {
    vec!["Edit", "Write", "Bash(rm:*)", "Bash(npm:*)", ...]
}
```

### 5. ClaudeCodeExecutor (`executor.rs`)

```rust
pub struct ClaudeCodeExecutor {
    process: Option<Child>,
    stdin: Option<ChildStdin>,
    session_id: Option<String>,
    permission_manager: Arc<Mutex<PermissionManager>>,
    state_machine: Arc<Mutex<StateMachine>>,
    parser: StreamParser,
    event_tx: mpsc::Sender<ExecutorEvent>,
}

impl ClaudeCodeExecutor {
    pub async fn start(&mut self) -> Result<()> {
        let mut cmd = Command::new("claude");
        cmd.args(["--print", "--output-format", "stream-json"]);
        // --allowedTools, --resume など
        ...
    }

    pub async fn execute(&mut self, prompt: &str) -> Result<String> {
        // stdinにプロンプト送信
        // 状態をProcessingに
        // 完了を待機
        ...
    }
}
```

---

## 実装済み機能

### Rustバックエンド (`src-tauri/src/acp/`)

#### 1. メッセージ型 (`message.rs`)

- `ACPEnvelope` - v3エンベロープ（protocol + message + metadata）
- `ACPMessage` - レガシーメッセージ（後方互換）
- `ACPMessageV3` - 拡張メッセージ（AgentAddress, AddressType）
- `AddressType` - single/multiple/broadcast/pipeline
- `ACPFrame` - PTYトランスポート用フレーミング (`<ACP>{json}</ACP>`)

#### 2. エージェント型 (`agent.rs`) - A2A準拠

- `AgentCard` - A2A準拠エージェントカード
- `Skill` - スキル定義（旧Capability）
- `AgentCapabilities` - 技術的機能（streaming等）
- `Authentication` - 認証スキーム
- `Provider` - 組織情報
- `DiscoveryQuery` - A2A互換検索クエリ

#### 3. パイプライン (`pipeline.rs`) - NEW

- `PipelineDefinition` - パイプライン定義
- `PipelineExecution` - 実行状態管理
- `PipelineExecutor` - 実行エンジン
- `StageResult` - ステージ結果

#### 4. レジストリ (`registry.rs`)

- `AgentRegistry` - スレッドセーフなエージェント登録管理
- ハートビート追跡
- 古いエージェントの自動クリーンアップ

#### 5. tmuxオーケストレーター (`tmux.rs`)

- セッション作成/破棄
- エージェント起動（Claude Code, Codex, GenericShell）
- **Broadcast機能** - CapabilityFilter付き一斉送信
- **discover_agents** - フィルター付きエージェント検索

### TypeScriptフロントエンド (`src/acp/`)

#### 1. 型定義 (`types.ts`)

- Rust型に対応するTypeScript型定義
- AgentCard, ACPMessage, DiscoveryQuery, TaskState等

#### 2. ACPクライアント (`index.ts`)

- `ACPClient` クラス
  - `registerAgent()` - エージェント登録
  - `discoverAgents()` - エージェント検索
  - `listAgents()` - 全エージェント一覧
  - `send()` - メッセージ送信
  - `broadcast()` - ブロードキャスト
  - `translate()` - 翻訳の便利メソッド
  - `reviewCode()` - コードレビューの便利メソッド

#### 3. トランスポート (`transport/index.ts`)

- Tauri IPCトランスポート実装

### Tauriコマンド

#### CLI Executor コマンド（v3.5 - 2026-02-22追加）

| コマンド | 説明 |
|---------|------|
| `executor_start` | CLIエグゼキューター起動（working_dir, allowed_tools, session_id） |
| `executor_execute` | タスクを実行（prompt） |
| `executor_stop` | エグゼキューター停止 |
| `executor_get_state` | 現在のAgentStateを取得 |
| `executor_submit_permission` | 権限要求に回答（request_id, allow, always） |
| `executor_is_running` | 起動状態確認 |

#### ACP v3 コマンド（新規）

| コマンド | 説明 |
|---------|------|
| `acp_define_pipeline` | パイプライン定義 |
| `acp_execute_pipeline` | パイプライン実行 |
| `acp_get_pipeline_status` | 実行状態取得 |
| `acp_cancel_pipeline` | キャンセル |
| `acp_list_pipelines` | パイプライン一覧 |
| `acp_list_active_executions` | アクティブ実行一覧 |
| `acp_broadcast_v3` | ブロードキャスト（CapabilityFilter対応） |
| `acp_broadcast_to_idle` | アイドルエージェントのみ送信 |
| `acp_discover_agents_v3` | CapabilityFilter検索 |
| `acp_stats_v3` | 拡張統計情報 |

#### ACP v2 コマンド

| コマンド | 説明 |
|---------|------|
| `acp_register_agent` | エージェントをタイプ指定で登録 |
| `acp_discover_agents` | ケーパビリティでエージェント検索 |
| `acp_list_agents` | 登録済みエージェント一覧 |
| `acp_get_agent` | IDでエージェント取得 |
| `acp_send_message` | エージェントにメッセージ送信 |
| `acp_broadcast` | 複数エージェントにブロードキャスト |
| `acp_get_task` | タスク状態取得 |
| `acp_stats` | オーケストレーター統計 |
| `acp_get_context` | 共有コンテキスト取得 |

### UI統合 (`src/App.tsx`)

- ACPエージェントパネル
- エージェント登録ボタン
- 翻訳エージェント検索
- 翻訳テストインターフェース
- エージェント一覧表示
- 統計情報表示

---

## 未実装・簡易実装の機能

### 1. エージェント実行エンジン

**現状:** オーケストレーターはエージェントカードの登録のみを行い、実際のタスク実行は行わない。

**必要な実装:**
- アダプターインスタンスの管理（現在は`Box<dyn AgentAdapter>`が`Send + Sync`でないため保存できない）
- PTYからの非同期出力読み取り
- タスクのライフサイクル管理
- ストリーミングレスポンス

### 2. PTYの非同期読み取り

**現状:** PTYからの出力読み取りは同期的で、ポーリングが必要。

**必要な実装:**
- async-compatibleなPTY実装
- イベントベースの出力通知
- タイムアウト処理

### 3. メッセージルーティング

**現状:** `acp_send_message`は単にPTYに転送するだけ。

**必要な実装:**
- 宛先エージェントへの実際のルーティング
- 複数エージェントへの並列分散
- レスポンスの集約

### 4. 入出力変換の完全実装

**現状:**
- `ClaudeCodeOutputConverter`は簡易パーサー
- コードブロック、思考、ツール使用の検出が未完成

**必要な実装:**
- マークダウン構文の完全パース
- ツール使用の検出と構造化
- エラーメッセージの検出

### 5. WebSocketトランスポート

**現状:** PTYトランスポートのみ実装。

**必要な実装:**
- WebSocketサーバー/クライアント
- リモートエージェント対応
- 認証・認可

### 6. エージェント発見プロトコル

**現状:** ローカルレジストリのみ。

**必要な実装:**
- ネットワーク上のエージェント発見
- mDNS/Broadcast対応
- エージェント広告（Advertise メッセージタイプ）

### 7. ハートビート・ヘルスチェック

**現状:** レジストリにハートビート機能はあるが、自動実行されない。

**必要な実装:**
- 定期的なハートビート送信
- エージェント死活監視
- 自動再接続

### 8. セキュリティ

**現状:** 認証なし。

**必要な実装:**
- エージェント認証
- メッセージ署名
- 暗号化通信

---

## 技術的な課題・制約

### 1. `portable-pty`の`Send + Sync`問題

`PtyManager`が使用する`portable_pty::PtyPair`等は`Send + Sync`を実装していない。そのため、以下の制約がある：

- アダプターを`Arc<Mutex>`でグローバルに保存できない
- 非同期コンテキストでの使用に制限がある

**回避策:** 現在はオーケストレーターでアダプターを直接管理せず、エージェントカードのみ管理している。

### 2. Tauri Stateの`Send + Sync`要件

Tauriの`State<T>`は`T: Send + Sync`を要求する。複雑なPTY構造を含む型は直接管理できない。

### 3. 非同期PTY読み取り

`portable-pty`は同期APIのみ提供。非同期的な読み取りには別スレッドでポーリングする必要がある。

---

## 次のステップ

### 完了済み

#### Phase 1: MVP完成 (ACP v2) ✅

1. [x] tmux基本機能検証 (Level 0.5) - 2025-02-19完了
2. [x] TmuxOrchestrator実装 - 2025-02-19完了
3. [x] フロントエンド統合テスト - 2025-02-19完了
4. [x] 状態検知の完全実装 (Level 1) - 2025-02-19完了
5. [x] 質問処理 (Level 3) - 2026-02-21完了

#### Phase 2: ACP v3 & A2A準拠 ✅ (2026-02-21完了)

1. [x] ACPEnvelope導入 - message.rs
2. [x] アドレス型拡張 - AddressType (single/multiple/broadcast/pipeline)
3. [x] エージェントカードv3 - A2A Agent Card準拠
4. [x] TypeScript型更新 - types.ts
5. [x] Pipeline通信実装 - pipeline.rs新規作成
6. [x] Broadcast実装 - tmux.rs拡張
7. [x] 新APIコマンド追加 - lib.rs

#### Phase 2.5: CLIベース移行 ✅ (2026-02-22完了)

tmux画面キャプチャからCLIベース（`--print --output-format stream-json`）に移行。

**解決した問題:**
| 問題 | 解決方法 |
|------|----------|
| 入出力の区別不可 | stdin/stdoutが明確に分離 |
| 状態検出の不確実性 | JSONイベントで全状態が明示 |
| 権限プロンプト検出 | `tool_result`のエラーで検出 |
| 完了検出 | `result`イベントで確実に検出 |

**新規ファイル:**
- `executor.rs` - ClaudeCodeExecutor（子プロセス管理）
- `stream_parser.rs` - stream-jsonパーサー
- `state_machine.rs` - AgentState enumと状態マシン
- `permission.rs` - 権限管理（自動許可/人間確認）

**新規Tauriコマンド:**
| コマンド | 説明 |
|---------|------|
| `executor_start` | CLIエグゼキューター起動 |
| `executor_execute` | タスク実行 |
| `executor_stop` | 停止 |
| `executor_get_state` | 現在の状態取得 |
| `executor_submit_permission` | 権限要求に回答 |
| `executor_is_running` | 起動状態確認 |

**Claude Code stream-jsonイベント:**
```json
{"type":"system","subtype":"init",...}           // 初期化
{"type":"assistant","message":{...}}             // 応答ストリーム
{"type":"result","subtype":"success",...}        // 完了
```

### 進行中・予定

#### Phase 3: Re-Voice統合

1. [ ] 字幕翻訳ワークフローの実装
2. [ ] 翻訳結果のVOICEVOX連携
3. [ ] エンドツーエンドの動作確認

#### Phase 4: マルチエージェント拡張

1. [ ] Pipeline UI - パイプライン定義・実行UI
2. [ ] エージェント選択UI改善
3. [ ] WebSocketトランスポート（リモートエージェント対応）
4. [ ] A2A完全準拠 - HTTP/SSE通信

---

## ファイル構成

```
src-tauri/src/
├── acp/
│   ├── mod.rs           # モジュール定義・再エクスポート
│   ├── message.rs       # ACPメッセージ型（v3: Envelope, AddressType）
│   ├── agent.rs         # A2A準拠AgentCard, Skill, Capabilities
│   ├── pipeline.rs      # パイプライン実行エンジン
│   ├── registry.rs      # エージェントレジストリ
│   ├── adapter.rs       # アダプターtrait群
│   ├── orchestrator.rs  # オーケストレーター
│   │
│   │  # === CLIベース (v3) ===
│   ├── executor.rs      # ClaudeCodeExecutor（子プロセス管理）
│   ├── stream_parser.rs # stream-jsonパーサー
│   ├── state_machine.rs # AgentState enumと状態マシン
│   ├── permission.rs    # 権限管理（自動許可/人間確認）
│   ├── runner.rs        # PipelineRunner（CLIベース版）
│   │
│   │  # === レガシー (tmuxベース) ===
│   ├── parser.rs        # 出力パーサー（状態検知）[廃止予定]
│   ├── poller.rs        # ステータスポーラー [廃止予定]
│   ├── tmux.rs          # tmux + Broadcast機能 [廃止予定]
│   │
│   ├── adapters/
│   │   ├── mod.rs
│   │   └── claude_code.rs  # Claude Codeアダプター
│   └── transport/
│       ├── mod.rs
│       └── pty.rs       # PTYトランスポート
├── pty.rs               # PTYマネージャー（既存）
└── lib.rs               # Tauriコマンド（v3追加）

src/
├── acp/
│   ├── index.ts         # ACPクライアント
│   ├── types.ts         # TypeScript型定義（A2A準拠 + AgentState）
│   └── transport/
│       └── index.ts     # Tauri IPCトランスポート
└── App.tsx              # UI統合
```

---

## テスト方法

```bash
# 開発サーバー起動
pnpm tauri dev

# 1. Claude Codeを起動
# 2. 「Register Agent」をクリック
# 3. 「Find Translators」で翻訳エージェントを検索
# 4. 翻訳テキストを入力して「Translate to Japanese」
```

---

## 参考リンク

- [Tauri Documentation](https://tauri.app/)
- [portable-pty crate](https://docs.rs/portable-pty/)
- [async-trait crate](https://docs.rs/async-trait/)
