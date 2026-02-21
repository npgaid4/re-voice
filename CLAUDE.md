# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.
日本語を使用すること。

## 作業開始前に必ず読む

### 参照優先順位
1. **`memory/`** を先に参照（要点を圧縮、context節約）
2. **`docs/`** は詳細が必要な場合に参照

### memory/ ディレクトリ（優先）
- プロジェクトルート直下に作成: `<project>/memory/`
- **`memory/MEMORY.md`** - プロジェクト概要・実装状況・既知の問題
- **`memory/debugging.md`** - デバッグノウハウ・回避策
- **`memory/api.md`** - Tauri IPCコマンド詳細

### memory/ の更新ルール
- 各ファイルは1〜2行の追記にとどめる
- 詳細は docs/ に書き、memory/ には要点のみ
- 古い情報は削除して最新状態を維持
- docs/ 更新時に対応する memory/ も更新

### docs/ ディレクトリ（詳細用）
- **`docs/handover.md`** - プロジェクトの要件と決定事項
- **`docs/coding.md`** - 実装詳細、未実装機能、技術的課題
- **`docs/acp-design-v2-tmux.md`** - ACP v2設計（tmuxベース）

### docs/ 参照時の注意（Context節約）
- **直接Readしない** - docs/配下のファイルを読む際は、サブエージェント（Explore Agent）を使用
- **検索結果のみ受け取る** - 必要な情報を検索・抽出した結果だけをサブエージェントから受け取る
- **目的**: 大きなドキュメント全体をコンテキストに載せないため

### 不明点の解消
- **ウェブ検索を活用** - 不明なことがある場合は、サブエージェントを使ってウェブ検索を行い解消する
- **推測で実装しない** - 確信が持てない場合は必ず検索で確認してから実装する
- **承認を得る** - ウェブ検索で調査した結果は、人間に承認をもらってから使用する

### 更新タイミング
| タイミング | docs/ | memory/ |
|-----------|-------|---------|
| セッション終了時 | セッション履歴追記 | なし |
| 新機能実装時 | 詳細追記 | 1〜2行の要点追記 |
| 問題発見時 | 詳細追記 | 1〜2行の要点追記 |
| docs/更新時 | - | 対応する要点を更新 |

## プロジェクト概要

YouTubeの外国語動画を日本語吹替版に変換するTauriデスクトップアプリ。

## 開発ツール
pnpm

## 開発コマンド

```bash
# 開発サーバー起動
pnpm tauri dev

# ビルド
pnpm tauri build

# Rustのみチェック
cd src-tauri && cargo check
```

## アーキテクチャ

- **フロントエンド**: React + TypeScript（`src/`）
- **バックエンド**: Rust + Tauri（`src-tauri/`）
- **通信**: Tauri IPC（フロントエンド ↔ Rust）
- **CLI AI通信**: PTY（Rust側でClaude Codeを仮想端末上で実行）

## 注意事項

- Claude CodeはPTY経由で非表示実行される
- ユーザーにはWeb UIのみ表示
- 外部依存: yt-dlp, ffmpeg, VOICEVOX
