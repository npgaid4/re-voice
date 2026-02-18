# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 作業開始前に必ず読む

**`handover.md`** を参照して、プロジェクトの要件と決定事項を確認してください。

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
