//! PTY Transport for ACP messages

use anyhow::Result;

use crate::pty::PtyManager;
use crate::acp::message::{ACPFrame, ACPMessage};

/// PTY-based transport for ACP messages
pub struct PtyTransport {
    pty: PtyManager,
    read_buffer: Vec<u8>,
}

impl PtyTransport {
    /// Create a new PTY transport
    pub fn new() -> Self {
        Self {
            pty: PtyManager::new(),
            read_buffer: Vec::with_capacity(65536),
        }
    }

    /// Check if PTY is running
    pub fn is_running(&self) -> bool {
        self.pty.is_running()
    }

    /// Spawn the underlying process
    pub fn spawn(&mut self) -> Result<()> {
        self.pty.spawn_claude_code()
    }

    /// Send an ACP message
    pub fn send(&self, message: &ACPMessage) -> Result<()> {
        let frame = ACPFrame::encode(message)?;
        self.pty.send_message(&frame)
    }

    /// Send raw text (not framed as ACP)
    pub fn send_raw(&self, text: &str) -> Result<()> {
        self.pty.send_message(text)
    }

    /// Read and parse ACP messages from PTY output
    pub fn read_messages(&mut self) -> Result<Vec<ACPMessage>> {
        let mut buffer = [0u8; 8192];
        let n = self.pty.read_output(&mut buffer)?;

        if n > 0 {
            self.read_buffer.extend_from_slice(&buffer[..n]);
        }

        // Try to decode as UTF-8
        if let Ok(text) = std::str::from_utf8(&self.read_buffer) {
            let messages: Vec<ACPMessage> = ACPFrame::parse(text)
                .into_iter()
                .filter_map(|r| r.ok())
                .collect();

            // Clear processed data (simplified - in production would track position)
            if !messages.is_empty() {
                self.read_buffer.clear();
            }

            Ok(messages)
        } else {
            Ok(vec![])
        }
    }

    /// Read raw output without parsing
    pub fn read_raw(&mut self) -> Result<String> {
        let mut buffer = [0u8; 8192];
        let n = self.pty.read_output(&mut buffer)?;

        if n > 0 {
            String::from_utf8(buffer[..n].to_vec())
                .map_err(|e| anyhow::anyhow!("UTF-8 decode error: {}", e))
        } else {
            Ok(String::new())
        }
    }

    /// Cancel current operation (send Ctrl+C)
    pub fn cancel(&self) -> Result<()> {
        self.pty.send_message("\x03")
    }
}

impl Default for PtyTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_creation() {
        let transport = PtyTransport::new();
        assert!(!transport.is_running());
    }
}
