# Re-Voice API リファレンス

## CLI Executor コマンド (v3.5 - 推奨)

| コマンド | 引数 | 説明 |
|---------|------|------|
| `executor_start` | workingDir?, allowedTools[]?, sessionId? | CLIエグゼキューター起動 |
| `executor_execute` | prompt | タスク実行 |
| `executor_stop` | - | 停止 |
| `executor_get_state` | - | 現在のAgentState取得 |
| `executor_submit_permission` | requestId, allow, always | 権限要求に回答 |
| `executor_is_running` | - | 起動状態確認 |

### AgentState値

| 値 | 意味 |
|----|------|
| `initializing` | 起動中 |
| `idle` | アイドル（次のタスク待ち） |
| `processing` | 処理中（currentTool, startedAt） |
| `waiting_for_permission` | 権限要求中 |
| `waiting_for_input` | 入力待ち |
| `error` | エラー（message, recoverable） |
| `completed` | 完了（output） |

### Executorイベント

| イベント | ペイロード | 発火タイミング |
|---------|-----------|---------------|
| `executor:state_changed` | `{old_state, new_state}` | 状態変化時 |
| `executor:permission_required` | `{request_id, tool_name, tool_input}` | 権限要求時 |
| `pipeline:progress` | `{execution_id, stage_index, status, message}` | 進捗更新時 |

### 使用例（推奨）

```typescript
// エグゼキューター起動
const sessionId = await invoke('executor_start', {
  workingDir: '/path/to/project',
  allowedTools: ['Read', 'Grep', 'Glob', 'Edit', 'Write']
});

// タスク実行
const result = await invoke('executor_execute', {
  prompt: 'Translate this file to Japanese'
});

// 状態監視
listen('executor:state_changed', (event) => {
  console.log('State:', event.payload.new_state);
});

// 権限要求への回答
listen('executor:permission_required', (event) => {
  const { request_id, tool_name } = event.payload;
  // ユーザーに確認後
  await invoke('executor_submit_permission', {
    requestId: request_id,
    allow: true,
    always: false
  });
});
```

---

## ACP v3 コマンド（新規）

| コマンド | 引数 | 説明 |
|---------|------|------|
| `acp_define_pipeline` | name, stages[] | パイプライン定義 |
| `acp_execute_pipeline` | pipelineId | パイプライン実行 |
| `acp_get_pipeline_status` | executionId | 実行状態取得 |
| `acp_cancel_pipeline` | executionId | キャンセル |
| `acp_list_pipelines` | - | パイプライン一覧 |
| `acp_list_active_executions` | - | アクティブ実行一覧 |
| `acp_broadcast_v3` | content, filter? | ブロードキャスト（フィルター対応） |
| `acp_broadcast_to_idle` | content, filter? | アイドルエージェントのみ |
| `acp_discover_agents_v3` | filter? | CapabilityFilter検索 |
| `acp_stats_v3` | - | 拡張統計情報 |

## tmuxコマンド (ACP v2)

| コマンド | 引数 | 説明 |
|---------|------|------|
| `tmux_create_session` | - | セッション作成 |
| `tmux_spawn_agent` | agentId, agentType, capabilities | エージェント起動 |
| `tmux_capture_pane` | agentId | 画面キャプチャ |
| `tmux_send_message` | agentId, message | メッセージ送信 |
| `tmux_get_status` | agentId | 状態取得 |
| `tmux_list_agents` | - | エージェント一覧 |
| `tmux_destroy_session` | - | セッション破棄 |
| `tmux_start_polling` | intervalMs? | 自動ポーリング開始 |
| `tmux_stop_polling` | - | 自動ポーリング停止 |
| `tmux_is_polling` | - | ポーリング状態 |
| `tmux_answer_question` | agentId, answer | 質問に回答 |
| `tmux_get_agent_status` | agentId | エージェント状態 |

## tmuxイベント (Tauri)

| イベント | ペイロード | 発火タイミング |
|---------|-----------|---------------|
| `tmux:status_changed` | `{agent_id, old_status, new_status}` | 状態変化時 |
| `tmux:output_ready` | `{agent_id, content, content_length}` | 出力完了時 |
| `tmux:question` | `{agent_id, question, question_id, context}` | 質問発生時 |

## AgentStatus値

| 値 | 意味 |
|----|------|
| `Initializing` | 起動中 |
| `Processing` | 処理中 |
| `Idle` | アイドル（プロンプト待ち） |
| `WaitingForInput:{question}` | 質問待ち |
| `Error:{message}` | エラー |
| `Unknown` | 不明 |

## Pipeline使用例

```typescript
// パイプライン定義
const pipelineId = await invoke('acp_define_pipeline', {
  name: 'translate-pipeline',
  stages: [
    { name: 'translate', agent: { id: 'translator@local' } },
    { name: 'review', agent: { id: 'reviewer@local' } }
  ]
});

// 実行
const execution = await invoke('acp_execute_pipeline', { pipelineId });

// 状態取得
const status = await invoke('acp_get_pipeline_status', {
  executionId: execution.execution_id
});
```

## Broadcast使用例

```typescript
// CapabilityFilter付きブロードキャスト
const result = await invoke('acp_broadcast_v3', {
  content: 'Translate this text',
  filter: {
    capabilities: ['translation'],
    tags: ['multilingual']
  }
});
// result: { success: [...], failures: [...], total_sent: N }
```

## フロントエンド使用例

```typescript
// セッション作成
await invoke('tmux_create_session');

// エージェント起動
await invoke('tmux_spawn_agent', {
  agentId: 'claude-1',
  agentType: 'ClaudeCode',
  capabilities: ['translation', 'code-review']
});

// イベントリスナー
listen('tmux:question', (event) => {
  const { agent_id, question } = event.payload;
  // 質問ダイアログ表示
});

// 回答送信
await invoke('tmux_answer_question', {
  agentId: 'claude-1',
  answer: '1'
});
```
