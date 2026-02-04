//! Smart Turn v3 - Semantic turn detection
//!
//! Determines when a speaker has finished their turn using audio analysis.
//! Uses Whisper Tiny encoder + classifier to analyze speech patterns.
//!
//! Input: 16kHz mono audio (max 8 seconds)
//! Output: prediction (0=incomplete, 1=complete) + confidence

use ndarray::{Array2, Array3};
use ort::session::{builder::GraphOptimizationLevel, Session};
use std::f32::consts::PI;
use std::path::PathBuf;

/// Smart Turn prediction result
#[derive(Debug, Clone, serde::Serialize)]
pub struct TurnPrediction {
    /// 1 if turn is complete, 0 if incomplete
    pub prediction: i32,
    /// Confidence score (0.0 to 1.0)
    pub probability: f32,
    /// Whether the speaker is likely done
    pub is_complete: bool,
}

/// Smart Turn v3 configuration
pub struct SmartTurnConfig {
    /// Confidence threshold for turn completion (default 0.7)
    pub threshold: f32,
    /// Sample rate (must be 16000)
    pub sample_rate: u32,
    /// Max audio duration in seconds (8s for Smart Turn)
    pub max_duration_secs: f32,
}

impl Default for SmartTurnConfig {
    fn default() -> Self {
        Self {
            threshold: 0.7,
            sample_rate: 16000,
            max_duration_secs: 8.0,
        }
    }
}

/// Smart Turn v3 engine for turn detection
pub struct SmartTurnEngine {
    session: Option<Session>,
    config: SmartTurnConfig,
    // Pre-computed mel filterbank
    mel_filters: Array2<f32>,
}

// Whisper feature extraction constants
const N_FFT: usize = 400;       // FFT size
const HOP_LENGTH: usize = 160;  // Hop length (10ms at 16kHz)
const N_MELS: usize = 80;       // Number of mel bins
const SAMPLE_RATE: u32 = 16000;

impl SmartTurnEngine {
    /// Create a new Smart Turn engine
    pub fn new(config: SmartTurnConfig) -> Self {
        // Pre-compute mel filterbank
        let mel_filters = create_mel_filterbank(SAMPLE_RATE, N_FFT, N_MELS);

        Self {
            session: None,
            config,
            mel_filters,
        }
    }

    /// Initialize the engine (load model)
    pub fn initialize(&mut self, models_dir: &PathBuf) -> Result<(), String> {
        let model_path = models_dir.join("smart-turn-v3.onnx");

        if !model_path.exists() {
            return Err(format!("Smart Turn model not found: {:?}", model_path));
        }

        let session = Session::builder()
            .map_err(|e| format!("Failed to create session builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| format!("Failed to set optimization level: {}", e))?
            .with_intra_threads(2)
            .map_err(|e| format!("Failed to set threads: {}", e))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("Failed to load Smart Turn model: {}", e))?;

        self.session = Some(session);
        println!("[SmartTurn] Engine initialized");
        Ok(())
    }

    /// Predict if a turn is complete
    ///
    /// # Arguments
    /// * `samples` - Audio samples (16kHz mono, normalized to [-1, 1])
    ///
    /// # Returns
    /// Turn prediction with confidence
    pub fn predict(&self, samples: &[f32]) -> Result<TurnPrediction, String> {
        let session = self.session.as_ref()
            .ok_or("Smart Turn not initialized")?;

        // Ensure audio is normalized
        let normalized = normalize_audio(samples);

        // Truncate or pad to max duration (8 seconds = 128000 samples)
        let max_samples = (self.config.max_duration_secs * SAMPLE_RATE as f32) as usize;
        let audio = if normalized.len() > max_samples {
            // Take last 8 seconds (most recent audio)
            normalized[normalized.len() - max_samples..].to_vec()
        } else {
            // Pad with zeros at the beginning
            let mut padded = vec![0.0f32; max_samples - normalized.len()];
            padded.extend_from_slice(&normalized);
            padded
        };

        // Compute mel spectrogram features (Whisper-style)
        let mel_features = self.compute_mel_spectrogram(&audio)?;

        // Run inference
        // Input shape: [batch, n_mels, time] = [1, 80, 800]
        let input = Array3::from_shape_vec(
            (1, N_MELS, mel_features.ncols()),
            mel_features.iter().cloned().collect()
        ).map_err(|e| format!("Failed to create input array: {}", e))?;

        let outputs = session.run(
            ort::inputs![input].map_err(|e| format!("Input error: {}", e))?
        ).map_err(|e| format!("Inference failed: {}", e))?;

        // Extract output probability
        let output = outputs.iter().next()
            .map(|(_, v)| v)
            .ok_or("No output from model")?;

        let output_array: ndarray::ArrayViewD<f32> = output
            .try_extract_tensor()
            .map_err(|e| format!("Failed to extract output: {}", e))?;

        let probability = output_array.iter().next().copied().unwrap_or(0.5);
        let prediction = if probability >= self.config.threshold { 1 } else { 0 };

        Ok(TurnPrediction {
            prediction,
            probability,
            is_complete: prediction == 1,
        })
    }

    /// Compute mel spectrogram (Whisper-style features)
    fn compute_mel_spectrogram(&self, samples: &[f32]) -> Result<Array2<f32>, String> {
        // Number of frames
        let n_frames = (samples.len() - N_FFT) / HOP_LENGTH + 1;

        // Target is 800 frames for 8 seconds of audio
        let target_frames = 800;

        // Compute STFT magnitude
        let mut stft_mag = Array2::<f32>::zeros((N_FFT / 2 + 1, n_frames));

        for (frame_idx, start) in (0..samples.len().saturating_sub(N_FFT))
            .step_by(HOP_LENGTH)
            .enumerate()
        {
            if frame_idx >= n_frames {
                break;
            }

            // Apply Hann window and compute FFT
            let windowed: Vec<f32> = samples[start..start + N_FFT]
                .iter()
                .enumerate()
                .map(|(i, &s)| s * hann_window(i, N_FFT))
                .collect();

            // Compute magnitude spectrum using DFT
            let spectrum = compute_fft_magnitude(&windowed);

            for (i, &mag) in spectrum.iter().enumerate() {
                stft_mag[[i, frame_idx]] = mag;
            }
        }

        // Apply mel filterbank
        let mel_spec = self.mel_filters.dot(&stft_mag);

        // Convert to log scale (like Whisper)
        let log_mel: Array2<f32> = mel_spec.mapv(|x| (x.max(1e-10)).ln());

        // Normalize (Whisper-style)
        let max_val = log_mel.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let normalized = log_mel.mapv(|x| (x - max_val + 8.0).max(-8.0) / 4.0);

        // Pad or truncate to target frames
        let result = if normalized.ncols() < target_frames {
            // Pad with zeros
            let mut padded = Array2::<f32>::zeros((N_MELS, target_frames));
            for i in 0..N_MELS {
                for j in 0..normalized.ncols() {
                    padded[[i, j]] = normalized[[i, j]];
                }
            }
            padded
        } else {
            // Truncate
            normalized.slice(ndarray::s![.., ..target_frames]).to_owned()
        };

        Ok(result)
    }

    /// Check if the engine is initialized
    pub fn is_initialized(&self) -> bool {
        self.session.is_some()
    }
}

/// Normalize audio to [-1, 1] range
fn normalize_audio(samples: &[f32]) -> Vec<f32> {
    let max_val = samples.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    if max_val > 1.0 {
        samples.iter().map(|x| x / max_val).collect()
    } else if max_val < 0.001 {
        // Very quiet audio, don't amplify noise
        samples.to_vec()
    } else {
        samples.to_vec()
    }
}

/// Hann window function
fn hann_window(n: usize, length: usize) -> f32 {
    0.5 * (1.0 - (2.0 * PI * n as f32 / (length - 1) as f32).cos())
}

/// Compute FFT magnitude (simplified DFT for small N_FFT)
fn compute_fft_magnitude(samples: &[f32]) -> Vec<f32> {
    let n = samples.len();
    let n_bins = n / 2 + 1;
    let mut magnitudes = vec![0.0f32; n_bins];

    for k in 0..n_bins {
        let mut real = 0.0f32;
        let mut imag = 0.0f32;

        for (n_idx, &sample) in samples.iter().enumerate() {
            let angle = -2.0 * PI * k as f32 * n_idx as f32 / n as f32;
            real += sample * angle.cos();
            imag += sample * angle.sin();
        }

        magnitudes[k] = (real * real + imag * imag).sqrt();
    }

    magnitudes
}

/// Create mel filterbank matrix
fn create_mel_filterbank(sample_rate: u32, n_fft: usize, n_mels: usize) -> Array2<f32> {
    let n_freqs = n_fft / 2 + 1;

    // Mel frequency range
    let f_min = 0.0f32;
    let f_max = sample_rate as f32 / 2.0;

    // Convert to mel scale
    let mel_min = hz_to_mel(f_min);
    let mel_max = hz_to_mel(f_max);

    // Create mel points
    let mel_points: Vec<f32> = (0..=n_mels + 1)
        .map(|i| mel_min + (mel_max - mel_min) * i as f32 / (n_mels + 1) as f32)
        .collect();

    // Convert back to Hz
    let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

    // Convert to FFT bin indices
    let bin_points: Vec<usize> = hz_points
        .iter()
        .map(|&f| ((n_fft as f32 + 1.0) * f / sample_rate as f32).floor() as usize)
        .collect();

    // Create filterbank
    let mut filterbank = Array2::<f32>::zeros((n_mels, n_freqs));

    for m in 0..n_mels {
        let left = bin_points[m];
        let center = bin_points[m + 1];
        let right = bin_points[m + 2];

        // Rising slope
        for k in left..center {
            if k < n_freqs && center > left {
                filterbank[[m, k]] = (k - left) as f32 / (center - left) as f32;
            }
        }

        // Falling slope
        for k in center..right {
            if k < n_freqs && right > center {
                filterbank[[m, k]] = (right - k) as f32 / (right - center) as f32;
            }
        }
    }

    filterbank
}

/// Convert Hz to mel scale
fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

/// Convert mel to Hz
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0f32.powf(mel / 2595.0) - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_audio() {
        let samples = vec![0.5, -0.8, 0.3, -0.2];
        let normalized = normalize_audio(&samples);
        assert!(normalized.iter().all(|&x| x >= -1.0 && x <= 1.0));
    }

    #[test]
    fn test_mel_filterbank() {
        let fb = create_mel_filterbank(16000, 400, 80);
        assert_eq!(fb.shape(), &[80, 201]);
    }

    #[test]
    fn test_hann_window() {
        let w = hann_window(0, 400);
        assert!(w.abs() < 0.001); // Should be ~0 at start
        let w_mid = hann_window(200, 400);
        assert!((w_mid - 1.0).abs() < 0.01); // Should be ~1 at middle
    }
}
