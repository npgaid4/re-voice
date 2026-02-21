import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { ACPClient, AgentCard, OrchestratorStats } from "./acp";
import PipelineRunner from "./components/PipelineRunner";
import "./App.css";

// ============================================================================
// tmux Test Component (ACP v2 PoC)
// ============================================================================

interface TmuxAgent {
  agent_id: string;
  pane_id: string;
  agent_type: string;
  capabilities: string[];
  status: string;
}

// 質問イベントの型
interface QuestionEvent {
  agent_id: string;
  question: string;
  question_id: string;
  context: string;
}

// 質問ダイアログの状態
interface QuestionDialogState {
  visible: boolean;
  agent_id: string;
  question: string;
  question_id: string;
  context: string;
  userInput: string;
}

function TmuxTestSection({ addOutput }: { addOutput: (text: string) => void }) {
  const [sessionCreated, setSessionCreated] = useState(false);
  const [agents, setAgents] = useState<TmuxAgent[]>([]);
  const [selectedAgent, setSelectedAgent] = useState<string>("");
  const [message, setMessage] = useState("");
  const [capturedOutput, setCapturedOutput] = useState("");
  const [polling, setPolling] = useState(false);

  // 質問ダイアログの状態
  const [questionDialog, setQuestionDialog] = useState<QuestionDialogState>({
    visible: false,
    agent_id: "",
    question: "",
    question_id: "",
    context: "",
    userInput: "",
  });

  // イベントリスナー登録（状態変化と出力準備完了）
  useEffect(() => {
    let mounted = true;
    const unlisteners: UnlistenFn[] = [];

    // 状態変化イベント
    listen<{ agent_id: string; old_status: string; new_status: string }>(
      "tmux:status_changed",
      (event) => {
        if (!mounted) return;
        const { agent_id, old_status, new_status } = event.payload;
        addOutput(`[TMUX EVENT] ${agent_id}: ${old_status} -> ${new_status}`);

        // エージェント一覧を更新
        refreshAgents();
      }
    ).then((unlisten) => {
      if (mounted) {
        unlisteners.push(unlisten);
      } else {
        unlisten();
      }
    });

    // 出力準備完了イベント
    listen<{ agent_id: string; content: string; content_length: number }>(
      "tmux:output_ready",
      (event) => {
        if (!mounted) return;
        const { agent_id, content, content_length } = event.payload;
        addOutput(`[TMUX EVENT] Output ready from ${agent_id}: ${content_length} chars`);

        // 選択中のエージェントの出力を更新
        if (agent_id === selectedAgent) {
          setCapturedOutput(content);
        }
      }
    ).then((unlisten) => {
      if (mounted) {
        unlisteners.push(unlisten);
      } else {
        unlisten();
      }
    });

    // 質問イベント（Level 3: 質問処理）
    listen<QuestionEvent>(
      "tmux:question",
      (event) => {
        if (!mounted) return;
        const { agent_id, question, question_id, context } = event.payload;
        addOutput(`[TMUX EVENT] Question from ${agent_id}: ${question}`);

        // 質問ダイアログを表示
        setQuestionDialog({
          visible: true,
          agent_id,
          question,
          question_id,
          context,
          userInput: "",
        });
      }
    ).then((unlisten) => {
      if (mounted) {
        unlisteners.push(unlisten);
      } else {
        unlisten();
      }
    });

    return () => {
      mounted = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedAgent]);

  // セッション作成
  const handleCreateSession = async () => {
    try {
      const result = await invoke<string>("tmux_create_session");
      addOutput(`[TMUX] ${result}`);
      setSessionCreated(true);
      // 自動的に main エージェントを追加
      setAgents([{
        agent_id: "main",
        pane_id: "%0",
        agent_type: "GenericShell",
        capabilities: [],
        status: "Idle",
      }]);
      setSelectedAgent("main");
    } catch (e) {
      addOutput(`[TMUX] Error: ${e}`);
    }
  };

  // Claude Code起動
  const handleSpawnClaudeCode = async () => {
    try {
      const paneId = await invoke<string>("tmux_spawn_agent", {
        agentId: "claude-code",
        agentType: "claude-code",
        capabilities: ["translation", "code-review"],
      });
      addOutput(`[TMUX] Claude Code spawned: ${paneId}`);
      await refreshAgents();
    } catch (e) {
      addOutput(`[TMUX] Error spawning agent: ${e}`);
    }
  };

  // エージェント一覧更新
  const refreshAgents = async () => {
    try {
      const list = await invoke<TmuxAgent[]>("tmux_list_agents");
      setAgents(list);
      if (list.length > 0 && !selectedAgent) {
        setSelectedAgent(list[0].agent_id);
      }
    } catch (e) {
      addOutput(`[TMUX] Error listing agents: ${e}`);
    }
  };

  // ペイン内容キャプチャ
  const handleCapture = async () => {
    if (!selectedAgent) return;
    try {
      const content = await invoke<string>("tmux_capture_pane", { agentId: selectedAgent });
      setCapturedOutput(content);
      addOutput(`[TMUX] Captured ${content.length} chars from ${selectedAgent}`);
    } catch (e) {
      addOutput(`[TMUX] Error capturing: ${e}`);
    }
  };

  // メッセージ送信
  const handleSend = async () => {
    if (!selectedAgent || !message.trim()) return;
    try {
      await invoke("tmux_send_message", { agentId: selectedAgent, message });
      addOutput(`[TMUX] Sent to ${selectedAgent}: ${message.substring(0, 50)}...`);
      setMessage("");
    } catch (e) {
      addOutput(`[TMUX] Error sending: ${e}`);
    }
  };

  // 状態取得
  const handleGetStatus = async () => {
    if (!selectedAgent) return;
    try {
      const status = await invoke<string>("tmux_get_status", { agentId: selectedAgent });
      addOutput(`[TMUX] ${selectedAgent} status: ${status}`);
    } catch (e) {
      addOutput(`[TMUX] Error getting status: ${e}`);
    }
  };

  // ポーリング開始
  const handleStartPolling = async () => {
    try {
      await invoke("tmux_start_polling", { intervalMs: 500 });
      setPolling(true);
      addOutput("[TMUX] Polling started");
    } catch (e) {
      addOutput(`[TMUX] Error starting polling: ${e}`);
    }
  };

  // ポーリング停止
  const handleStopPolling = async () => {
    try {
      await invoke("tmux_stop_polling");
      setPolling(false);
      addOutput("[TMUX] Polling stopped");
    } catch (e) {
      addOutput(`[TMUX] Error stopping polling: ${e}`);
    }
  };

  // セッション終了
  const handleDestroySession = async () => {
    try {
      // ポーリングを先に停止
      if (polling) {
        await invoke("tmux_stop_polling");
        setPolling(false);
      }
      await invoke("tmux_destroy_session");
      addOutput("[TMUX] Session destroyed");
      setSessionCreated(false);
      setAgents([]);
      setSelectedAgent("");
      setCapturedOutput("");
    } catch (e) {
      addOutput(`[TMUX] Error destroying session: ${e}`);
    }
  };

  // 質問に回答（Level 3: 質問処理）
  const handleAnswerQuestion = async () => {
    if (!questionDialog.userInput.trim()) {
      addOutput("[QUESTION] 入力が空です");
      return;
    }

    try {
      await invoke("tmux_answer_question", {
        agentId: questionDialog.agent_id,
        answer: questionDialog.userInput,
      });
      addOutput(`[QUESTION] 回答を送信: ${questionDialog.userInput}`);
      setQuestionDialog({
        visible: false,
        agent_id: "",
        question: "",
        question_id: "",
        context: "",
        userInput: "",
      });
    } catch (e) {
      addOutput(`[QUESTION] エラー: ${e}`);
    }
  };

  // 質問ダイアログを閉じる
  const handleCloseQuestionDialog = () => {
    setQuestionDialog({
      visible: false,
      agent_id: "",
      question: "",
      question_id: "",
      context: "",
      userInput: "",
    });
  };

  return (
    <section className="section-card tmux-test">
      <h2>tmux テスト (ACP v2 PoC)</h2>

      <div className="button-group">
        <button onClick={handleCreateSession} disabled={sessionCreated}>
          {sessionCreated ? "セッション作成済み" : "tmuxセッション作成"}
        </button>
        <button onClick={handleSpawnClaudeCode} disabled={!sessionCreated}>
          Claude Code起動
        </button>
        <button onClick={refreshAgents} disabled={!sessionCreated}>
          エージェント一覧更新
        </button>
        <button onClick={handleDestroySession} disabled={!sessionCreated} className="btn-danger">
          セッション終了
        </button>
      </div>

      {sessionCreated && (
        <>
          {/* エージェント選択 */}
          <div className="input-section">
            <label>エージェント選択</label>
            <select
              value={selectedAgent}
              onChange={(e) => setSelectedAgent(e.target.value)}
            >
              {agents.map((a) => (
                <option key={a.agent_id} value={a.agent_id}>
                  {a.agent_id} ({a.agent_type}) - {a.status}
                </option>
              ))}
            </select>
          </div>

          {/* 操作ボタン */}
          <div className="button-group">
            <button onClick={handleCapture} disabled={!selectedAgent}>
              画面キャプチャ
            </button>
            <button onClick={handleGetStatus} disabled={!selectedAgent}>
              状態取得
            </button>
            {polling ? (
              <button onClick={handleStopPolling} className="btn-success">
                自動ポーリング停止
              </button>
            ) : (
              <button onClick={handleStartPolling} disabled={!selectedAgent}>
                自動ポーリング開始
              </button>
            )}
          </div>

          {/* メッセージ送信 */}
          <div className="input-section">
            <label>メッセージ送信</label>
            <div className="input-row">
              <input
                type="text"
                placeholder="メッセージを入力..."
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleSend()}
              />
              <button onClick={handleSend} disabled={!selectedAgent || !message}>
                送信
              </button>
            </div>
          </div>

          {/* キャプチャ出力 */}
          <div className="output-section">
            <label>tmux出力 ({selectedAgent}) {polling && <span className="polling-indicator">● 自動更新中</span>}</label>
            <pre className="tmux-output">{capturedOutput || "(出力なし)"}</pre>
          </div>

          {/* tmux attach ヒント */}
          <div className="hint">
            <code>tmux attach -t revoice</code> でターミナルから確認できます
          </div>
        </>
      )}

      {/* 質問ダイアログ（Level 3: 質問処理） */}
      {questionDialog.visible && (
        <div className="dialog-overlay">
          <div className="dialog question-dialog">
            <h3>エージェントからの質問</h3>
            <div className="dialog-agent">
              <strong>エージェント:</strong> {questionDialog.agent_id}
            </div>
            <div className="dialog-question">
              {/* 問題文と選択肢を分割して表示 */}
              {(() => {
                const parts = questionDialog.question.split("\n---\n");
                const questionText = parts.length > 1 ? parts[0] : "";
                const optionsText = parts.length > 1 ? parts[1] : questionDialog.question;

                return (
                  <>
                    {/* 問題文（あれば表示） */}
                    {questionText && (
                      <div className="question-text-wrapper">
                        <p className="question-text">{questionText}</p>
                      </div>
                    )}
                    {/* 選択肢をボタンで表示 */}
                    {optionsText.includes("\n") ? (
                      <div className="options-section">
                        <strong>選択肢:</strong>
                        <div className="option-buttons">
                          {optionsText.split("\n").map((option, index) => (
                            <button
                              key={index}
                              className={`option-btn ${questionDialog.userInput === option.replace(/^\d+\.\s*/, "") ? "selected" : ""}`}
                              onClick={() => {
                                // 番号を除去して回答を設定
                                const answer = option.replace(/^\d+\.\s*/, "");
                                setQuestionDialog({ ...questionDialog, userInput: answer });
                              }}
                            >
                              {option}
                            </button>
                          ))}
                        </div>
                      </div>
                    ) : (
                      <p className="question-text">{optionsText}</p>
                    )}
                  </>
                );
              })()}
            </div>
            <div className="dialog-input">
              <input
                type="text"
                placeholder="回答を入力するか、上の選択肢をクリック..."
                value={questionDialog.userInput}
                onChange={(e) => setQuestionDialog({ ...questionDialog, userInput: e.target.value })}
                onKeyDown={(e) => e.key === "Enter" && handleAnswerQuestion()}
                autoFocus
              />
            </div>
            <div className="dialog-buttons">
              <button
                onClick={handleAnswerQuestion}
                disabled={!questionDialog.userInput.trim()}
                className="btn-primary"
              >
                回答を送信
              </button>
              <button onClick={handleCloseQuestionDialog} className="btn-secondary">
                スキップ
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

// Create ACP client instance
const acp = new ACPClient();

// PTY出力をフィルタリング（翻訳結果のみを抽出）
function filterPtyOutput(output: string): string {
  // 除去するパターン
  const filterPatterns = [
    /ctrl\+g to edit in VS Code/gi,
    /esc to interrupt/gi,
    /\? for shortcuts/gi,
    /─────────────+/g,
    /[✢✳✶✻✷✸✹✺·]+/g, // スピナーアニメーション記号
    /❯\s*/g, // プロンプト
  ];

  // スピナーテキストパターン（動詞+ing…）
  const spinnerTextPatterns = [
    /\w+ing…/gi,
    /\w+ing\.\.\./gi,
  ];

  let filtered = output;

  // 各パターンを適用
  for (const pattern of filterPatterns) {
    filtered = filtered.replace(pattern, '');
  }

  // スピナーテキストを除去
  for (const pattern of spinnerTextPatterns) {
    filtered = filtered.replace(pattern, '');
  }

  // 複数のスペースを1つにまとめる
  filtered = filtered.replace(/ {2,}/g, ' ');

  // 複数の改行を1つにまとめる
  filtered = filtered.replace(/\n{3,}/g, '\n\n');

  // 先頭・末尾の空白を削除
  filtered = filtered.trim();

  return filtered;
}

// 入力要求ダイアログの状態
interface InputRequiredDialog {
  visible: boolean;
  promptType: string;
  context: string;
  userInput: string;
}

function App() {
  const [youtubeUrl, setYoutubeUrl] = useState("");
  const [output, setOutput] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [claudeRunning, setClaudeRunning] = useState(false);
  const [subtitleLang, setSubtitleLang] = useState("en");

  // ACP state
  const [agents, setAgents] = useState<AgentCard[]>([]);
  const [discoveredAgents, setDiscoveredAgents] = useState<AgentCard[]>([]);
  const [stats, setStats] = useState<OrchestratorStats | null>(null);
  const [translationText, setTranslationText] = useState("");
  const [translatedText, setTranslatedText] = useState("");

  // 入力要求ダイアログの状態
  const [inputDialog, setInputDialog] = useState<InputRequiredDialog>({
    visible: false,
    promptType: "",
    context: "",
    userInput: "",
  });

  // Claude Code起動状態を定期的にチェック
  useEffect(() => {
    const checkInterval = setInterval(async () => {
      try {
        const running = await invoke<boolean>("is_claude_running");
        setClaudeRunning(running);
      } catch (e) {
        console.error("Failed to check Claude status:", e);
      }
    }, 5000);

    return () => clearInterval(checkInterval);
  }, []);

  // ACP statsを定期的に更新
  useEffect(() => {
    const statsInterval = setInterval(async () => {
      try {
        const s = await acp.getStats();
        setStats(s);

        const agentList = await acp.listAgents();
        setAgents(agentList);
      } catch (e) {
        console.error("Failed to get ACP stats:", e);
      }
    }, 3000);

    return () => clearInterval(statsInterval);
  }, []);

  // PTYイベントリスナー（イベント駆動）
  useEffect(() => {
    let mounted = true;
    const unlisteners: UnlistenFn[] = [];

    // リスナー登録完了をログに表示
    if (mounted) addOutput("PTY event listeners registering...");

    // PTY出力イベント
    listen<string>("pty-output", (event) => {
      if (!mounted) return;
      const output = event.payload;
      addOutput(`[EVENT] pty-output: ${output.length} chars`);
      // フィルタリングして翻訳結果エリアに追加
      const filtered = filterPtyOutput(output);
      if (filtered) {
        setTranslatedText((prev) => prev + filtered + '\n');
      }
    }).then((unlisten) => {
      if (mounted) {
        addOutput("pty-output listener registered");
        unlisteners.push(unlisten);
      } else {
        unlisten(); // 既にアンマウントされている場合は即座に解除
      }
    });

    // プロンプト検知イベント（入力待ち状態）
    listen("pty-prompt", () => {
      if (!mounted) return;
      addOutput("[EVENT] pty-prompt: Response completed");
      setIsLoading(false);
    }).then((unlisten) => {
      if (mounted) {
        unlisteners.push(unlisten);
      } else {
        unlisten();
      }
    });

    // エラーイベント
    listen<string>("pty-error", (event) => {
      if (!mounted) return;
      addOutput(`[EVENT] pty-error: ${event.payload}`);
    }).then((unlisten) => {
      if (mounted) {
        unlisteners.push(unlisten);
      } else {
        unlisten();
      }
    });

    // 入力要求イベント（認証必要やユーザー入力が必要）
    listen<{ promptType: unknown; context: string }>("pty-input-required", (event) => {
      if (!mounted) return;
      const { promptType, context } = event.payload;
      addOutput(`[EVENT] pty-input-required: ${JSON.stringify(promptType)}`);

      // ダイアログを表示
      setInputDialog({
        visible: true,
        promptType: JSON.stringify(promptType),
        context: context.slice(-500), // 直近500文字
        userInput: "",
      });
      setIsLoading(false);
    }).then((unlisten) => {
      if (mounted) {
        unlisteners.push(unlisten);
      } else {
        unlisten();
      }
    });

    return () => {
      mounted = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 出力を追加
  const addOutput = useCallback((text: string) => {
    setOutput((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${text}`]);
  }, []);

  // Claude Code起動
  const handleSpawnClaude = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<string>("spawn_claude");
      addOutput(result);
      setClaudeRunning(true);
    } catch (e) {
      addOutput(`Error: ${e}`);
    }
    setIsLoading(false);
  };

  // ACP: エージェント登録
  const handleRegisterAgent = async () => {
    setIsLoading(true);
    try {
      const agentId = await acp.registerAgent("claude-code", "main");
      addOutput(`Agent registered: ${agentId}`);
    } catch (e) {
      addOutput(`Error registering agent: ${e}`);
    }
    setIsLoading(false);
  };

  // ACP: 翻訳エージェントを発見
  const handleDiscoverTranslators = async () => {
    setIsLoading(true);
    try {
      const translators = await acp.findTranslators();
      setDiscoveredAgents(translators);
      if (translators.length === 0) {
        addOutput("No translation agents found");
      } else {
        addOutput(`Found ${translators.length} translator(s)`);
      }
    } catch (e) {
      addOutput(`Error discovering agents: ${e}`);
    }
    setIsLoading(false);
  };

  // ACP: 翻訳実行（イベント駆動）
  const handleTranslate = async () => {
    if (!translationText.trim()) {
      addOutput("Please enter text to translate");
      return;
    }

    setIsLoading(true);
    setTranslatedText(""); // 結果エリアをクリア

    try {
      addOutput(`Translating: "${translationText.substring(0, 50)}..."`);

      // 翻訳プロンプトを送信（応答はイベントで受信）
      const prompt = `以下の英語を日本語に翻訳してください。翻訳結果だけを出力してください:\n\n${translationText}`;
      const result = await acp.send("claude-code-main", prompt);
      addOutput(result); // "Message sent..." メッセージ
      // 実際の翻訳結果は pty-output イベントで translatedText に追加される
    } catch (e) {
      addOutput(`Translation error: ${e}`);
      setIsLoading(false);
    }
  };

  // テストコマンド実行
  const handleTestCommand = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<string>("execute_command", {
        command: "echo 'Hello from Re-Voice!'",
      });
      addOutput(`Result: ${result}`);
    } catch (e) {
      addOutput(`Error: ${e}`);
    }
    setIsLoading(false);
  };

  // PTYテスト: シンプルなメッセージ送信
  const handlePtyTest = async () => {
    setIsLoading(true);
    try {
      addOutput("[TEST] Sending 'hello' to PTY...");
      const result = await invoke<string>("send_to_claude", {
        message: "hello",
      });
      addOutput(`[TEST] Result: ${result}`);
    } catch (e) {
      addOutput(`[TEST] Error: ${e}`);
    }
    setIsLoading(false);
  };

  // PTY: セキュリティ確認を承認
  const handleConfirmSecurity = async () => {
    setIsLoading(true);
    try {
      addOutput("[SECURITY] Confirming workspace trust...");
      // "1" を送信して "Yes, I trust this folder" を選択
      await invoke<string>("send_to_claude", { message: "1" });
      // Enter を送信して確定
      await new Promise(resolve => setTimeout(resolve, 100));
      await invoke<string>("send_to_claude", { message: "" });
      addOutput("[SECURITY] Confirmation sent");
    } catch (e) {
      addOutput(`[SECURITY] Error: ${e}`);
    }
    setIsLoading(false);
  };

  // PTYテスト: 改行のみ送信（プロンプト確認用）
  const handlePtyNewline = async () => {
    setIsLoading(true);
    try {
      addOutput("[TEST] Sending newline to PTY...");
      const result = await invoke<string>("send_to_claude", {
        message: "",
      });
      addOutput(`[TEST] Result: ${result}`);
    } catch (e) {
      addOutput(`[TEST] Error: ${e}`);
    }
    setIsLoading(false);
  };

  // PTY出力を取得
  const handleGetOutput = async () => {
    try {
      const result = await invoke<string>("read_from_claude");
      addOutput(`[OUTPUT] ${result.length} chars: ${result.substring(0, 200)}...`);
    } catch (e) {
      addOutput(`[OUTPUT] Error: ${e}`);
    }
  };

  // PTYテスト: 送信後即座に読み取り
  const handleRoundtrip = async () => {
    setIsLoading(true);
    try {
      addOutput("[ROUNDTRIP] Sending and waiting 1s...");
      const result = await invoke<string>("pty_test_roundtrip", {
        message: "hello",
      });
      addOutput(`[ROUNDTRIP] Result: ${result.length} chars`);
      addOutput(`[ROUNDTRIP] Content: ${result.substring(result.length - 500)}`);
    } catch (e) {
      addOutput(`[ROUNDTRIP] Error: ${e}`);
    }
    setIsLoading(false);
  };

  // 子プロセス状態確認
  const handleCheckChild = async () => {
    try {
      const pid = await invoke<number | null>("get_child_pid");
      const alive = await invoke<boolean>("is_child_alive");
      addOutput(`[CHILD] PID: ${pid}, Alive: ${alive}`);
    } catch (e) {
      addOutput(`[CHILD] Error: ${e}`);
    }
  };

  // 入力要求ダイアログ: 入力を送信
  const handleSendUserInput = async () => {
    if (!inputDialog.userInput.trim()) {
      addOutput("[DIALOG] 入力が空です");
      return;
    }

    setIsLoading(true);
    try {
      addOutput(`[DIALOG] Sending: ${inputDialog.userInput}`);
      await invoke<string>("send_to_claude", { message: inputDialog.userInput });
      setInputDialog({ visible: false, promptType: "", context: "", userInput: "" });
      addOutput("[DIALOG] Input sent successfully");
    } catch (e) {
      addOutput(`[DIALOG] Error: ${e}`);
    }
    setIsLoading(false);
  };

  // 入力要求ダイアログ: /login を送信（認証用）
  const handleSendLogin = async () => {
    setIsLoading(true);
    try {
      addOutput("[DIALOG] Sending /login...");
      await invoke<string>("send_to_claude", { message: "/login" });
      setInputDialog({ visible: false, promptType: "", context: "", userInput: "" });
      addOutput("[DIALOG] /login sent");
    } catch (e) {
      addOutput(`[DIALOG] Error: ${e}`);
    }
    setIsLoading(false);
  };

  // 入力要求ダイアログ: 閉じる
  const handleCloseDialog = () => {
    setInputDialog({ visible: false, promptType: "", context: "", userInput: "" });
  };

  // yt-dlpで動画情報取得
  const handleFetchVideoInfo = async () => {
    if (!youtubeUrl) {
      addOutput("URLを入力してください");
      return;
    }

    setIsLoading(true);
    addOutput(`動画情報を取得中: ${youtubeUrl}`);

    try {
      const result = await invoke<string>("execute_command", {
        command: `yt-dlp --print title "${youtubeUrl}"`,
      });
      addOutput(`タイトル: ${result.trim()}`);
    } catch (e) {
      addOutput(`Error: ${e}`);
    }
    setIsLoading(false);
  };

  // 利用可能な字幕一覧を取得
  const handleListSubtitles = async () => {
    if (!youtubeUrl) {
      addOutput("URLを入力してください");
      return;
    }

    setIsLoading(true);
    addOutput("字幕一覧を取得中...");

    try {
      const result = await invoke<string>("get_available_subtitles", {
        url: youtubeUrl,
      });
      addOutput(`字幕一覧:\n${result}`);
    } catch (e) {
      addOutput(`Error: ${e}`);
    }
    setIsLoading(false);
  };

  // 字幕をダウンロード
  const handleDownloadSubtitle = async (useAuto: boolean = false) => {
    if (!youtubeUrl) {
      addOutput("URLを入力してください");
      return;
    }

    setIsLoading(true);
    const subtitleType = useAuto ? "自動生成字幕" : "字幕";
    addOutput(`${subtitleType}をダウンロード中... (言語: ${subtitleLang})`);

    try {
      const outputPath = "/tmp/revoice_subtitle";
      let result: string;

      if (useAuto) {
        result = await invoke<string>("download_auto_subtitles", {
          url: youtubeUrl,
          lang: subtitleLang,
          outputPath,
        });
      } else {
        result = await invoke<string>("download_subtitles", {
          url: youtubeUrl,
          lang: subtitleLang,
          outputPath,
        });
      }

      addOutput(result);
    } catch (e) {
      addOutput(`Error: ${e}`);
    }
    setIsLoading(false);
  };

  return (
    <main className="container">
      <h1>Re-Voice</h1>
      <p className="subtitle">YouTube外国語動画 → 日本語吹替版</p>

      {/* ステータス表示 */}
      <div className="status-bar">
        <span className={`status-indicator ${claudeRunning ? "running" : "stopped"}`}>
          Claude Code: {claudeRunning ? "起動中" : "停止中"}
        </span>
        {stats && (
          <span className="status-indicator">
            ACP: {stats.totalAgents} agents, {stats.tasksCompleted} completed
          </span>
        )}
      </div>

      {/* ==================== Claude Code セクション ==================== */}
      <section className="section-card">
        <h2>Claude Code</h2>

        {/* 起動・登録ボタン */}
        <div className="button-group">
          <button
            onClick={handleSpawnClaude}
            disabled={isLoading || claudeRunning}
            className={claudeRunning ? "btn-success" : ""}
          >
            {claudeRunning ? "起動済み" : "Claude Code起動"}
          </button>
          <button onClick={handleRegisterAgent} disabled={isLoading || !claudeRunning}>
            エージェント登録
          </button>
          <button onClick={handleDiscoverTranslators} disabled={isLoading || !claudeRunning}>
            翻訳エージェント検索
          </button>
        </div>

        {/* 登録済みエージェント */}
        {agents.length > 0 && (
          <div className="agent-list">
            <h3>登録済みエージェント</h3>
            <ul>
              {agents.map((agent) => (
                <li key={agent.id}>
                  <strong>{agent.name}</strong>
                  <small>{agent.capabilities.map((c) => c.name).join(", ")}</small>
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* 検索結果 */}
        {discoveredAgents.length > 0 && (
          <div className="agent-list discovered">
            <h3>検索結果</h3>
            <ul>
              {discoveredAgents.map((agent) => (
                <li key={agent.id}>
                  <strong>{agent.name}</strong>
                  <small>{agent.capabilities.map((c) => c.name).join(", ")}</small>
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* 翻訳テスト */}
        <div className="translation-section">
          <h3>翻訳テスト</h3>
          <textarea
            placeholder="翻訳したいテキストを入力..."
            value={translationText}
            onChange={(e) => setTranslationText(e.target.value)}
            rows={3}
          />
          <button onClick={handleTranslate} disabled={isLoading || !translationText || !claudeRunning}>
            日本語に翻訳
          </button>
          {translatedText && (
            <div className="translation-result">
              <h4>翻訳結果:</h4>
              <p>{translatedText}</p>
            </div>
          )}
        </div>
      </section>

      {/* ==================== YouTube セクション ==================== */}
      <section className="section-card">
        <h2>YouTube</h2>

        {/* URL入力 */}
        <div className="input-section">
          <label htmlFor="youtube-url">動画URL</label>
          <input
            id="youtube-url"
            type="text"
            placeholder="https://www.youtube.com/watch?v=..."
            value={youtubeUrl}
            onChange={(e) => setYoutubeUrl(e.target.value)}
          />
        </div>

        {/* 字幕言語選択 */}
        <div className="input-section">
          <label htmlFor="subtitle-lang">字幕言語</label>
          <select
            id="subtitle-lang"
            value={subtitleLang}
            onChange={(e) => setSubtitleLang(e.target.value)}
          >
            <option value="en">英語 (en)</option>
            <option value="ko">韓国語 (ko)</option>
            <option value="zh-CN">中国語 簡体字 (zh-CN)</option>
            <option value="zh-TW">中国語 繁体字 (zh-TW)</option>
            <option value="es">スペイン語 (es)</option>
            <option value="fr">フランス語 (fr)</option>
            <option value="de">ドイツ語 (de)</option>
            <option value="pt">ポルトガル語 (pt)</option>
            <option value="ru">ロシア語 (ru)</option>
            <option value="ar">アラビア語 (ar)</option>
          </select>
        </div>

        {/* アクションボタン */}
        <div className="button-group">
          <button onClick={handleFetchVideoInfo} disabled={isLoading || !youtubeUrl}>
            動画情報取得
          </button>
          <button onClick={handleListSubtitles} disabled={isLoading || !youtubeUrl}>
            字幕一覧
          </button>
          <button onClick={() => handleDownloadSubtitle(false)} disabled={isLoading || !youtubeUrl}>
            字幕DL
          </button>
          <button onClick={() => handleDownloadSubtitle(true)} disabled={isLoading || !youtubeUrl}>
            自動字幕DL
          </button>
        </div>
      </section>

      {/* ==================== ログ ==================== */}
      <div className="output-section">
        <label>ログ</label>
        <div className="output-log">
          {output.length === 0 ? (
            <p className="placeholder">出力がありません</p>
          ) : (
            output.map((line, i) => <div key={i}>{line}</div>)
          )}
        </div>
      </div>

      {/* デバッグ用 */}
      <div className="button-group debug">
        <button onClick={handleTestCommand} disabled={isLoading}>
          テストコマンド
        </button>
        <button onClick={handleConfirmSecurity} disabled={isLoading || !claudeRunning}>
          セキュリティ確認
        </button>
        <button onClick={handlePtyTest} disabled={isLoading || !claudeRunning}>
          PTY: hello送信
        </button>
        <button onClick={handlePtyNewline} disabled={isLoading || !claudeRunning}>
          PTY: 改行のみ
        </button>
        <button onClick={handleGetOutput} disabled={!claudeRunning}>
          PTY: 出力取得
        </button>
        <button onClick={handleRoundtrip} disabled={isLoading || !claudeRunning}>
          PTY: Roundtrip
        </button>
        <button onClick={handleCheckChild} disabled={!claudeRunning}>
          子プロセス確認
        </button>
      </div>

      {/* ==================== tmuxテスト (ACP v2 PoC) ==================== */}
      <TmuxTestSection addOutput={addOutput} />

      {/* ==================== 字幕翻訳パイプライン (Phase 3) ==================== */}
      <PipelineRunner addOutput={addOutput} />

      {/* ==================== 入力要求ダイアログ ==================== */}
      {inputDialog.visible && (
        <div className="dialog-overlay">
          <div className="dialog">
            <h3>入力が必要です</h3>
            <div className="dialog-type">
              <strong>検出タイプ:</strong> {inputDialog.promptType}
            </div>
            <div className="dialog-context">
              <strong>コンテキスト:</strong>
              <pre>{inputDialog.context}</pre>
            </div>
            <div className="dialog-input">
              <input
                type="text"
                placeholder="入力してください..."
                value={inputDialog.userInput}
                onChange={(e) => setInputDialog({ ...inputDialog, userInput: e.target.value })}
                onKeyDown={(e) => e.key === "Enter" && handleSendUserInput()}
              />
            </div>
            <div className="dialog-buttons">
              <button onClick={handleSendUserInput} disabled={isLoading || !inputDialog.userInput}>
                送信
              </button>
              <button onClick={handleSendLogin} disabled={isLoading}>
                /login
              </button>
              <button onClick={handleCloseDialog} className="btn-secondary">
                閉じる
              </button>
            </div>
          </div>
        </div>
      )}
    </main>
  );
}

export default App;
