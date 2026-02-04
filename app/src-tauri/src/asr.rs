//! ASR (Automatic Speech Recognition) module using sherpa-rs
//!
//! Uses Silero VAD for speech detection and SenseVoice for transcription.
//! SenseVoice provides: transcription + emotion detection + audio event detection.
//! Audio is processed in segments detected by VAD.

use sherpa_rs::sense_voice::{SenseVoiceConfig, SenseVoiceRecognizer};
use sherpa_rs::silero_vad::{SileroVad, SileroVadConfig};
use std::path::PathBuf;

/// Detected emotion from SenseVoice
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum Emotion {
    Neutral,
    Happy,
    Sad,
    Angry,
    Fearful,
    Disgusted,
    Surprised,
    Unknown,
}

impl Default for Emotion {
    fn default() -> Self {
        Emotion::Neutral
    }
}

/// Detected audio events from SenseVoice
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub enum AudioEvent {
    Speech,
    Laughter,
    Applause,
    Music,
    Noise,
    Other(String),
}

/// Transcription result with emotion and events
#[derive(Debug, Clone, serde::Serialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub source: String,           // "microphone" or "system"
    pub timestamp_ms: u64,
    pub is_final: bool,
    pub language: String,         // Detected language (zh/en/ja/ko/yue)
    pub emotion: Emotion,         // Detected emotion
    pub audio_events: Vec<AudioEvent>, // Detected audio events
    pub is_turn_complete: bool,   // Whether speaker has finished their turn
    pub turn_confidence: f32,     // Confidence of turn completion (0-1)
}

/// ASR configuration
pub struct AsrConfig {
    pub models_dir: PathBuf,
    pub sample_rate: u32,
}

impl Default for AsrConfig {
    fn default() -> Self {
        let models_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("second-brain")
            .join("models");

        Self {
            models_dir,
            sample_rate: 16000,
        }
    }
}

/// ASR Engine that processes audio and emits transcriptions
pub struct AsrEngine {
    config: AsrConfig,
    // Separate VADs for microphone and system audio
    mic_vad: Option<SileroVad>,
    system_vad: Option<SileroVad>,
    recognizer: Option<SenseVoiceRecognizer>,
}

impl AsrEngine {
    /// Create a new ASR engine
    pub fn new(config: AsrConfig) -> Self {
        Self {
            config,
            mic_vad: None,
            system_vad: None,
            recognizer: None,
        }
    }

    /// Initialize the ASR engine (load models)
    pub fn initialize(&mut self) -> Result<(), String> {
        let models_dir = &self.config.models_dir;

        // Initialize Silero VAD
        let vad_model = models_dir.join("silero_vad.onnx");
        if !vad_model.exists() {
            return Err(format!("VAD model not found: {:?}", vad_model));
        }

        // Create VAD config
        let create_vad_config = || SileroVadConfig {
            model: vad_model.to_string_lossy().to_string(),
            min_silence_duration: 0.5,  // 500ms silence to end speech
            min_speech_duration: 0.25,  // 250ms min speech
            max_speech_duration: 30.0,  // Max 30s speech segment
            threshold: 0.5,
            sample_rate: self.config.sample_rate,
            window_size: 512,
            provider: None,
            num_threads: None,
            debug: false,
        };

        // Create separate VADs for mic and system audio (independent state)
        self.mic_vad = Some(SileroVad::new(create_vad_config(), 60.0)
            .map_err(|e| format!("Mic VAD init error: {:?}", e))?);
        self.system_vad = Some(SileroVad::new(create_vad_config(), 60.0)
            .map_err(|e| format!("System VAD init error: {:?}", e))?);

        // Initialize SenseVoice recognizer
        let sensevoice_dir = models_dir.join("sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17");

        // Try int8 model first, fallback to fp32
        let model_path = if sensevoice_dir.join("model.int8.onnx").exists() {
            sensevoice_dir.join("model.int8.onnx")
        } else {
            sensevoice_dir.join("model.onnx")
        };

        let tokens_path = sensevoice_dir.join("tokens.txt");

        if !model_path.exists() {
            return Err(format!("SenseVoice model not found: {:?}", model_path));
        }
        if !tokens_path.exists() {
            return Err(format!("SenseVoice tokens not found: {:?}", tokens_path));
        }

        let sensevoice_config = SenseVoiceConfig {
            model: model_path.to_string_lossy().to_string(),
            tokens: tokens_path.to_string_lossy().to_string(),
            language: "auto".to_string(),  // Auto-detect language
            use_itn: true,                 // Inverse text normalization
            provider: None,
            num_threads: Some(4),
            debug: false,
        };

        self.recognizer = Some(
            SenseVoiceRecognizer::new(sensevoice_config)
                .map_err(|e| format!("SenseVoice init error: {:?}", e))?
        );

        println!("[ASR] SenseVoice engine initialized");
        Ok(())
    }

    /// Process audio from microphone
    pub fn process_microphone(&mut self, samples: &[f32], sample_rate: u32) -> Option<TranscriptionResult> {
        self.process_audio(samples, sample_rate, "microphone")
    }

    /// Process audio from system (guests)
    pub fn process_system(&mut self, samples: &[f32], sample_rate: u32) -> Option<TranscriptionResult> {
        self.process_audio(samples, sample_rate, "system")
    }

    /// Process audio and return transcription when speech segment ends
    fn process_audio(&mut self, samples: &[f32], sample_rate: u32, source: &str) -> Option<TranscriptionResult> {
        // Get the appropriate VAD based on source
        let vad = if source == "microphone" {
            self.mic_vad.as_mut()?
        } else {
            self.system_vad.as_mut()?
        };
        let recognizer = self.recognizer.as_mut()?;

        // Resample if needed (silent - this runs on every audio chunk)
        let resampled = if sample_rate != self.config.sample_rate {
            resample(samples, sample_rate, self.config.sample_rate)
        } else {
            samples.to_vec()
        };

        // Feed samples to VAD
        vad.accept_waveform(resampled);

        // Check for completed speech segments
        let mut result: Option<TranscriptionResult> = None;

        // Log when VAD has detected a speech segment (especially for system audio)
        if !vad.is_empty() && source != "microphone" {
            println!("[ASR-{}] VAD detected speech segment!", source);
        }

        while !vad.is_empty() {
            // Get speech segment from VAD
            let segment = vad.front();
            let speech_samples = segment.samples.clone();
            vad.pop();

            // Only transcribe if segment has enough audio (> 250ms)
            if speech_samples.len() > self.config.sample_rate as usize / 4 {
                // Transcribe with SenseVoice
                let sensevoice_result = recognizer.transcribe(self.config.sample_rate, &speech_samples);

                // Parse the raw text to extract emotion, events, and clean text
                let parsed = parse_sensevoice_output(&sensevoice_result.text);

                if !parsed.text.trim().is_empty() {
                    result = Some(TranscriptionResult {
                        text: parsed.text,
                        source: source.to_string(),
                        timestamp_ms: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                        is_final: true,
                        language: sensevoice_result.lang.clone(),
                        emotion: parsed.emotion,
                        audio_events: parsed.events,
                        is_turn_complete: false,  // Will be set by Smart Turn
                        turn_confidence: 0.0,
                    });
                }
            }
        }

        result
    }

    /// Reset the engine state
    pub fn reset(&mut self) {
        if let Some(vad) = self.mic_vad.as_mut() {
            vad.clear();
        }
        if let Some(vad) = self.system_vad.as_mut() {
            vad.clear();
        }
    }
}

/// Parsed output from SenseVoice
struct ParsedSenseVoiceOutput {
    text: String,
    emotion: Emotion,
    events: Vec<AudioEvent>,
}

/// Parse SenseVoice output to extract emotion, events, and clean text
///
/// SenseVoice outputs special tokens like:
/// - Emotions: <|HAPPY|>, <|SAD|>, <|ANGRY|>, <|NEUTRAL|>, etc.
/// - Events: <|Speech|>, <|Laughter|>, <|Applause|>, <|Music|>, <|BGM|>, <|Noise|>
/// - Language: <|zh|>, <|en|>, <|ja|>, <|ko|>, <|yue|>
///
/// Example output: "<|en|><|NEUTRAL|><|Speech|>Hello how are you<|/Speech|>"
fn parse_sensevoice_output(raw_text: &str) -> ParsedSenseVoiceOutput {
    let mut emotion = Emotion::Neutral;
    let mut events = Vec::new();
    let mut clean_text = raw_text.to_string();

    // Extract emotion
    if raw_text.contains("<|HAPPY|>") || raw_text.contains("<|happy|>") {
        emotion = Emotion::Happy;
    } else if raw_text.contains("<|SAD|>") || raw_text.contains("<|sad|>") {
        emotion = Emotion::Sad;
    } else if raw_text.contains("<|ANGRY|>") || raw_text.contains("<|angry|>") {
        emotion = Emotion::Angry;
    } else if raw_text.contains("<|FEARFUL|>") || raw_text.contains("<|fearful|>") {
        emotion = Emotion::Fearful;
    } else if raw_text.contains("<|DISGUSTED|>") || raw_text.contains("<|disgusted|>") {
        emotion = Emotion::Disgusted;
    } else if raw_text.contains("<|SURPRISED|>") || raw_text.contains("<|surprised|>") {
        emotion = Emotion::Surprised;
    } else if raw_text.contains("<|NEUTRAL|>") || raw_text.contains("<|neutral|>") {
        emotion = Emotion::Neutral;
    }

    // Extract audio events
    if raw_text.contains("<|Speech|>") || raw_text.contains("<|speech|>") {
        events.push(AudioEvent::Speech);
    }
    if raw_text.contains("<|Laughter|>") || raw_text.contains("<|laughter|>") {
        events.push(AudioEvent::Laughter);
    }
    if raw_text.contains("<|Applause|>") || raw_text.contains("<|applause|>") {
        events.push(AudioEvent::Applause);
    }
    if raw_text.contains("<|Music|>") || raw_text.contains("<|music|>") ||
       raw_text.contains("<|BGM|>") || raw_text.contains("<|bgm|>") {
        events.push(AudioEvent::Music);
    }
    if raw_text.contains("<|Noise|>") || raw_text.contains("<|noise|>") {
        events.push(AudioEvent::Noise);
    }

    // Remove all special tokens to get clean text
    // Pattern: <|...|> or <|/...|>
    let token_patterns = [
        // Language tokens
        "<|zh|>", "<|en|>", "<|ja|>", "<|ko|>", "<|yue|>",
        // Emotion tokens
        "<|HAPPY|>", "<|SAD|>", "<|ANGRY|>", "<|NEUTRAL|>",
        "<|FEARFUL|>", "<|DISGUSTED|>", "<|SURPRISED|>",
        "<|happy|>", "<|sad|>", "<|angry|>", "<|neutral|>",
        "<|fearful|>", "<|disgusted|>", "<|surprised|>",
        // Event tokens
        "<|Speech|>", "<|/Speech|>", "<|speech|>", "<|/speech|>",
        "<|Laughter|>", "<|/Laughter|>", "<|laughter|>", "<|/laughter|>",
        "<|Applause|>", "<|/Applause|>", "<|applause|>", "<|/applause|>",
        "<|Music|>", "<|/Music|>", "<|music|>", "<|/music|>",
        "<|BGM|>", "<|/BGM|>", "<|bgm|>", "<|/bgm|>",
        "<|Noise|>", "<|/Noise|>", "<|noise|>", "<|/noise|>",
        // Other common tokens
        "<|startoftranscript|>", "<|endoftext|>", "<|nospeech|>",
        "<|NOISE|>", "<|EMO_UNKNOWN|>", "<|Event_UNK|>",
    ];

    for pattern in token_patterns {
        clean_text = clean_text.replace(pattern, "");
    }

    // Clean up whitespace
    clean_text = clean_text.trim().to_string();

    // If no events detected, default to Speech
    if events.is_empty() && !clean_text.is_empty() {
        events.push(AudioEvent::Speech);
    }

    ParsedSenseVoiceOutput {
        text: clean_text,
        emotion,
        events,
    }
}

/// Simple linear resampling
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let new_len = (samples.len() as f64 / ratio) as usize;
    let mut result = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_idx = i as f64 * ratio;
        let src_floor = src_idx.floor() as usize;
        let src_ceil = (src_floor + 1).min(samples.len() - 1);
        let t = src_idx - src_floor as f64;

        let value = samples[src_floor] as f64 * (1.0 - t) + samples[src_ceil] as f64 * t;
        result.push(value as f32);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sensevoice_output() {
        // Test with emotion and speech event
        let output = "<|en|><|HAPPY|><|Speech|>Hello how are you<|/Speech|>";
        let parsed = parse_sensevoice_output(output);
        assert_eq!(parsed.text, "Hello how are you");
        assert_eq!(parsed.emotion, Emotion::Happy);
        assert!(parsed.events.contains(&AudioEvent::Speech));

        // Test with laughter
        let output2 = "<|en|><|NEUTRAL|><|Laughter|>haha<|/Laughter|>";
        let parsed2 = parse_sensevoice_output(output2);
        assert!(parsed2.events.contains(&AudioEvent::Laughter));

        // Test plain text
        let output3 = "Just plain text";
        let parsed3 = parse_sensevoice_output(output3);
        assert_eq!(parsed3.text, "Just plain text");
        assert_eq!(parsed3.emotion, Emotion::Neutral);
    }
}
