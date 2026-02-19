# Re-Voice Handover Document

## プロジェクト概要

YouTubeの外国語動画を日本語吹替版に変換するTauriデスクトップアプリ。

## 実装済み機能

### 1. PTY通信システム (`src-tauri/src/pty.rs`)

Claude Codeとの通信を管理するPTYマネージャー。

**実装内容:**
- `PtyManager`: PTYの生成・管理
- バックグラウンドリーダースレッド（ノンブロッキング読み取り）
- ANSIエスケープシーケンス処理（`process_ansi`関数）
- 出力バッファリング（100KB制限）

**重要な実装詳細:**
- メッセージ送信時は**文字列と改行を分けて送信**する必要がある
  ```rust
  // OK: 分けて送信
  self.write_input(message.as_bytes())?;
  std::thread::sleep(std::time::Duration::from_millis(100));
  self.write_input(b"\r")?;

  // NG: 一度に送信（応答が返ってこない）
  self.write_input(format!("{}\r\n", message).as_bytes())?;
  ```

### 2. 自動応答システム (`PromptDetector`)

Claude Code起動時の確認プロンプトに自動応答。

**対応プロンプト:**
- Trust verification（"trust this folder"）
- Bypass permissions（"dangerously-skip-permissions"）

**自動応答ロジック:**
1. 選択肢番号だけを送信（Enterなし）
2. 300ms待機
3. Enter (`\r`) を送信

### 3. 入力要求検出・ダイアログ機能

Claude Codeが認証待ちやユーザー入力待ちで停止した状態を検出し、ダイアログを表示。

**検出パターン (`PromptType` enum):**
- `AuthenticationRequired`: OAuth期限切れ、API 401エラー、`/login` 必要
- `UserInputRequired`: 選択肢付きプロンプト
- `Choice`: 自動応答可能な選択肢
- `InputReady`: 通常の入力待ち

**イベントフロー:**
1. Rust側で `PtyEvent::InputRequired` イベント発火
2. フロントエンドで `pty-input-required` イベント受信
3. ダイアログ表示（コンテキスト + 入力欄 + ボタン）
4. ユーザー入力を送信または `/login` 実行

### 4. 翻訳結果フィルタリング機能 (`filterPtyOutput`)

PTY出力から不要なUI要素を除去し、翻訳テキストのみを抽出。

**フィルタリング対象:**
- `ctrl+g to edit in VS Code`
- `esc to interrupt`
- `? for shortcuts`
- `─────────────` 区切り線
- スピナーアニメーション記号（✢ ✳ ✶ ✻ など）
- `❯ ` プロンプト
- スピナーテキスト（`Harmonizing…`, `Seasoning…` など）
- 余分なスペース・改行

### 5. イベント駆動アーキテクチャ

**Rust側:**
- `PtyEvent` enum: `Output`, `Prompt`, `Error`, `InputRequired`
- `set_event_callback()` でコールバック登録
- Tauriイベントでフロントエンドに通知

**フロントエンド側:**
- `pty-output`: 出力チャンク受信（フィルタリング済み）
- `pty-prompt`: 入力待ち状態
- `pty-error`: エラー通知
- `pty-input-required`: 入力要求（ダイアログ表示）

### 6. イベントリスナー二重登録問題の修正

**問題:**
React StrictMode（開発モード）で useEffect が2回実行され、`listen()` の Promise が解決される前にクリーンアップが走り、結果として2つのリスナーが登録されていた。

**解決策:**
```typescript
useEffect(() => {
  let mounted = true;
  const unlisteners: UnlistenFn[] = [];

  listen<string>("pty-output", (event) => {
    if (!mounted) return;
    // ...
  }).then((unlisten) => {
    if (mounted) {
      unlisteners.push(unlisten);
    } else {
      unlisten(); // 既にアンマウント済みなら即座に解除
    }
  });

  return () => {
    mounted = false;
    unlisteners.forEach((unlisten) => unlisten());
  };
}, []);
```

### 7. ACP (Agent Communication Protocol)

マルチエージェント対応の汎用通信プロトコル（基本実装済み）。

**実装済みモジュール:**
- `acp/agent.rs`: AgentCard, DiscoveryQuery, Transport
- `acp/orchestrator.rs`: AgentOrchestrator
- `acp/registry.rs`: AgentRegistry
- `acp/adapter.rs`: AgentAdapter trait, SharedContext
- `acp/adapters/claude_code.rs`: Claude Code用アダプタ
- `acp/message.rs`: ACPMessage
- `acp/transport/pty.rs`: PTYトランスポート

**TypeScript クライアント:**
- `src/acp/index.ts`: ACPClient クラス
- `src/acp/types.ts`: 型定義

### 8. フロントエンド (`src/App.tsx`)

- Claude Code起動ボタン
- エージェント登録
- 翻訳テスト機能（フィルタリング済み結果表示）
- 入力要求ダイアログ
- デバッグ用PTY操作ボタン

## 不明点・調査が必要な事項

### 1. なぜ「文字列と改行を分けて送信」が必要なのか

**現象:**
- `message + "\r\n"` を一度に送信 → 応答なし
- `message` 送信 → 100ms待機 → `"\r"` 送信 → 成功

**推測される原因:**
- PTYのバッファリングの問題
- Claude Codeの入力処理のタイミング
- portable-ptyライブラリの挙動

### 2. ルールベースのプロンプト検出の限界

**現状:**
- 文字列パターンマッチングで検出
- 新しいパターンが出るたびにコード修正が必要

**将来のアプローチ:**
- **Controller Agent + Worker Agent 構成**
  - Controller Agent: Workerの状態を監視・判定
  - Worker Agent: 実際のタスク実行
- AIベースの状態判定（軽量モデル使用）

### 3. フィルタリングの完全性

**現状:**
- 正規表現ベースのフィルタリング
- 新しいUI要素が出る可能性

**課題:**
- Claude CodeのUI変更に追随する必要
- 過剰フィルタリングで重要な情報を消すリスク

## 未実装機能

### 優先度: 高

1. **タイムアウト検出**
   - 一定時間無応答の場合にユーザーに通知
   - `last_activity` フィールドは実装済みだが未使用

2. **Controller Agent連携**
   - Worker Agentの状態を監視する別エージェント
   - AIベースの状態判定

3. **認証フローの自動化**
   - `/login` 実行後の認証完了検出
   - ブラウザ認証の完了待ち

### 優先度: 中

4. **YouTube機能の実装**
   - 字幕ダウンロード（yt-dlp） ※Rust側は実装済み
   - 字幕解析・セグメント分割
   - 翻訳キューイング

5. **音声合成連携**
   - VOICEVOX API連携
   - 音声生成・同期

6. **動画編集機能**
   - ffmpeg連携
   - 音声差し替え

### 優先度: 低

7. **ACPの完全実装**
   - マルチエージェント対応
   - エージェント間通信
   - 共有コンテキスト管理

8. **UI/UX改善**
   - プログレス表示
   - 設定画面
   - 履歴機能

## 技術スタック

- **フロントエンド**: React + TypeScript
- **バックエンド**: Rust + Tauri v2
- **PTY通信**: portable-pty
- **外部依存**:
  - yt-dlp（YouTube字幕取得）
  - ffmpeg（動画編集）
  - VOICEVOX（音声合成）

## 開発コマンド

```bash
# 開発サーバー起動
pnpm tauri dev

# Rustチェック
/Users/eiji/.cargo/bin/cargo check --manifest-path src-tauri/Cargo.toml

# ビルド
pnpm tauri build
```

## デバッグのヒント

1. **PTY通信のログ確認**
   - ターミナルに `[PTY READER]` ログが出力される
   - Raw bytes と After process_ansi を比較

2. **フロントエンドのイベント確認**
   - ブラウザの開発者ツールでコンソール確認
   - `pty-output`, `pty-prompt`, `pty-error`, `pty-input-required` イベント

3. **Claude Codeの状態確認**
   - 「子プロセス確認」ボタンでPIDと生存確認
   - 「PTY: 出力取得」でバッファ確認

4. **翻訳テスト用英語**
   ```
   Artificial intelligence is changing the world rapidly.
   ```

## 最終更新

2026-02-19
