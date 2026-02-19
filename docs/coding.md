# ACP (Agent Communication Protocol) 実装ドキュメント

## 概要

Re-Voiceおよび今後開発するすべてのAIアプリの基盤となる、マルチエージェント対応の汎用通信プロトコルを実装した。

## 実装済み機能

### Rustバックエンド (`src-tauri/src/acp/`)

#### 1. メッセージ型 (`message.rs`)

- `ACPMessage` - コアメッセージフォーマット
  - ルーティング (from, to)
  - ペイロード (content, data)
  - メタデータ (priority, ttl, correlationId)
- `ACPFrame` - PTYトランスポート用フレーミング (`<ACP>{json}</ACP>`)
- `MessageType` - prompt, response, broadcast, discover, advertise, error

#### 2. エージェント型 (`agent.rs`)

- `AgentCard` - エージェントの識別情報
  - ID, 名前, エンドポイント
  - ケーパビリティ一覧
  - トランスポート種別
- `Capability` - 能力宣言 (ID, 名前, タグ)
- `DiscoveryQuery` - エージェント検索クエリ
  - ケーパビリティでフィルタ
  - タグでフィルタ
  - トランスポート種別でフィルタ

#### 3. レジストリ (`registry.rs`)

- `AgentRegistry` - スレッドセーフなエージェント登録管理
- ハートビート追跡
- 古いエージェントの自動クリーンアップ
- ケーパビリティによる検索

#### 4. アダプター (`adapter.rs`)

- `AgentAdapter` trait - プロトコル変換レイヤー
- `InputConverter` trait - ACP → ネイティブCLI入力
- `OutputConverter` trait - ネイティブCLI出力 → ACP
- `TaskRequest`, `TaskResult` - タスク実行型
- `SharedContext` - マルチエージェント間のコンテキスト共有

#### 5. Claude Codeアダプター (`adapters/claude_code.rs`)

- PTYベースのClaude Code通信
- コンテキスト埋め込み（共有ファイル、会話履歴）
- ANSIエスケープシーケンス除去
- 出力パース（簡易実装）

#### 6. オーケストレーター (`orchestrator.rs`)

- エージェント管理（簡易版）
- タスク状態追跡
- 統計情報（完了タスク数、失敗数など）
- 共有コンテキスト管理

#### 7. トランスポート (`transport/pty.rs`)

- PTYトランスポート実装
- ACPフレームの送受信

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

### Phase 1: MVP完成 (ACP v2)

1. [x] tmux基本機能検証 (Level 0.5) - 2025-02-19完了
   - ✅ tmux capture-pane で出力取得可能
   - ✅ tmux send-keys -l で日本語送信可能
   - ✅ プロンプト検知 (❯, > ) 可能
   - ✅ Claude Code起動・操作確認
2. [x] TmuxOrchestrator実装 - 2025-02-19完了
   - src-tauri/src/acp/tmux.rs
3. [x] フロントエンド統合テスト - 2025-02-19完了
   - TmuxTestSectionコンポーネント実装
   - 全機能の動作確認完了
4. [x] 状態検知の完全実装 (Level 1) - 2025-02-19完了
   - ✅ OutputParser実装 (src-tauri/src/acp/parser.rs)
     - Processing/Idle/WaitingForInput/Error検出
     - ANSIエスケープシーケンス処理
   - ✅ StatusPoller実装 (src-tauri/src/acp/poller.rs)
     - 自動ポーリング（500ms間隔）
     - イベント発火（tmux:status_changed, tmux:output_ready）
   - ✅ フロントエンドイベント対応
     - イベントリスナー実装

### Phase 2: Re-Voice統合

1. [ ] 字幕翻訳ワークフローの実装
2. [ ] 翻訳結果のVOICEVOX連携
3. [ ] エンドツーエンドの動作確認

### Phase 3: 拡張

1. [ ] マルチエージェント翻訳（翻訳→品質チェック）
2. [ ] WebSocketトランスポート
3. [ ] エージェント発見プロトコル

---

## ファイル構成

```
src-tauri/src/
├── acp/
│   ├── mod.rs           # モジュール定義・再エクスポート
│   ├── message.rs       # ACPメッセージ型
│   ├── agent.rs         # エージェントカード・ケーパビリティ
│   ├── registry.rs      # エージェントレジストリ
│   ├── adapter.rs       # アダプターtrait群
│   ├── orchestrator.rs  # オーケストレーター
│   ├── parser.rs        # NEW: 出力パーサー（状態検知）
│   ├── poller.rs        # NEW: ステータスポーラー
│   ├── tmux.rs          # tmuxオーケストレーター
│   ├── adapters/
│   │   ├── mod.rs
│   │   └── claude_code.rs  # Claude Codeアダプター
│   └── transport/
│       ├── mod.rs
│       └── pty.rs       # PTYトランスポート
├── pty.rs               # PTYマネージャー（既存）
└── lib.rs               # Tauriコマンド

src/
├── acp/
│   ├── index.ts         # ACPクライアント
│   ├── types.ts         # TypeScript型定義
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
