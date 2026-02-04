//! Speaker Diarization module using sherpa-rs
//!
//! Uses Pyannote segmentation + 3D-Speaker embeddings for identifying
//! different speakers in system audio.

use sherpa_rs::diarize::{Diarize, DiarizeConfig};
use std::path::PathBuf;

/// Diarization result with speaker-labeled segments
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiarizedSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker_id: i32,
    pub speaker_label: String,  // "Speaker 1", "Speaker 2", etc.
}

/// Speaker diarization engine configuration
pub struct SpeakerDiarizationConfig {
    pub models_dir: PathBuf,
    pub num_speakers: Option<i32>,  // None = auto-detect
    pub threshold: f32,             // Clustering threshold (default 0.5)
}

impl Default for SpeakerDiarizationConfig {
    fn default() -> Self {
        let models_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("second-brain")
            .join("models");

        Self {
            models_dir,
            num_speakers: None,  // Auto-detect
            threshold: 0.5,
        }
    }
}

/// Speaker diarization engine
pub struct SpeakerDiarizationEngine {
    config: SpeakerDiarizationConfig,
    diarizer: Option<Diarize>,
}

impl SpeakerDiarizationEngine {
    /// Create a new speaker diarization engine
    pub fn new(config: SpeakerDiarizationConfig) -> Self {
        Self {
            config,
            diarizer: None,
        }
    }

    /// Initialize the diarization engine (load models)
    pub fn initialize(&mut self) -> Result<(), String> {
        let models_dir = &self.config.models_dir;

        // Find segmentation model
        let segmentation_model = if models_dir.join("sherpa-onnx-pyannote-segmentation-3-0").join("model.onnx").exists() {
            models_dir.join("sherpa-onnx-pyannote-segmentation-3-0").join("model.onnx")
        } else if models_dir.join("model.onnx").exists() {
            models_dir.join("model.onnx")
        } else {
            return Err(format!(
                "Speaker segmentation model not found in {:?}",
                models_dir
            ));
        };

        // Find speaker embedding model
        let embedding_model = models_dir.join("3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx");
        if !embedding_model.exists() {
            return Err(format!(
                "Speaker embedding model not found: {:?}",
                embedding_model
            ));
        }

        let diarize_config = DiarizeConfig {
            num_clusters: self.config.num_speakers,
            threshold: Some(self.config.threshold),
            min_duration_on: Some(0.0),
            min_duration_off: Some(0.5),
            provider: None,
            debug: false,
        };

        let diarizer = Diarize::new(segmentation_model, embedding_model, diarize_config)
            .map_err(|e| format!("Failed to initialize diarizer: {:?}", e))?;

        self.diarizer = Some(diarizer);
        println!("Speaker diarization engine initialized");
        Ok(())
    }

    /// Process audio samples and return speaker-labeled segments
    ///
    /// # Arguments
    /// * `samples` - Audio samples at 16kHz mono
    /// * `sample_rate` - Sample rate (will resample if not 16kHz)
    ///
    /// # Returns
    /// Vector of diarized segments with speaker IDs
    pub fn process(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<Vec<DiarizedSegment>, String> {
        let diarizer = self.diarizer.as_mut()
            .ok_or("Diarization engine not initialized")?;

        // Resample to 16kHz if needed
        let samples_16k = if sample_rate != 16000 {
            resample(&samples, sample_rate, 16000)
        } else {
            samples
        };

        // Run diarization
        let segments = diarizer.compute(samples_16k, None)
            .map_err(|e| format!("Diarization failed: {:?}", e))?;

        // Convert to our format with labels
        let diarized: Vec<DiarizedSegment> = segments
            .into_iter()
            .map(|seg| DiarizedSegment {
                start_ms: (seg.start * 1000.0) as u64,
                end_ms: (seg.end * 1000.0) as u64,
                speaker_id: seg.speaker,
                speaker_label: format!("Speaker {}", seg.speaker + 1),
            })
            .collect();

        println!("[Diarization] Found {} segments with {} unique speakers",
            diarized.len(),
            diarized.iter().map(|s| s.speaker_id).collect::<std::collections::HashSet<_>>().len()
        );

        Ok(diarized)
    }

    /// Check if the engine is initialized
    pub fn is_initialized(&self) -> bool {
        self.diarizer.is_some()
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

/// Map diarization results back to transcript segments
///
/// Given a list of transcript segments with timestamps and diarization results,
/// relabel the speakers based on overlapping time ranges.
pub fn relabel_speakers(
    segments: &mut Vec<(u64, u64, String, String)>,  // (start_ms, end_ms, original_speaker, text)
    diarization: &[DiarizedSegment],
) {
    for (start_ms, end_ms, speaker, _text) in segments.iter_mut() {
        // Only relabel "Guest" speakers
        if speaker != "Guest" {
            continue;
        }

        // Find overlapping diarization segment
        let segment_mid = (*start_ms + *end_ms) / 2;

        if let Some(diar_seg) = diarization.iter().find(|d| {
            segment_mid >= d.start_ms && segment_mid <= d.end_ms
        }) {
            *speaker = diar_seg.speaker_label.clone();
        }
    }
}
