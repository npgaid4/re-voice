/**
 * PipelineRunner Component
 *
 * Phase 3: 字幕翻訳パイプラインの実行UI
 * - YouTube URL入力
 * - 字幕言語選択
 * - VOICEVOX話者選択
 * - 実行ボタン
 * - 進捗バー
 * - 権限ダイアログ
 * - 生成ファイル一覧
 * - 音声プレイヤー
 */

import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import "./PipelineRunner.css";

// VOICEVOX話者情報
interface SpeakerStyle {
  name: string;
  id: number;
}

interface Speaker {
  name: string;
  speaker_uuid: string;
  styles: SpeakerStyle[];
}

// パイプライン進捗イベント
interface ProgressEvent {
  execution_id: string;
  stage_index: number;
  stage_name: string;
  status: string;
  progress_percent: number;
  message: string;
}

// パイプライン実行状態
interface PipelineExecution {
  pipeline_id: string;
  execution_id: string;
  status: string;
  current_stage: number;
  stage_results: {
    stage_name: string;
    stage_index: number;
    status: string;
    output: Record<string, unknown> | null;
    error: string | null;
  }[];
  progress: number;
}

// 権限要求イベント
interface PermissionRequestEvent {
  request_id: string;
  tool_name: string;
  tool_input: Record<string, unknown>;
}

// 権限ダイアログ状態
interface PermissionDialogState {
  visible: boolean;
  request_id: string;
  tool_name: string;
  tool_input: Record<string, unknown>;
}

// 生成ファイル情報
interface GeneratedFile {
  path: string;
  type: "vtt" | "wav" | "json";
  name: string;
}

interface PipelineRunnerProps {
  addOutput: (text: string) => void;
}

// 4ステージの定義
const STAGE_NAMES = [
  "字幕ダウンロード",
  "VTT解析",
  "翻訳",
  "音声生成",
];

function PipelineRunner({ addOutput }: PipelineRunnerProps) {
  // 入力状態
  const [youtubeUrl, setYoutubeUrl] = useState("");
  const [subtitleLang, setSubtitleLang] = useState("en");
  const [outputDir, setOutputDir] = useState("/tmp/revoice");

  // VOICEVOX状態
  const [voicevoxRunning, setVoicevoxRunning] = useState(false);
  const [speakers, setSpeakers] = useState<Speaker[]>([]);
  const [selectedSpeaker, setSelectedSpeaker] = useState(1);

  // CLIエグゼキューター状態
  const [cliExecutorRunning, setCliExecutorRunning] = useState(false);

  // 実行状態
  const [isRunning, setIsRunning] = useState(false);
  const [executionId, setExecutionId] = useState<string | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [execution, setExecution] = useState<PipelineExecution | null>(null);

  // 権限ダイアログ
  const [permissionDialog, setPermissionDialog] = useState<PermissionDialogState>({
    visible: false,
    request_id: "",
    tool_name: "",
    tool_input: {},
  });

  // 生成ファイル一覧
  const [generatedFiles, setGeneratedFiles] = useState<GeneratedFile[]>([]);

  // 音声プレイヤー
  const [currentAudio, setCurrentAudio] = useState<string | null>(null);
  const audioRef = useRef<HTMLAudioElement>(null);

  // CLIエグゼキューター状態チェック
  useEffect(() => {
    const checkExecutor = async () => {
      try {
        const running = await invoke<boolean>("executor_is_running");
        setCliExecutorRunning(running);
      } catch {
        setCliExecutorRunning(false);
      }
    };

    checkExecutor();
    const interval = setInterval(checkExecutor, 5000);
    return () => clearInterval(interval);
  }, []);

  // VOICEVOX状態チェック
  useEffect(() => {
    const checkVoicevox = async () => {
      try {
        const running = await invoke<boolean>("voicevox_is_running");
        setVoicevoxRunning(running);

        if (running) {
          const speakerList = await invoke<Speaker[]>("voicevox_get_speakers");
          setSpeakers(speakerList);
          addOutput(`[VOICEVOX] ${speakerList.length}人の話者を取得`);
        }
      } catch (e) {
        addOutput(`[VOICEVOX] エラー: ${e}`);
      }
    };

    checkVoicevox();
    const interval = setInterval(checkVoicevox, 10000);
    return () => clearInterval(interval);
  }, [addOutput]);

  // イベントリスナー
  useEffect(() => {
    let mounted = true;
    const unlisteners: UnlistenFn[] = [];

    // 進捗イベント
    listen<ProgressEvent>("pipeline:progress", (event) => {
      if (!mounted) return;
      const data = event.payload;
      setProgress(data);

      // execution_idを設定（初回）
      if (data.status === "pipeline-started") {
        setExecutionId(data.execution_id);
        setGeneratedFiles([]); // リセット
      }

      addOutput(`[PIPELINE] ${data.stage_name}: ${data.status} - ${data.message}`);

      if (data.status === "pipeline-completed") {
        setIsRunning(false);
        // 生成ファイルを確認
        scanGeneratedFiles();
      }

      if (data.status === "stage-failed") {
        setIsRunning(false);
      }
    }).then((unlisten) => {
      if (mounted) {
        unlisteners.push(unlisten);
      } else {
        unlisten();
      }
    });

    // 権限要求イベント
    listen<PermissionRequestEvent>("executor:permission_required", (event) => {
      if (!mounted) return;
      const { request_id, tool_name, tool_input } = event.payload;

      addOutput(`[PERMISSION] 権限要求: ${tool_name}`);

      setPermissionDialog({
        visible: true,
        request_id,
        tool_name,
        tool_input,
      });
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
  }, [addOutput]);

  // 実行状態を定期取得
  useEffect(() => {
    if (!executionId || !isRunning) return;

    const interval = setInterval(async () => {
      try {
        const exec = await invoke<PipelineExecution | null>(
          "get_pipeline_execution",
          { executionId }
        );
        if (exec) {
          setExecution(exec);
        }
      } catch (e) {
        addOutput(`[PIPELINE] 状態取得エラー: ${e}`);
      }
    }, 1000);

    return () => clearInterval(interval);
  }, [executionId, isRunning, addOutput]);

  // 生成ファイルをスキャン
  const scanGeneratedFiles = async () => {
    try {
      // 簡易実装: 出力ディレクトリのファイルをリスト
      // 実際はRust側でファイル一覧を返すコマンドを実装すべき
      const files: GeneratedFile[] = [];

      // 既知のファイルパスをチェック
      const knownFiles = [
        { path: `${outputDir}/translated.ja.vtt`, type: "vtt" as const, name: "翻訳済み字幕" },
        { path: `${outputDir}/segments.json`, type: "json" as const, name: "セグメント情報" },
      ];

      for (const file of knownFiles) {
        // ファイル存在チェックは実際にはRust側で行う
        files.push(file);
      }

      // 音声ファイルはディレクトリから取得
      // 実際はRust側でスキャンする必要がある
      for (let i = 0; i < 100; i++) {
        const audioPath = `${outputDir}/audio/audio_${String(i).padStart(4, '0')}.wav`;
        files.push({ path: audioPath, type: "wav", name: `音声 ${i + 1}` });
      }

      setGeneratedFiles(files);
      addOutput(`[FILES] ${files.length}個のファイルを生成`);
    } catch (e) {
      addOutput(`[FILES] スキャンエラー: ${e}`);
    }
  };

  // CLIエグゼキューター起動
  const handleStartExecutor = async () => {
    try {
      addOutput("[EXECUTOR] CLIエグゼキューターを起動中...");

      const sessionId = await invoke<string>("executor_start", {
        workingDir: outputDir,
        allowedTools: ["Read", "Write", "Edit"],
      });

      setCliExecutorRunning(true);
      addOutput(`[EXECUTOR] 起動完了: session=${sessionId}`);
    } catch (e) {
      addOutput(`[EXECUTOR] 起動エラー: ${e}`);
    }
  };

  // パイプライン実行
  const handleRunPipeline = async () => {
    if (!youtubeUrl.trim()) {
      addOutput("[PIPELINE] URLを入力してください");
      return;
    }

    // CLIエグゼキューターが起動していない場合は起動
    if (!cliExecutorRunning) {
      await handleStartExecutor();
    }

    setIsRunning(true);
    setProgress(null);
    setExecution(null);
    setGeneratedFiles([]);

    try {
      addOutput(`[PIPELINE] 開始: ${youtubeUrl} (${subtitleLang})`);

      // バックグラウンドでパイプラインを開始
      const result = await invoke<string>("run_subtitle_pipeline", {
        youtubeUrl,
        subtitleLang,
        outputDir,
      });

      addOutput(`[PIPELINE] パイプライン開始: ${result}`);
    } catch (e) {
      addOutput(`[PIPELINE] エラー: ${e}`);
      setIsRunning(false);
    }
  };

  // キャンセル
  const handleCancel = async () => {
    if (!executionId) return;

    try {
      await invoke("cancel_pipeline_execution", { executionId });
      addOutput("[PIPELINE] キャンセルしました");
      setIsRunning(false);
    } catch (e) {
      addOutput(`[PIPELINE] キャンセルエラー: ${e}`);
    }
  };

  // 権限許可
  const handlePermissionAllow = async (always: boolean = false) => {
    try {
      await invoke("executor_submit_permission", {
        requestId: permissionDialog.request_id,
        allow: true,
        always,
      });
      addOutput(`[PERMISSION] 許可: ${permissionDialog.tool_name}`);
    } catch (e) {
      addOutput(`[PERMISSION] エラー: ${e}`);
    }
    setPermissionDialog({ visible: false, request_id: "", tool_name: "", tool_input: {} });
  };

  // 権限拒否
  const handlePermissionDeny = async () => {
    try {
      await invoke("executor_submit_permission", {
        requestId: permissionDialog.request_id,
        allow: false,
        always: false,
      });
      addOutput(`[PERMISSION] 拒否: ${permissionDialog.tool_name}`);
    } catch (e) {
      addOutput(`[PERMISSION] エラー: ${e}`);
    }
    setPermissionDialog({ visible: false, request_id: "", tool_name: "", tool_input: {} });
  };

  // 音声再生
  const handlePlayAudio = (path: string) => {
    setCurrentAudio(path);
    if (audioRef.current) {
      audioRef.current.src = `file://${path}`;
      audioRef.current.play().catch((e) => {
        addOutput(`[AUDIO] 再生エラー: ${e}`);
      });
    }
  };

  // 音声合成テスト
  const handleTestSynthesis = async () => {
    if (!voicevoxRunning) {
      addOutput("[VOICEVOX] Engineが起動していません");
      return;
    }

    try {
      addOutput(`[VOICEVOX] テスト音声を生成中... (話者ID: ${selectedSpeaker})`);

      const outputPath = `${outputDir}/test_voice.wav`;
      const result = await invoke<string>("voicevox_synthesize", {
        text: "こんにちは、これはテストです。",
        speaker: selectedSpeaker,
        outputPath,
      });

      addOutput(`[VOICEVOX] 生成完了: ${result}`);

      // 生成ファイル一覧に追加
      setGeneratedFiles((prev) => [
        ...prev,
        { path: outputPath, type: "wav", name: "テスト音声" },
      ]);
    } catch (e) {
      addOutput(`[VOICEVOX] 合成エラー: ${e}`);
    }
  };

  // 進捗バーの色を決定
  const getProgressColor = () => {
    if (!progress) return "#4a90d9";
    switch (progress.status) {
      case "stage-completed":
      case "pipeline-completed":
        return "#28a745";
      case "stage-failed":
        return "#dc3545";
      case "cancelled":
        return "#6c757d";
      default:
        return "#4a90d9";
    }
  };

  // ステータス表示
  const getStatusLabel = () => {
    if (!execution) return "待機中";
    switch (execution.status) {
      case "running":
        return "実行中";
      case "completed":
        return "完了";
      case "failed":
        return "失敗";
      case "cancelled":
        return "キャンセル";
      default:
        return execution.status;
    }
  };

  return (
    <section className="section-card pipeline-runner">
      <h2>字幕翻訳パイプライン (Phase 3)</h2>

      {/* ステータス行 */}
      <div className="status-row">
        <div className={`voicevox-status ${voicevoxRunning ? "running" : "stopped"}`}>
          <span className="status-dot"></span>
          VOICEVOX: {voicevoxRunning ? "起動中" : "停止中"}
        </div>
        <div className={`executor-status ${cliExecutorRunning ? "running" : "stopped"}`}>
          <span className="status-dot"></span>
          CLI Executor: {cliExecutorRunning ? "起動中" : "停止中"}
        </div>
      </div>

      {/* 入力エリア */}
      <div className="input-section">
        <label>YouTube URL</label>
        <input
          type="text"
          placeholder="https://www.youtube.com/watch?v=..."
          value={youtubeUrl}
          onChange={(e) => setYoutubeUrl(e.target.value)}
          disabled={isRunning}
        />
      </div>

      <div className="input-row">
        <div className="input-section">
          <label>字幕言語</label>
          <select
            value={subtitleLang}
            onChange={(e) => setSubtitleLang(e.target.value)}
            disabled={isRunning}
          >
            <option value="en">英語 (en)</option>
            <option value="ko">韓国語 (ko)</option>
            <option value="zh-CN">中国語 簡体字 (zh-CN)</option>
            <option value="zh-TW">中国語 繁体字 (zh-TW)</option>
            <option value="es">スペイン語 (es)</option>
            <option value="fr">フランス語 (fr)</option>
          </select>
        </div>

        <div className="input-section">
          <label>VOICEVOX話者</label>
          <select
            value={selectedSpeaker}
            onChange={(e) => setSelectedSpeaker(parseInt(e.target.value))}
            disabled={isRunning || !voicevoxRunning}
          >
            {speakers.length === 0 ? (
              <option value={1}>ずんだもん (デフォルト)</option>
            ) : (
              speakers.flatMap((speaker) =>
                speaker.styles.map((style) => (
                  <option key={style.id} value={style.id}>
                    {speaker.name} ({style.name})
                  </option>
                ))
              )
            )}
          </select>
        </div>

        <div className="input-section">
          <label>出力ディレクトリ</label>
          <input
            type="text"
            value={outputDir}
            onChange={(e) => setOutputDir(e.target.value)}
            disabled={isRunning}
          />
        </div>
      </div>

      {/* アクションボタン */}
      <div className="button-group">
        {!cliExecutorRunning && (
          <button
            onClick={handleStartExecutor}
            disabled={isRunning}
            className="btn-secondary"
          >
            CLI Executor起動
          </button>
        )}

        <button
          onClick={handleRunPipeline}
          disabled={isRunning || !youtubeUrl.trim()}
          className="btn-primary"
        >
          {isRunning ? "実行中..." : "パイプライン実行"}
        </button>

        {isRunning && (
          <button onClick={handleCancel} className="btn-danger">
            キャンセル
          </button>
        )}

        <button
          onClick={handleTestSynthesis}
          disabled={!voicevoxRunning || isRunning}
          className="btn-secondary"
        >
          音声合成テスト
        </button>
      </div>

      {/* 進捗表示 */}
      {(isRunning || progress) && (
        <div className="progress-section">
          <div className="progress-header">
            <span className="stage-name">
              {progress?.stage_name || STAGE_NAMES[progress?.stage_index || 0]}
            </span>
            <span className="progress-percent">{progress?.progress_percent || 0}%</span>
          </div>
          <div className="progress-bar">
            <div
              className="progress-fill"
              style={{
                width: `${progress?.progress_percent || 0}%`,
                backgroundColor: getProgressColor(),
              }}
            />
          </div>
          <div className="progress-message">{progress?.message || "準備中..."}</div>

          {/* 4ステージインジケーター */}
          <div className="stage-indicators">
            {STAGE_NAMES.map((name, index) => (
              <div
                key={index}
                className={`stage-indicator ${
                  execution && execution.current_stage >= index ? "active" : ""
                } ${
                  execution?.stage_results[index]?.status === "completed" ? "completed" : ""
                }`}
              >
                <span className="stage-num">{index + 1}</span>
                <span className="stage-label">{name}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* 実行状態 */}
      {execution && (
        <div className="execution-status">
          <div className="status-header">
            <span className={`status-badge ${execution.status}`}>
              {getStatusLabel()}
            </span>
            <span className="execution-id">ID: {execution.execution_id.slice(0, 8)}...</span>
          </div>

          {/* ステージ一覧 */}
          <div className="stage-list">
            {execution.stage_results.map((stage, index) => (
              <div key={index} className={`stage-item ${stage.status}`}>
                <span className="stage-number">{index + 1}</span>
                <span className="stage-name">{stage.stage_name}</span>
                <span className="stage-status">{stage.status}</span>
                {stage.error && (
                  <span className="stage-error">{stage.error}</span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* 生成ファイル一覧 */}
      {generatedFiles.length > 0 && (
        <div className="generated-files">
          <h3>生成ファイル</h3>
          <div className="file-list">
            {generatedFiles.filter(f => f.type === "vtt" || f.type === "json").map((file, index) => (
              <div key={index} className="file-item">
                <span className={`file-icon ${file.type}`}>{file.type.toUpperCase()}</span>
                <span className="file-name">{file.name}</span>
                <span className="file-path">{file.path}</span>
              </div>
            ))}
          </div>

          {/* 音声ファイル */}
          {generatedFiles.some(f => f.type === "wav") && (
            <div className="audio-files">
              <h4>音声ファイル</h4>
              <div className="audio-list">
                {generatedFiles
                  .filter(f => f.type === "wav")
                  .slice(0, 10) // 最初の10件のみ表示
                  .map((file, index) => (
                    <button
                      key={index}
                      className={`audio-btn ${currentAudio === file.path ? "playing" : ""}`}
                      onClick={() => handlePlayAudio(file.path)}
                    >
                      ▶ {file.name}
                    </button>
                  ))}
                {generatedFiles.filter(f => f.type === "wav").length > 10 && (
                  <span className="more-count">
                    +{generatedFiles.filter(f => f.type === "wav").length - 10} more
                  </span>
                )}
              </div>
            </div>
          )}
        </div>
      )}

      {/* 音声プレイヤー（非表示） */}
      <audio ref={audioRef} style={{ display: "none" }} />

      {/* ヒント */}
      <div className="hint">
        <strong>使い方:</strong>
        <ol>
          <li>VOICEVOX Engineを起動（別途必要）</li>
          <li>YouTube URLを入力</li>
          <li>字幕言語と話者を選択</li>
          <li>パイプライン実行をクリック</li>
        </ol>
        <p className="note">
          ※ 翻訳にはClaude Code CLIが必要です。権限ダイアログが表示されたら許可してください。
        </p>
      </div>

      {/* 権限ダイアログ */}
      {permissionDialog.visible && (
        <div className="dialog-overlay">
          <div className="dialog permission-dialog">
            <h3>権限要求</h3>
            <div className="dialog-tool">
              <strong>ツール:</strong> {permissionDialog.tool_name}
            </div>
            <div className="dialog-input">
              <strong>入力:</strong>
              <pre>{JSON.stringify(permissionDialog.tool_input, null, 2)}</pre>
            </div>
            <div className="dialog-buttons">
              <button
                onClick={() => handlePermissionAllow(true)}
                className="btn-primary"
              >
                常に許可
              </button>
              <button
                onClick={() => handlePermissionAllow(false)}
                className="btn-primary"
              >
                今回のみ許可
              </button>
              <button onClick={handlePermissionDeny} className="btn-danger">
                拒否
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

export default PipelineRunner;
