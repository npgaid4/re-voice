# Re-Voice 要件・決定事項

## プロジェクト概要

YouTubeの外国語動画を日本語吹替版に変換するデスクトップアプリケーション。

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────────────┐
│                         Tauri App                               │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    ACP Orchestrator                      │   │
│  │         (エージェント管理・メッセージルーティング)          │   │
│  └───────────────────────┬─────────────────────────────────┘   │
│                          │ ACP Message                          │
│          ┌───────────────┼───────────────┐                      │
│          ▼               ▼               ▼                      │
│  ┌───────────────┐ ┌───────────────┐ ┌───────────────┐         │
│  │ Agent Adapter │ │ Agent Adapter │ │ Agent Adapter │         │
│  │  (Claude #1)  │ │  (Claude #2)  │ │   (Codex)     │         │
│  │ ┌───────────┐ │ │ ┌───────────┐ │ │ ┌───────────┐ │         │
│  │ │    PTY    │ │ │ │    PTY    │ │ │ │    PTY    │ │         │
│  │ └─────┬─────┘ │ │ └─────┬─────┘ │ │ └─────┬─────┘ │         │
│  └───────┼───────┘ └───────┼───────┘ └───────┼───────┘         │
│          │                 │                 │                  │
└──────────┼─────────────────┼─────────────────┼──────────────────┘
           ▼                 ▼                 ▼
    ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
    │ Claude Code  │  │ Claude Code  │  │    Codex     │
    │  Instance 1  │  │  Instance 2  │  │   (将来対応)  │
    └──────────────┘  └──────────────┘  └──────────────┘
```

- **フロントエンド**: React + TypeScript + HTML/CSS
- **バックエンド**: Rust + Tauri
- **通信プロトコル**: ACP (Agent Communication Protocol)
- **CLI通信**: PTY（疑似端末）でClaude Codeを仮想端末上で実行

## ACP (Agent Communication Protocol)

Re-Voiceおよび今後開発するすべてのAIアプリの基盤となる、マルチエージェント対応の汎用通信プロトコル。

### エージェント識別子

```
acp://<agent-id>@<host>/<instance>

例:
- acp://claude-code@localhost/main
- acp://codex@192.168.1.100/worker-3
```

### エージェントカード

```typescript
interface AgentCard {
  id: string;                    // "claude-code@localhost"
  protocol: "ACP/1.0";
  name: string;                  // "Claude Code"
  capabilities: Capability[];    // 能力宣言
  endpoint: string;              // 接続先
  transport: "pty" | "stdio" | "websocket" | "http";
}
```

### メッセージフォーマット

```typescript
interface ACPMessage {
  id: string;                    // UUID
  timestamp: string;             // ISO 8601
  from: string;                  // 送信元
  to: string | string[];         // 宛先
  type: "prompt" | "response" | "broadcast" | "discover" | "advertise" | "error";
  payload: { content: string; data?: object };
  metadata?: { priority?, ttl?, correlationId? };
}
```

## 技術スタック

| 項目 | 技術 |
|------|------|
| アプリフレームワーク | Tauri |
| フロントエンド | React + TypeScript |
| バックエンド | Rust |
| エージェント通信 | ACP (Agent Communication Protocol) |
| CLI通信 | PTY (portable-pty) |
| 動画ダウンロード | yt-dlp |
| 文字起こし | YouTube字幕を使用 |
| TTS | VOICEVOX（ローカル、既にインストール済み） |
| 動画合成 | ffmpeg |

## CLI AI の役割

Claude Code に以下を担当させる：

1. **翻訳** - 字幕の日本語翻訳
2. **品質向上** - 文脈調整・自然な表現への修正
3. **オーケストレーター** - ワークフロー全体の管理、エラー処理

## ワークフロー

```
YouTube URL入力
    ↓
yt-dlpで動画+字幕ダウンロード
    ↓
ACP経由でClaude Codeに字幕送信
    ↓
Claude Codeが字幕を翻訳・調整
    ↓
VOICEVOXで日本語音声生成
    ↓
ffmpegで動画と合成
    ↓
プレビュー・保存
```

## 出力形式

- **音声置換版** - 元の音声を日本語吹替に置き換え
- **二重音声版** - 元音声を残して日本語を重ねる
- ユーザーが選択可能

## UI要素（MVP）

- YouTube URL入力欄
- 変換オプション（置換/二重音声の選択、VOICEVOXのキャラ選択等）
- 処理の進行状況表示
- プレビュー/再生機能
- 完了通知・保存先指定
- **ACPエージェントパネル** - エージェント登録・検索・翻訳テスト

## 対応CLI AI（MVP）

- Claude Code のみ

## プロジェクト構造

```
re-voice/
├── src/                    # Reactフロントエンド
│   ├── App.tsx             # メインコンポーネント
│   ├── main.tsx            # エントリーポイント
│   ├── acp/                # ACPクライアント
│   │   ├── index.ts        # ACPクライアントクラス
│   │   ├── types.ts        # TypeScript型定義
│   │   └── transport/      # トランスポート
│   └── assets/             # 静的アセット
├── src-tauri/              # Rustバックエンド
│   ├── src/
│   │   ├── main.rs         # エントリーポイント
│   │   ├── lib.rs          # Tauriコマンド
│   │   ├── pty.rs          # PTYマネージャー
│   │   └── acp/            # ACPモジュール
│   │       ├── mod.rs      # モジュール定義
│   │       ├── message.rs  # メッセージ型
│   │       ├── agent.rs    # エージェントカード
│   │       ├── registry.rs # レジストリ
│   │       ├── adapter.rs  # アダプターtrait
│   │       ├── orchestrator.rs # オーケストレーター
│   │       ├── adapters/   # アダプター実装
│   │       │   └── claude_code.rs
│   │       └── transport/  # トランスポート
│   │           └── pty.rs
│   ├── Cargo.toml          # Rust依存関係
│   └── tauri.conf.json     # Tauri設定
├── package.json            # Node.js依存関係
├── CLAUDE.md               # Claude Code用ガイド
├── coding.md               # 実装詳細ドキュメント
└── handover.md             # このファイル（要件・決定事項）
```

## 開発コマンド

```bash
# 開発サーバー起動（フロントエンド + バックエンド同時起動）
pnpm tauri dev

# ビルド
pnpm tauri build

# Rustのみチェック
cd src-tauri && cargo check

# TypeScriptチェック
npx tsc --noEmit
```

## Tauri IPC コマンド

### レガシーPTYコマンド

| コマンド | 引数 | 戻り値 | 説明 |
|---------|------|--------|------|
| `spawn_claude` | - | `Result<String, String>` | Claude CodeをPTYで起動 |
| `send_to_claude` | `message: String` | `Result<(), String>` | Claude Codeにメッセージ送信 |
| `read_from_claude` | - | `Result<String, String>` | Claude Codeから出力読み取り |
| `is_claude_running` | - | `bool` | Claude Code起動確認 |
| `execute_command` | `command: String` | `Result<String, String>` | テスト用: コマンド実行 |

### ACPコマンド

| コマンド | 引数 | 戻り値 | 説明 |
|---------|------|--------|------|
| `acp_register_agent` | `agentType, instanceId` | `Result<String, String>` | エージェント登録 |
| `acp_discover_agents` | `capabilities?, tags?, transport?` | `Result<Vec<AgentCard>, String>` | エージェント検索 |
| `acp_list_agents` | - | `Vec<AgentCard>` | 全エージェント一覧 |
| `acp_get_agent` | `agentId` | `Option<AgentCard>` | エージェント取得 |
| `acp_send_message` | `to, content, from` | `Result<String, String>` | メッセージ送信 |
| `acp_broadcast` | `content, capabilities?, from` | `Result<Vec<String>, String>` | ブロードキャスト |
| `acp_get_task` | `taskId` | `Option<TaskState>` | タスク状態取得 |
| `acp_stats` | - | `OrchestratorStats` | 統計情報 |
| `acp_get_context` | - | `SharedContext` | 共有コンテキスト |

### YouTube/字幕コマンド

| コマンド | 引数 | 戻り値 | 説明 |
|---------|------|--------|------|
| `get_available_subtitles` | `url: String` | `Result<String, String>` | 字幕一覧取得 |
| `download_subtitles` | `url, lang, outputPath` | `Result<String, String>` | 字幕ダウンロード |
| `download_auto_subtitles` | `url, lang, outputPath` | `Result<String, String>` | 自動生成字幕DL |

### tmuxコマンド (ACP v2)

| コマンド | 引数 | 戻り値 | 説明 |
|---------|------|--------|------|
| `tmux_create_session` | - | `Result<String, String>` | tmuxセッション作成 |
| `tmux_spawn_agent` | `agentId, agentType, capabilities` | `Result<String, String>` | エージェント起動 |
| `tmux_capture_pane` | `agentId` | `Result<String, String>` | 画面キャプチャ |
| `tmux_send_message` | `agentId, message` | `Result<(), String>` | メッセージ送信 |
| `tmux_get_status` | `agentId` | `Result<String, String>` | 状態取得 |
| `tmux_list_agents` | - | `Result<Vec<Value>, String>` | エージェント一覧 |
| `tmux_destroy_session` | - | `Result<(), String>` | セッション破棄 |
| `tmux_start_polling` | `intervalMs?` | `Result<(), String>` | 自動ポーリング開始 |
| `tmux_stop_polling` | - | `Result<(), String>` | 自動ポーリング停止 |
| `tmux_is_polling` | - | `bool` | ポーリング状態 |

### tmuxイベント (Tauriイベント)

| イベント名 | ペイロード | 説明 |
|-----------|-----------|------|
| `tmux:status_changed` | `{ agent_id, old_status, new_status }` | エージェント状態変化 |
| `tmux:output_ready` | `{ agent_id, content, content_length }` | 出力準備完了 |

---

## セッション完了履歴

### 2025-02-19 セッション2: 状態検知と自動ポーリング実装 ✅ 完了

**完了した作業:**

1. **OutputParser実装** (`src-tauri/src/acp/parser.rs`)
   - 処理中パターン検出: `Thinking`, `Processing`, `Working`, スピナー文字
   - プロンプトパターン検出: `>`, `❯`
   - 質問パターン検出: `?`, `？`, 疑問語、選択肢提示
   - エラーパターン検出: `Error:`, `Failed:`, `Exception:` 等
   - ANSIエスケープシーケンス除去
   - 意味のあるコンテンツ抽出

2. **StatusPoller実装** (`src-tauri/src/acp/poller.rs`)
   - 500ms間隔での自動ポーリング（設定変更可能）
   - 状態変化時のTauriイベント発火:
     - `tmux:status_changed` - エージェントの状態が変化
     - `tmux:output_ready` - 出力の準備完了（Idle/WaitingForInput時）
   - スナップショットベースの変化検出

3. **Tauriコマンド追加** (`src-tauri/src/lib.rs`)
   - `tmux_start_polling` - 自動ポーリング開始
   - `tmux_stop_polling` - 自動ポーリング停止
   - `tmux_is_polling` - ポーリング状態取得

4. **フロントエンドイベント対応** (`src/App.tsx`)
   - `tmux:status_changed` イベントリスナー
   - `tmux:output_ready` イベントリスナー
   - 手動ポーリングからイベント駆動への移行

**ファイル構成の更新:**

```
src-tauri/src/acp/
├── mod.rs           # モジュール定義（parser, pollerを追加）
├── parser.rs        # NEW: 出力パーサー（状態検知）
├── poller.rs        # NEW: ステータスポーラー
├── tmux.rs          # TmuxOrchestrator（OutputParser統合）
└── ...              # その他既存ファイル
```

**動作確認:**

| テスト項目 | 結果 |
|-----------|------|
| Rustコンパイル | ✅ |
| TypeScriptコンパイル | ✅ |
| イベント発火 | 要確認 |

**次のステップ:**
- エージェント間通信の実装
- 実際の翻訳ワークフローの統合

---

### 2025-02-19 セッション1: ACP v2技術リスク検証 ✅ 完了

**完了した作業:**

1. **技術リスク検証 (Level 0.5)** - すべて成功
   - ✅ tmux capture-pane で出力取得
   - ✅ tmux send-keys -l で日本語送信
   - ✅ プロンプト検知パターン (❯, > )
   - ✅ Claude Code起動・操作

2. **TmuxOrchestrator実装** (`src-tauri/src/acp/tmux.rs`)
   - セッション作成/破棄
   - エージェント起動 (Claude Code, Codex, GenericShell)
   - ペインキャプチャ (ANSI付き/なし)
   - 状態検知 (Processing, Idle, WaitingForInput, Error)
   - メッセージ送信 (リテラルモード使用)

3. **Tauriコマンド追加** (`src-tauri/src/lib.rs`)
   - tmux_create_session
   - tmux_spawn_agent
   - tmux_capture_pane
   - tmux_send_message
   - tmux_get_status
   - tmux_list_agents
   - tmux_destroy_session

4. **フロントエンドUI** (`src/App.tsx`)
   - TmuxTestSectionコンポーネント
   - セッション作成/破棄
   - エージェント選択
   - 画面キャプチャ表示
   - ポーリング機能

**検証結果:**

| テスト項目 | 結果 |
|-----------|------|
| tmuxセッション作成 | ✅ 成功 |
| Claude Code起動 | ✅ 成功 |
| 画面キャプチャ | ✅ 成功 |
| 日本語メッセージ送信 | ✅ 成功 |
| 応答取得 | ✅ 成功 |

**次のステップ (Level 1):**
- 状態検知の精度向上
- 自動ポーリングとイベント通知
- エージェント間通信の実装

---

### 2025-02-18 セッション2: ACP実装

**完了した作業:**

1. **ACPメッセージ型** (`src-tauri/src/acp/message.rs`)
   - `ACPMessage` - ルーティング、ペイロード、メタデータ
   - `ACPFrame` - PTYトランスポート用フレーミング
   - 6種類のメッセージタイプ (prompt, response, broadcast, discover, advertise, error)

2. **エージェント型** (`src-tauri/src/acp/agent.rs`)
   - `AgentCard` - エージェント識別情報
   - `Capability` - 能力宣言
   - `DiscoveryQuery` - 検索クエリ

3. **レジストリ** (`src-tauri/src/acp/registry.rs`)
   - スレッドセーフなエージェント登録
   - ハートビート追跡
   - 古いエージェントのクリーンアップ

4. **アダプター** (`src-tauri/src/acp/adapter.rs`)
   - `AgentAdapter` trait - プロトコル変換
   - `InputConverter` / `OutputConverter` traits
   - `SharedContext` - マルチエージェント間のコンテキスト共有

5. **Claude Codeアダプター** (`src-tauri/src/acp/adapters/claude_code.rs`)
   - PTYベース通信
   - コンテキスト埋め込み
   - ANSI除去・出力パース（簡易）

6. **オーケストレーター** (`src-tauri/src/acp/orchestrator.rs`)
   - エージェント管理（簡易版）
   - タスク状態追跡
   - 統計情報

7. **TypeScript ACPクライアント** (`src/acp/`)
   - 型定義
   - ACPClient クラス
   - Tauri IPCトランスポート

8. **UI統合**
   - ACPエージェントパネル
   - エージェント登録・検索
   - 翻訳テストインターフェース

**動作確認済み機能:**

| 機能 | 状態 |
|------|------|
| Rustコンパイル | ✅ |
| TypeScriptコンパイル | ✅ |
| ACPエージェント登録 | ✅ UI実装済み |
| エージェント検索 | ✅ UI実装済み |
| 翻訳テストUI | ✅ 実装済み |

**未実装・簡易実装:**

- [ ] エージェント実行エンジン（タスクの実際の実行）
- [ ] PTYの非同期読み取り
- [ ] メッセージルーティング（現在はPTY転送のみ）
- [ ] 出力パーサーの完全実装
- [ ] WebSocketトランスポート
- [ ] エージェント発見プロトコル
- [ ] セキュリティ（認証・暗号化）

**技術的課題:**

- `portable-pty`が`Send + Sync`を実装していないため、アダプターをグローバルに保存できない
- オーケストレーターは現在エージェントカードのみ管理

---

### 2025-02-18 セッション1

**完了した作業:**

1. **プロジェクト初期化**
   - Tauri + React + TypeScript プロジェクト作成
   - Rust インストール（rustup使用）
   - 依存関係インストール完了

2. **PTY通信機能**
   - `portable-pty` クレートで仮想端末実装
   - Claude Code起動コマンド追加
   - PTY経由でのコマンド送受信機能

3. **フロントエンド基本UI**
   - YouTube URL入力欄
   - Claude Code起動ボタン
   - テストコマンドボタン
   - ログ表示エリア
   - ステータス表示（Claude Code起動状態）

4. **外部ツール確認・インストール**
   - yt-dlp インストール（Homebrew）
   - ffmpeg インストール（Homebrew）
   - VOICEVOX 動作確認（API: localhost:50021）

5. **字幕機能実装**
   - 字幕一覧取得
   - 字幕ダウンロード（手動/自動生成）
   - 言語選択UI（10言語対応）

6. **GitHub公開**
   - リポジトリ作成: https://github.com/npgaid4/re-voice
   - 初期コミット & プッシュ完了

**動作確認済み機能:**

| 機能 | 状態 |
|------|------|
| テストコマンド実行 | ✅ |
| YouTube動画情報取得 | ✅ |
| Claude Code起動 (PTY) | ✅ |
| ステータス表示 | ✅ |
| 字幕一覧取得 | ✅ |
| 字幕ダウンロード | ✅ |
| yt-dlp | ✅ インストール済み |
| ffmpeg | ✅ インストール済み |
| VOICEVOX | ✅ 動作確認済み |

---

## 不明点・検討事項

1. **Claude Code通信** ✅ ほぼ解決
   - ✅ PTY経由でClaude CodeのTUI出力を正しくパースできる（OutputParser実装）
   - ✅ リアルタイムで出力を取得する方法（StatusPollerでイベント駆動）
   - ✅ Claude Codeの応答完了をどう検知するか（プロンプト検知）
   - ⚠️ スピナー検出の精度向上が必要（Claude Code固有のパターン）

2. **字幕翻訳**
   - 字幕をどうやってClaude Codeに送るか（ファイル経由？直接テキスト？）
   - 翻訳結果のフォーマット（VTT形式を維持するか）
   - 長い字幕の場合の分割方法

3. **音声合成**
   - 字幕のタイムスタンプと音声の同期方法
   - 話速の調整（字幕の表示時間に合わせる）
   - 複数話者への対応

4. **動画合成**
   - 音声置換版と二重音声版の実装詳細
   - 処理の進捗表示方法

5. **エラーハンドリング**
   - 各段階でのエラー処理
   - ユーザーへのエラー表示方法

6. **パフォーマンス**
   - 長時間動画の処理時間
   - メモリ使用量

7. **ACP関連** ✅ tmuxで解決
   - ✅ `portable-pty`の`Send + Sync`問題 → tmux使用で回避
   - ⚠️ マルチエージェントの実際の並列実行（エージェント間通信未実装）

---

## 今後の拡張予定

### Phase 1: MVP完成
- [x] PTY出力のポーリング読み取り → StatusPoller実装
- [x] タスク完了検出 → OutputParser実装
- [x] フロントエンドでの出力表示 → イベント駆動化
- [ ] **エージェント間通信の実装** ← 次のステップ
- [ ] 翻訳結果のUI表示

### Phase 2: Re-Voice統合
- [ ] 字幕翻訳ワークフロー
- [ ] VOICEVOX連携
- [ ] エンドツーエンド動作

### Phase 3: 拡張
- [ ] マルチエージェント翻訳
- [ ] WebSocketトランスポート
- [ ] 他のCLI AIツール対応（Codex、Gemini CLI等）
