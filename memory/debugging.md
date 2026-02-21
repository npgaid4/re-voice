# Re-Voice デバッグノウハウ

## Tauri開発の注意点

### Rustコード変更が反映されない
```bash
# 解決策
source ~/.cargo/env && cd src-tauri
cargo clean -p userseijidevaiappre-voice && cargo build
pnpm tauri dev
```

### log crate使用不可
```rust
// ❌ エラーになる
log::debug("module", "message");

// ✅ 代わりに使う（独自ロガー）
crate::log::info("module", "message");

// ✅ または eprintln!
eprintln!("[Module] message");
```
※ eprintln!はターミナルのみに出力（ログファイルには入らない）

---

## CLIベース (v3.5) 関連

### stream-jsonの確認
```bash
# 実際のイベント構造を確認
unset CLAUDECODE && echo "What is 2+2?" | claude --print --output-format stream-json --verbose
```

### Send問題の回避
`parking_lot::Mutex`を保持したまま`await`すると`Send`トレイトエラーになる。
```rust
// ❌ エラー
let guard = self.executor.lock();
guard.execute(&prompt).await?;

// ✅ 解決策1: spawn_blocking
tokio::task::spawn_blocking(move || { ... }).await?;

// ✅ 解決策2: ロックを分割
let needs_start = { let g = self.executor.lock(); g.is_none() };
if needs_start { ... }
```

### よくあるエラー（CLIベース）

| エラー | 原因 | 解決策 |
|-------|------|--------|
| `future is not Send` | MutexGuard保持中にawait | spawn_blocking使用 |
| `stream-json requires --verbose` | フラグ不足 | `--verbose`追加 |
| `Nested session` | CLAUDECODE環境変数 | `unset CLAUDECODE` |

---

## parser.rsの問題（レガシー）

### 選択メニュー検出が動作しない
- 原因不明（Tauriホットリロードの問題？）
- **回避策**: poller.rsで直接チェック
```rust
if content.contains("Enter to select") || content.contains("↑/↓ to navigate") {
    detected_status = AgentStatus::WaitingForInput { ... };
}
```

## tmux関連（レガシー）

### Claude Code起動確認
```bash
tmux list-sessions
tmux capture-pane -t re-voice -p
```

### ペインID確認
```bash
tmux list-panes -t re-voice -F "#{pane_id}"
```

## A2A / ACP v3 関連

### AgentCardのidフィールド
- A2A準拠化で `id` は `Option<String>` になった
- 値がない場合は `name` を代わりに使用
```rust
let agent_id = card.id.clone().unwrap_or_else(|| card.name.clone());
```

### capabilities → skills
- 旧: `AgentCard.capabilities: Vec<Capability>`
- 新: `AgentCard.skills: Option<Vec<Skill>>`
- 互換性: `Capability` は `Skill` のエイリアス

### Pipeline実行エラー
| エラー | 原因 | 解決策 |
|-------|------|--------|
| `NoStages` | stages空 | 1つ以上追加 |
| `ExecutionNotFound` | ID誤り | 正しいexecutionId確認 |
| `AlreadyRunning` | 二重実行 | 完了待機またはキャンセル |

## よくあるエラー

| エラー | 原因 | 解決策 |
|-------|------|--------|
| `use of unresolved module or unlinked crate 'log'` | log crate未導入 | crate::log使用 |
| Rust変更が反映されない | キャッシュ問題 | cargo clean |
| 選択メニュー検出不可 | parser.rs不具合 | poller.rsで直接チェック |
| UTF-8インデックスエラー | ❯は3バイト | strip_prefix()使用 |
| ネストセッションエラー | CLAUDECODE環境変数 | unset CLAUDECODE |

## ログ確認
```bash
tail -f src-tauri/logs/current.log
```

## テスト実行
```bash
# Rustテスト
cd src-tauri && cargo test --lib acp::

# 特定テスト
cargo test --lib acp::agent::tests
```
