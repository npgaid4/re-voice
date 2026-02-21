# Re-Voice Project Memory

## 詳細ドキュメント
- `debugging.md` - デバッグノウハウ・回避策
- `api.md` - Tauri IPCコマンド詳細（v3含む）

## 概要
YouTube外国語動画→日本語吹替版変換Tauriアプリ

## アーキテクチャ
- Frontend: React + TypeScript (`src/`)
- Backend: Rust + Tauri (`src-tauri/`)
- Agent通信: **CLIベース** (`--print --output-format stream-json`)

## CLIベース移行完了 (2026-02-22)

tmux画面キャプチャからCLIベース（stream-json）に移行完了。
- **解決**: 入出力区別、状態検出、権限プロンプト処理
- **新ファイル**: `executor.rs`, `stream_parser.rs`, `state_machine.rs`, `permission.rs`
- **runner.rs**: CLIベースに書き換え済み
- **フロントエンド型**: `src/acp/types.ts` にAgentState型追加

### 新Tauriコマンド
| コマンド | 用途 |
|---------|------|
| `executor_start` | CLIエグゼキューター起動 |
| `executor_execute` | タスク実行 |
| `executor_stop` | 停止 |
| `executor_get_state` | 状態取得 |
| `executor_submit_permission` | 権限回答 |

### 状態定義 (AgentState)
- `initializing`, `idle`, `processing`, `waiting_for_permission`
- `waiting_for_input`, `error`, `completed`

## A2A Protocol 準拠 (2026-02-21)

### Agent Card構造
```json
{
  "name": "Claude Code",
  "url": "acp://claude-code@localhost/main",
  "protocolVersion": "0.3.0",
  "capabilities": { "streaming": true },
  "skills": [{ "id": "translation", "tags": ["multilingual"] }]
}
```

### 主要フィールド変更
| 旧 | 新 | 説明 |
|---|---|------|
| `capabilities` | `skills` | エージェントが実行できるタスク |
| `protocol` | `protocolVersion` | "0.3.0" |
| - | `capabilities` | 技術的機能 (streaming等) |
| - | `provider` | 組織情報 |

## 主要ファイル
| ファイル | 内容 |
|---------|------|
| `src-tauri/src/acp/executor.rs` | ClaudeCodeExecutor (CLI) |
| `src-tauri/src/acp/stream_parser.rs` | stream-jsonパーサー |
| `src-tauri/src/acp/state_machine.rs` | 状態マシン |
| `src-tauri/src/acp/permission.rs` | 権限管理 |
| `src-tauri/src/acp/runner.rs` | PipelineRunner (4ステージ対応) |
| `src-tauri/src/acp/pipeline.rs` | PipelineExecutor |
| `src-tauri/src/acp/subtitle_parser.rs` | VTTパーサー (Phase 3) |
| `src-tauri/src/voicevox.rs` | VOICEVOX APIクライアント |
| `src-tauri/src/youtube.rs` | yt-dlpラッパー |
| `src/components/PipelineRunner.tsx` | パイプライン実行UI |
| `src/acp/types.ts` | TypeScript型 (AgentState含む) |

## 開発コマンド
```bash
pnpm tauri dev                        # 開発起動
source ~/.cargo/env && cargo check    # Rustのみ
```

## 実装状況
| Level | 内容 | 状態 |
|-------|------|------|
| 0-3 | tmux基本〜質問処理 | ✅ (レガシー) |
| 4 | マルチエージェント基盤 | ✅ (ACP v3) |
| 5-7 | 通信/自動化/安定化 | ✅ (CLI移行完了) |
| 8 | CLIベース実行 | ✅ 新規実装 |
| **Phase 3** | **4ステージパイプライン** | ✅ **完了** |

## Phase 3: 4ステージパイプライン (2026-02-22)

### アーキテクチャ
```
Stage1: 字幕DL (yt-dlp/Rust) →
Stage2: VTT解析 (Rust) →
Stage3: 翻訳 (Claude Code CLI) →
Stage4: 音声生成 (VOICEVOX/Rust)
```

### 新規ファイル
| ファイル | 内容 |
|---------|------|
| `src-tauri/src/acp/subtitle_parser.rs` | VTTパーサー |
| `src/components/PipelineRunner.tsx` | パイプラインUI |
| `src/components/PipelineRunner.css` | スタイル |

### 主要変更
- `runner.rs`: CLIエグゼキューター統合、4ステージ化、Rust直接実行ステージ追加
- `lib.rs`: cli_executorを`Arc<RwLock<Option<...>>>`に変更

### テスト結果
- subtitle_parser: 8 passed
- runner: 3 passed
- 統合: 字幕DL、VTT解析、VOICEVOX生成 全て成功

## 既知の問題と回避策
1. **Rust変更反映なし** → `cargo clean -p userseijidevaiappre-voice && cargo build`
2. **log crate不可** → `crate::log::info()` 使用
3. **ネストセッションエラー** → `unset CLAUDECODE && claude code`
4. **UTF-8インデックスエラー** → `strip_prefix()` 使用

## イベント
| イベント | 用途 |
|---------|------|
| `executor:state_changed` | 状態変化 (新) |
| `executor:permission_required` | 権限要求 (新) |
| `pipeline:progress` | 進捗通知 |
| `tmux:status_changed` | 状態変化 (レガシー) |
| `tmux:output_ready` | 出力完了 (レガシー) |

## ログ場所
`src-tauri/logs/current.log`
