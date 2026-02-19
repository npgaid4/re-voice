# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.
日本語を使用すること。

## 作業開始前に必ず読む

- **`docs/handover.md`** - プロジェクトの要件と決定事項
- **`docs/coding.md`** - 実装詳細、未実装機能、技術的課題
- **`docs/acp-design-v2-tmux.md`** - ACP v2設計（tmuxベース）

## プロジェクト概要

YouTubeの外国語動画を日本語吹替版に変換するTauriデスクトップアプリ。

## 開発コマンド

```bash
# 開発サーバー起動
npm run tauri dev

# ビルド
npm run tauri build

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
