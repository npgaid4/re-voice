use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::io::{Read, Write};
use std::sync::Arc;

/// PTYマネージャー - Claude Code等のCLIツールとの通信を管理
pub struct PtyManager {
    pair: Option<PtyPair>,
    reader: Arc<Mutex<Option<Box<dyn Read + Send>>>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            pair: None,
            reader: Arc::new(Mutex::new(None)),
            writer: Arc::new(Mutex::new(None)),
        }
    }

    /// Claude CodeをPTYで起動
    pub fn spawn_claude_code(&mut self) -> Result<()> {
        let pty_system = native_pty_system();

        // 80x24の仮想端末を作成
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| anyhow!("Failed to create PTY: {}", e))?;

        // Claude Codeを起動
        let mut cmd = CommandBuilder::new("claude");
        cmd.arg("code");

        let _child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| anyhow!("Failed to spawn claude code: {}", e))?;

        // リーダーとライターを保存
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| anyhow!("Failed to clone reader: {}", e))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| anyhow!("Failed to take writer: {}", e))?;

        *self.reader.lock() = Some(reader);
        *self.writer.lock() = Some(writer);
        self.pair = Some(pair);

        Ok(())
    }

    /// 画面出力を読み取り
    pub fn read_output(&self, buffer: &mut [u8]) -> Result<usize> {
        let mut reader = self.reader.lock();
        if let Some(ref mut r) = *reader {
            r.read(buffer)
                .map_err(|e| anyhow!("Failed to read from PTY: {}", e))
        } else {
            Err(anyhow!("PTY not initialized"))
        }
    }

    /// 入力を送信
    pub fn write_input(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock();
        if let Some(ref mut w) = *writer {
            w.write_all(data)
                .map_err(|e| anyhow!("Failed to write to PTY: {}", e))?;
            w.flush()
                .map_err(|e| anyhow!("Failed to flush PTY: {}", e))?;
            Ok(())
        } else {
            Err(anyhow!("PTY not initialized"))
        }
    }

    /// メッセージを送信（改行付き）
    pub fn send_message(&self, message: &str) -> Result<()> {
        self.write_input(format!("{}\n", message).as_bytes())
    }

    /// PTYが起動しているか確認
    pub fn is_running(&self) -> bool {
        self.pair.is_some()
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}
