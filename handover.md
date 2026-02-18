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
| `get_available_subtitles` | `url: String` | `Result<String, String>` | 字幕一覧取得 |
| `download_subtitles` | `url, lang, outputPath` | `Result<String, String>` | 字幕ダウンロード |
| `download_auto_subtitles` | `url, lang, outputPath` | `Result<String, String>` | 自動生成字幕DL |

---

## セッション完了履歴

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

**未実装機能:**

- [ ] Claude Codeとの実際のメッセージ送受信（PTY経由）
- [ ] 字幕翻訳（Claude Code連携）
- [ ] VOICEVOX音声合成のアプリ統合
- [ ] 動画ダウンロード
- [ ] 音声・動画合成
- [ ] プレビュー機能
- [ ] ワークフロー自動化

---

## 不明点・検討事項

1. **Claude Code通信**
   - PTY経由でClaude CodeのTUI出力を正しくパースできるか？
   - リアルタイムで出力を取得する方法（ポーリング vs イベント駆動）
   - Claude Codeの応答完了をどう検知するか

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

---

## 今後の拡張予定

- 他のCLI AIツール対応（Codex、Gemini CLI等）
