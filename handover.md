# Re-Voice 要件・決定事項

## プロジェクト概要

YouTubeの外国語動画を日本語吹替版に変換するデスクトップアプリケーション。

## アーキテクチャ

```
┌─────────────────────────────────────────────────────┐
│                    Tauri App                        │
│  ┌─────────────┐     内部通信      ┌─────────────┐  │
│  │   Web UI    │ ◄──────────────► │ Rust Backend│  │
│  │  (React)    │      Tauri IPC   │             │  │
│  └─────────────┘                   │  ┌───────┐  │  │
│         ▲                          │  │  PTY  │  │  │
│         │                          │  │(仮想) │  │  │
│   ユーザーに見える                  │  └───┬───┘  │  │
│                                     │      │      │  │
│                                     │  ┌───▼───┐  │  │
│                                     │  │Claude │  │  │
│                                     │  │ Code  │  │  │
│                                     │  │(非表示)│  │  │
│                                     │  └───────┘  │  │
│                                     └─────────────┘  │
└─────────────────────────────────────────────────────┘
```

- **フロントエンド**: React + TypeScript + HTML/CSS
- **バックエンド**: Rust + Tauri
- **CLI通信**: PTY（疑似端末）でClaude Codeを仮想端末上で実行

## 技術スタック

| 項目 | 技術 |
|------|------|
| アプリフレームワーク | Tauri |
| フロントエンド | React + TypeScript |
| バックエンド | Rust |
| CLI AI通信 | PTY (portable-pty) |
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
CLI AIが字幕を翻訳・調整
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

## 対応CLI AI（MVP）

- Claude Code のみ

## プロジェクト構造

```
re-voice/
├── src/                    # Reactフロントエンド
│   ├── App.tsx             # メインコンポーネント
│   ├── main.tsx            # エントリーポイント
│   └── assets/             # 静的アセット
├── src-tauri/              # Rustバックエンド
│   ├── src/
│   │   ├── main.rs         # エントリーポイント
│   │   └── lib.rs          # ライブラリ（IPCコマンド定義）
│   ├── Cargo.toml          # Rust依存関係
│   └── tauri.conf.json     # Tauri設定
├── package.json            # Node.js依存関係
├── CLAUDE.md               # Claude Code用ガイド
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
```

## Tauri IPC コマンド（Rust → React）

| コマンド | 引数 | 戻り値 | 説明 |
|---------|------|--------|------|
| `spawn_claude` | - | `Result<String, String>` | Claude CodeをPTYで起動 |
| `send_to_claude` | `message: String` | `Result<(), String>` | Claude Codeにメッセージ送信 |
| `read_from_claude` | - | `Result<String, String>` | Claude Codeから出力読み取り |
| `is_claude_running` | - | `bool` | Claude Code起動確認 |
| `execute_command` | `command: String` | `Result<String, String>` | テスト用: コマンド実行 |

## 今後の拡張予定

- 他のCLI AIツール対応（Codex、Gemini CLI等）
