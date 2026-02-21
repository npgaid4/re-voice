//! VOICEVOX API クライアント
//!
//! VOICEVOX Engine (http://localhost:50021) と通信して
//! テキストから音声を生成する。

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// VOICEVOX APIエラー
#[derive(Debug, Error)]
pub enum VoicevoxError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Audio synthesis failed: {0}")]
    SynthesisFailed(String),

    #[error("File I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("VOICEVOX Engine not running: {0}")]
    EngineNotRunning(String),
}

/// VOICEVOX話者情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Speaker {
    pub name: String,
    pub speaker_uuid: String,
    pub styles: Vec<SpeakerStyle>,
}

/// 話者スタイル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerStyle {
    pub name: String,
    pub id: i32,
}

/// AudioQueryレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioQuery {
    pub accent_phrases: Vec<AccentPhrase>,
    pub speed_scale: f64,
    pub pitch_scale: f64,
    pub intonation_scale: f64,
    pub volume_scale: f64,
    pub pre_phoneme_length: f64,
    pub post_phoneme_length: f64,
    pub output_sampling_rate: i32,
    pub output_stereo: bool,
    pub kana: Option<String>,
}

/// アクセント句
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccentPhrase {
    pub moras: Vec<Mora>,
    pub accent: i32,
    pub pause_mora: Option<Mora>,
    #[serde(default)]
    pub is_interrogative: bool,
}

/// モーラ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mora {
    pub text: String,
    pub consonant: Option<String>,
    pub consonant_length: Option<f64>,
    pub vowel: String,
    pub vowel_length: f64,
    pub pitch: f64,
}

/// 音声合成オプション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisOptions {
    /// 話者ID
    pub speaker: i32,
    /// 話速（1.0が標準）
    #[serde(default = "default_speed")]
    pub speed_scale: f64,
    /// 音高（1.0が標準）
    #[serde(default = "default_pitch")]
    pub pitch_scale: f64,
    /// 抑揚（1.0が標準）
    #[serde(default = "default_intonation")]
    pub intonation_scale: f64,
    /// 音量（1.0が標準）
    #[serde(default = "default_volume")]
    pub volume_scale: f64,
}

fn default_speed() -> f64 { 1.0 }
fn default_pitch() -> f64 { 0.0 }
fn default_intonation() -> f64 { 1.0 }
fn default_volume() -> f64 { 1.0 }

impl Default for SynthesisOptions {
    fn default() -> Self {
        Self {
            speaker: 1, // ずんだもん
            speed_scale: 1.0,
            pitch_scale: 0.0,
            intonation_scale: 1.0,
            volume_scale: 1.0,
        }
    }
}

/// VOICEVOX API クライアント
pub struct VoicevoxClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl VoicevoxClient {
    /// 新しいクライアントを作成
    pub fn new() -> Self {
        Self {
            base_url: "http://localhost:50021".to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new()),
        }
    }

    /// カスタムURLでクライアントを作成
    pub fn with_url(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new()),
        }
    }

    /// VOICEVOX Engineが起動しているか確認
    pub fn is_running(&self) -> bool {
        match self.client.get(&format!("{}/version", self.base_url)).send() {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// バージョンを取得
    pub fn get_version(&self) -> Result<String, VoicevoxError> {
        let resp = self.client
            .get(&format!("{}/version", self.base_url))
            .send()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(VoicevoxError::EngineNotRunning(
                format!("Status: {}", resp.status())
            ));
        }

        resp.text()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))
    }

    /// 話者一覧を取得
    pub fn get_speakers(&self) -> Result<Vec<Speaker>, VoicevoxError> {
        let resp = self.client
            .get(&format!("{}/speakers", self.base_url))
            .send()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(VoicevoxError::HttpError(
                format!("Failed to get speakers: {}", resp.status())
            ));
        }

        let body = resp.text()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        let speakers: Vec<Speaker> = serde_json::from_str(&body)?;
        Ok(speakers)
    }

    /// AudioQueryを作成
    pub fn create_audio_query(
        &self,
        text: &str,
        speaker: i32,
    ) -> Result<AudioQuery, VoicevoxError> {
        let url = format!(
            "{}/audio_query?text={}&speaker={}",
            self.base_url,
            urlencoding::encode(text),
            speaker
        );

        let resp = self.client
            .post(&url)
            .send()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let error_body = resp.text().unwrap_or_default();
            return Err(VoicevoxError::SynthesisFailed(
                format!("Audio query failed: {}", error_body)
            ));
        }

        let query: AudioQuery = resp.json()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        Ok(query)
    }

    /// テキストから音声を合成してファイルに保存
    pub fn text_to_speech(
        &self,
        text: &str,
        speaker: i32,
        output_path: &str,
    ) -> Result<String, VoicevoxError> {
        self.text_to_speech_with_options(text, SynthesisOptions {
            speaker,
            ..Default::default()
        }, output_path)
    }

    /// オプション付きでテキストから音声を合成
    pub fn text_to_speech_with_options(
        &self,
        text: &str,
        options: SynthesisOptions,
        output_path: &str,
    ) -> Result<String, VoicevoxError> {
        // Step 1: AudioQueryを作成
        let mut query = self.create_audio_query(text, options.speaker)?;

        // Step 2: パラメータを調整
        query.speed_scale = options.speed_scale;
        query.pitch_scale = options.pitch_scale;
        query.intonation_scale = options.intonation_scale;
        query.volume_scale = options.volume_scale;

        // Step 3: 音声合成
        let url = format!(
            "{}/synthesis?speaker={}",
            self.base_url,
            options.speaker
        );

        let resp = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&query)?)
            .send()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            let error_body = resp.text().unwrap_or_default();
            return Err(VoicevoxError::SynthesisFailed(
                format!("Synthesis failed: {}", error_body)
            ));
        }

        // Step 4: WAVデータを保存
        let wav_data = resp.bytes()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        // ディレクトリを作成（存在しない場合）
        if let Some(parent) = Path::new(output_path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        std::fs::write(output_path, &wav_data)?;

        crate::log::info("VoicevoxClient", &format!(
            "Saved audio: {} bytes to {}",
            wav_data.len(),
            output_path
        ));

        Ok(output_path.to_string())
    }

    /// 複数テキストを連続して合成
    pub fn synthesize_batch(
        &self,
        texts: &[String],
        speaker: i32,
        output_dir: &str,
    ) -> Result<Vec<String>, VoicevoxError> {
        let mut outputs = Vec::new();

        for (i, text) in texts.iter().enumerate() {
            let output_path = format!("{}/audio_{:04}.wav", output_dir, i);
            self.text_to_speech(text, speaker, &output_path)?;
            outputs.push(output_path);
        }

        Ok(outputs)
    }

    /// アクセント句を調整してから合成
    pub fn synthesize_with_accent(
        &self,
        text: &str,
        speaker: i32,
        accent_positions: &[usize],
        output_path: &str,
    ) -> Result<String, VoicevoxError> {
        let mut query = self.create_audio_query(text, speaker)?;

        // アクセント位置を調整
        for (i, &accent) in accent_positions.iter().enumerate() {
            if i < query.accent_phrases.len() {
                query.accent_phrases[i].accent = accent as i32;
            }
        }

        // 合成
        let url = format!("{}/synthesis?speaker={}", self.base_url, speaker);

        let resp = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&query)?)
            .send()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(VoicevoxError::SynthesisFailed(
                format!("Synthesis failed: {}", resp.status())
            ));
        }

        let wav_data = resp.bytes()
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        std::fs::write(output_path, &wav_data)?;

        Ok(output_path.to_string())
    }
}

impl Default for VoicevoxClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 非同期版VOICEVOXクライアント
pub struct VoicevoxClientAsync {
    base_url: String,
    client: reqwest::Client,
}

impl VoicevoxClientAsync {
    pub fn new() -> Self {
        Self {
            base_url: "http://localhost:50021".to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// VOICEVOX Engineが起動しているか確認
    pub async fn is_running(&self) -> bool {
        match self.client.get(&format!("{}/version", self.base_url)).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// 話者一覧を取得
    pub async fn get_speakers(&self) -> Result<Vec<Speaker>, VoicevoxError> {
        let resp = self.client
            .get(&format!("{}/speakers", self.base_url))
            .send()
            .await
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(VoicevoxError::HttpError(
                format!("Failed to get speakers: {}", resp.status())
            ));
        }

        let speakers: Vec<Speaker> = resp.json().await
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        Ok(speakers)
    }

    /// テキストから音声を合成
    pub async fn text_to_speech(
        &self,
        text: &str,
        speaker: i32,
        output_path: &str,
    ) -> Result<String, VoicevoxError> {
        // AudioQuery作成
        let url = format!(
            "{}/audio_query?text={}&speaker={}",
            self.base_url,
            urlencoding::encode(text),
            speaker
        );

        let query: AudioQuery = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?
            .json()
            .await
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        // 合成
        let url = format!("{}/synthesis?speaker={}", self.base_url, speaker);

        let wav_data = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&query)?)
            .send()
            .await
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| VoicevoxError::HttpError(e.to_string()))?;

        // ファイル保存
        if let Some(parent) = Path::new(output_path).parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        std::fs::write(output_path, &wav_data)?;

        Ok(output_path.to_string())
    }
}

impl Default for VoicevoxClientAsync {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = VoicevoxClient::new();
        assert_eq!(client.base_url, "http://localhost:50021");
    }

    #[test]
    fn test_custom_url() {
        let client = VoicevoxClient::with_url("http://custom:50021");
        assert_eq!(client.base_url, "http://custom:50021");
    }

    #[test]
    fn test_synthesis_options_default() {
        let options = SynthesisOptions::default();
        assert_eq!(options.speaker, 1);
        assert_eq!(options.speed_scale, 1.0);
    }

    // 注意: 以下のテストはVOICEVOX Engineが起動している場合のみ成功します

    #[test]
    #[ignore] // VOICEVOX Engineが必要
    fn test_get_speakers() {
        let client = VoicevoxClient::new();
        if client.is_running() {
            let speakers = client.get_speakers().unwrap();
            assert!(!speakers.is_empty());

            // ずんだもんが含まれているか確認
            let has_zundamon = speakers.iter().any(|s| s.name.contains("ずんだもん"));
            assert!(has_zundamon);
        }
    }

    #[test]
    #[ignore] // VOICEVOX Engineが必要
    fn test_text_to_speech() {
        let client = VoicevoxClient::new();
        if client.is_running() {
            let result = client.text_to_speech(
                "こんにちは、世界です！",
                1, // ずんだもん
                "/tmp/test_voicevox.wav"
            );

            assert!(result.is_ok());
            assert!(std::path::Path::new("/tmp/test_voicevox.wav").exists());
        }
    }
}
