use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::io::Write;
use tauri::{AppHandle, Emitter};

/// Model metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub size_bytes: u64,
    pub filename: String,
    pub is_archive: bool,
}

/// Download progress event
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub model_name: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub progress_percent: f32,
    pub status: String,
}

/// Model download status
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub size_bytes: u64,
}

/// Get the models directory path
pub fn get_models_dir() -> PathBuf {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("second-brain")
        .join("models");

    std::fs::create_dir_all(&data_dir).ok();
    data_dir
}

/// List of required models
pub fn get_required_models() -> Vec<ModelInfo> {
    vec![
        // Silero VAD model from sherpa-onnx releases (~2MB)
        // Must use sherpa-onnx version for compatibility with sherpa-rs
        ModelInfo {
            id: "silero-vad".to_string(),
            name: "Silero VAD".to_string(),
            url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx".to_string(),
            size_bytes: 2_000_000,
            filename: "silero_vad.onnx".to_string(),
            is_archive: false,
        },
        // SenseVoice ASR model - 5 languages (zh/en/ja/ko/yue) + emotion + audio events
        // 5-15x faster than Whisper, includes emotion detection and audio event detection
        ModelInfo {
            id: "sensevoice".to_string(),
            name: "SenseVoice ASR".to_string(),
            url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2".to_string(),
            size_bytes: 470_000_000,  // ~470MB compressed
            filename: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2".to_string(),
            is_archive: true,
        },
        // Smart Turn v3 - Semantic turn detection model (8MB int8)
        // Determines when speaker has finished their turn using audio analysis
        // From Pipecat AI - uses Whisper Tiny encoder + classifier
        ModelInfo {
            id: "smart-turn-v3".to_string(),
            name: "Smart Turn v3".to_string(),
            url: "https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/main/smart-turn-v3.0.onnx".to_string(),
            size_bytes: 8_000_000,  // ~8MB int8 quantized
            filename: "smart-turn-v3.onnx".to_string(),
            is_archive: false,
        },
        // GLiNER Multitask Large v0.5 - NER + Relationship Extraction model (~648MB quantized)
        // Supports both entity extraction and relation extraction for Graph-RAG
        ModelInfo {
            id: "gliner-model".to_string(),
            name: "GLiNER Multitask (Large)".to_string(),
            url: "https://huggingface.co/onnx-community/gliner-multitask-large-v0.5/resolve/main/onnx/model_int8.onnx".to_string(),
            size_bytes: 648_000_000,
            filename: "gliner-model.onnx".to_string(),
            is_archive: false,
        },
        // GLiNER Multitask tokenizer
        ModelInfo {
            id: "gliner-tokenizer".to_string(),
            name: "GLiNER Tokenizer".to_string(),
            url: "https://huggingface.co/onnx-community/gliner-multitask-large-v0.5/resolve/main/tokenizer.json".to_string(),
            size_bytes: 9_000_000,
            filename: "gliner-tokenizer.json".to_string(),
            is_archive: false,
        },
        // EmbeddingGemma 300M - Text embedding model (4-bit quantized ~197MB)
        // IMPORTANT: Keep original filenames - .onnx file references .onnx_data by name internally
        ModelInfo {
            id: "embedding-model".to_string(),
            name: "EmbeddingGemma (300M Q4)".to_string(),
            url: "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/main/onnx/model_q4.onnx".to_string(),
            size_bytes: 520_000,  // ~519KB for .onnx file
            filename: "model_q4.onnx".to_string(),
            is_archive: false,
        },
        // EmbeddingGemma external data file (required companion file for q4)
        // Must keep original name as .onnx references it internally
        ModelInfo {
            id: "embedding-model-data".to_string(),
            name: "EmbeddingGemma Data".to_string(),
            url: "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/main/onnx/model_q4.onnx_data".to_string(),
            size_bytes: 197_000_000,  // ~197MB
            filename: "model_q4.onnx_data".to_string(),
            is_archive: false,
        },
        // EmbeddingGemma tokenizer
        ModelInfo {
            id: "embedding-tokenizer".to_string(),
            name: "EmbeddingGemma Tokenizer".to_string(),
            url: "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/main/tokenizer.json".to_string(),
            size_bytes: 5_000_000,
            filename: "embedding-tokenizer.json".to_string(),
            is_archive: false,
        },
        // Speaker Segmentation model for diarization (pyannote ~5MB)
        ModelInfo {
            id: "speaker-segmentation".to_string(),
            name: "Speaker Segmentation (Pyannote)".to_string(),
            url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2".to_string(),
            size_bytes: 5_500_000,
            filename: "sherpa-onnx-pyannote-segmentation-3-0.tar.bz2".to_string(),
            is_archive: true,
        },
        // Speaker Embedding model for diarization (3D-Speaker ~26MB)
        ModelInfo {
            id: "speaker-embedding".to_string(),
            name: "Speaker Embedding (3D-Speaker)".to_string(),
            url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx".to_string(),
            size_bytes: 26_000_000,
            filename: "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx".to_string(),
            is_archive: false,
        },
    ]
}

/// Check if a model is installed
pub fn is_model_installed(model: &ModelInfo) -> bool {
    let models_dir = get_models_dir();

    if model.is_archive {
        // For archives, check for extracted files
        match model.id.as_str() {
            "sensevoice" => {
                // SenseVoice model files
                let sensevoice_dir = models_dir.join("sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17");
                (sensevoice_dir.join("model.onnx").exists() || sensevoice_dir.join("model.int8.onnx").exists()) &&
                sensevoice_dir.join("tokens.txt").exists()
            }
            "speaker-segmentation" => {
                // Pyannote segmentation model
                models_dir.join("sherpa-onnx-pyannote-segmentation-3-0").join("model.onnx").exists()
            }
            _ => false,
        }
    } else {
        models_dir.join(&model.filename).exists()
    }
}

/// Get status of all models
pub fn get_models_status() -> Vec<ModelStatus> {
    get_required_models()
        .into_iter()
        .map(|model| ModelStatus {
            id: model.id.clone(),
            name: model.name.clone(),
            installed: is_model_installed(&model),
            size_bytes: model.size_bytes,
        })
        .collect()
}

/// Check if all models are installed
pub fn all_models_installed() -> bool {
    get_required_models().iter().all(|m| is_model_installed(m))
}

/// Download a model with progress reporting
pub async fn download_model(
    app: AppHandle,
    model: ModelInfo,
) -> Result<(), String> {
    let client = Client::new();
    let models_dir = get_models_dir();

    // Start download
    let response = client
        .get(&model.url)
        .send()
        .await
        .map_err(|e| format!("Failed to start download: {}", e))?;

    let total_size = response.content_length().unwrap_or(model.size_bytes);

    // Emit initial progress
    let _ = app.emit("download-progress", DownloadProgress {
        model_id: model.id.clone(),
        model_name: model.name.clone(),
        downloaded_bytes: 0,
        total_bytes: total_size,
        progress_percent: 0.0,
        status: "downloading".to_string(),
    });

    // Download to temp file
    let temp_path = models_dir.join(format!("{}.tmp", model.filename));
    let mut file = std::fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Write error: {}", e))?;

        downloaded += chunk.len() as u64;
        let progress = (downloaded as f32 / total_size as f32) * 100.0;

        // Emit progress every ~1%
        if (progress as u32) % 1 == 0 {
            let _ = app.emit("download-progress", DownloadProgress {
                model_id: model.id.clone(),
                model_name: model.name.clone(),
                downloaded_bytes: downloaded,
                total_bytes: total_size,
                progress_percent: progress,
                status: "downloading".to_string(),
            });
        }
    }

    drop(file);

    // Handle archive extraction
    if model.is_archive {
        let _ = app.emit("download-progress", DownloadProgress {
            model_id: model.id.clone(),
            model_name: model.name.clone(),
            downloaded_bytes: total_size,
            total_bytes: total_size,
            progress_percent: 100.0,
            status: "extracting".to_string(),
        });

        extract_archive(&temp_path, &models_dir, &model)?;
        std::fs::remove_file(&temp_path).ok();
    } else {
        // Move temp file to final location
        let final_path = models_dir.join(&model.filename);
        std::fs::rename(&temp_path, &final_path)
            .map_err(|e| format!("Failed to move file: {}", e))?;
    }

    // Emit completion
    let _ = app.emit("download-progress", DownloadProgress {
        model_id: model.id.clone(),
        model_name: model.name.clone(),
        downloaded_bytes: total_size,
        total_bytes: total_size,
        progress_percent: 100.0,
        status: "complete".to_string(),
    });

    println!("Downloaded: {}", model.name);
    Ok(())
}

/// Extract tar.bz2 archive
fn extract_archive(
    archive_path: &PathBuf,
    dest_dir: &PathBuf,
    model: &ModelInfo,
) -> Result<(), String> {
    use std::process::Command;

    let archive_str = archive_path.to_str()
        .ok_or("Invalid archive path")?;
    let dest_str = dest_dir.to_str()
        .ok_or("Invalid destination path")?;

    println!("Extracting {} to {}", archive_str, dest_str);

    // Use system tar for bz2 - pass args separately to handle spaces in paths
    let output = Command::new("tar")
        .arg("-xjf")
        .arg(archive_str)
        .arg("-C")
        .arg(dest_str)
        .output()
        .map_err(|e| format!("Failed to run tar: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!("tar extraction failed: {} {}", stderr, stdout));
    }

    // Handle extracted directories based on model type
    match model.id.as_str() {
        "sensevoice" => {
            // SenseVoice extracts to sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17
            // Keep it in the subdirectory (don't move files)
            let extracted_dir = dest_dir.join("sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17");
            if extracted_dir.exists() {
                println!("SenseVoice extracted to: {:?}", extracted_dir);
            }
        }
        "speaker-segmentation" => {
            // Pyannote extracts to sherpa-onnx-pyannote-segmentation-3-0
            // Keep it in the subdirectory
            let extracted_dir = dest_dir.join("sherpa-onnx-pyannote-segmentation-3-0");
            if extracted_dir.exists() {
                println!("Pyannote segmentation extracted to: {:?}", extracted_dir);
            }
        }
        _ => {}
    }

    Ok(())
}

/// Download all missing models
pub async fn download_all_models(app: AppHandle) -> Result<(), String> {
    let models = get_required_models();

    for model in models {
        if !is_model_installed(&model) {
            download_model(app.clone(), model).await?;
        }
    }

    Ok(())
}
