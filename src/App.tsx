import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

function App() {
  const [youtubeUrl, setYoutubeUrl] = useState("");
  const [output, setOutput] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [claudeRunning, setClaudeRunning] = useState(false);
  const [subtitleLang, setSubtitleLang] = useState("en");

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

  // 出力を追加
  const addOutput = (text: string) => {
    setOutput((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${text}`]);
  };

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
      </div>

      {/* YouTube URL入力 */}
      <div className="input-section">
        <label htmlFor="youtube-url">YouTube URL</label>
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
        <button onClick={handleSpawnClaude} disabled={isLoading || claudeRunning}>
          Claude Code起動
        </button>
        <button onClick={handleTestCommand} disabled={isLoading}>
          テストコマンド
        </button>
        <button onClick={handleFetchVideoInfo} disabled={isLoading || !youtubeUrl}>
          動画情報取得
        </button>
      </div>

      {/* 字幕関連ボタン */}
      <div className="button-group">
        <button onClick={handleListSubtitles} disabled={isLoading || !youtubeUrl}>
          字幕一覧取得
        </button>
        <button onClick={() => handleDownloadSubtitle(false)} disabled={isLoading || !youtubeUrl}>
          字幕DL
        </button>
        <button onClick={() => handleDownloadSubtitle(true)} disabled={isLoading || !youtubeUrl}>
          自動字幕DL
        </button>
      </div>

      {/* 出力ログ */}
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
    </main>
  );
}

export default App;
