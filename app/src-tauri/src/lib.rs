use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    ipc::Channel,
    Manager, Emitter,
};
// Use parking_lot for high-performance synchronization
// - RwLock for read-heavy engines (initialized once, read many times)
// - Mutex for write-heavy state (audio buffers, etc.)
use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc;

// ============================================================================
// Adaptive Audio Chunking Configuration
// ============================================================================

/// Configuration for adaptive audio chunking based on energy levels
#[derive(Clone)]
pub struct AdaptiveChunkConfig {
    /// Minimum chunk size in samples (during active speech) - ~50ms at 16kHz
    pub min_chunk_samples: usize,
    /// Maximum chunk size in samples (during silence) - ~250ms at 16kHz
    pub max_chunk_samples: usize,
    /// RMS threshold to detect speech activity (typical speech: 0.02-0.1)
    pub speech_threshold: f32,
    /// RMS threshold for definite silence (very quiet: < 0.005)
    pub silence_threshold: f32,
    /// Number of consecutive silent chunks before switching to large chunks
    pub silence_holdoff_chunks: u32,
    /// Minimum time between emissions in ms (to prevent too frequent updates)
    pub min_emit_interval_ms: u64,
}

impl Default for AdaptiveChunkConfig {
    fn default() -> Self {
        Self {
            min_chunk_samples: 800,     // ~50ms at 16kHz (responsive during speech)
            max_chunk_samples: 4000,    // ~250ms at 16kHz (efficient during silence)
            speech_threshold: 0.015,    // RMS level indicating speech
            silence_threshold: 0.003,   // RMS level indicating silence
            silence_holdoff_chunks: 3,  // Wait 3 silent chunks before switching
            min_emit_interval_ms: 40,   // At least 40ms between emissions
        }
    }
}

/// State for adaptive chunking
struct AdaptiveChunkState {
    /// Current target chunk size
    current_chunk_size: usize,
    /// Consecutive silent chunk count
    silent_chunk_count: u32,
    /// Is currently in speech mode
    in_speech: bool,
    /// Last emission time
    last_emit: std::time::Instant,
    /// Config
    config: AdaptiveChunkConfig,
}

impl AdaptiveChunkState {
    fn new(config: AdaptiveChunkConfig) -> Self {
        Self {
            current_chunk_size: config.min_chunk_samples, // Start responsive
            silent_chunk_count: 0,
            in_speech: false,
            last_emit: std::time::Instant::now(),
            config,
        }
    }

    /// Calculate RMS (Root Mean Square) energy of samples
    fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Update state based on current audio buffer and return whether to emit
    fn should_emit(&mut self, buffer: &[f32]) -> bool {
        let rms = Self::calculate_rms(buffer);
        let elapsed_ms = self.last_emit.elapsed().as_millis() as u64;

        // Determine speech state
        if rms > self.config.speech_threshold {
            // Definite speech detected
            self.in_speech = true;
            self.silent_chunk_count = 0;
            self.current_chunk_size = self.config.min_chunk_samples;
        } else if rms < self.config.silence_threshold {
            // Definite silence
            self.silent_chunk_count += 1;
            if self.silent_chunk_count >= self.config.silence_holdoff_chunks {
                self.in_speech = false;
                self.current_chunk_size = self.config.max_chunk_samples;
            }
        }
        // In between thresholds: maintain current state (hysteresis)

        // Decide whether to emit
        let should_emit = buffer.len() >= self.current_chunk_size
            && elapsed_ms >= self.config.min_emit_interval_ms;

        if should_emit {
            self.last_emit = std::time::Instant::now();
        }

        should_emit
    }
}

// ============================================================================
// Tauri Channel Events for Streaming
// ============================================================================

/// Transcription event sent via Tauri Channel (more efficient than emit)
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum TranscriptionEvent {
    /// New transcription segment
    #[serde(rename_all = "camelCase")]
    Transcription {
        text: String,
        source: String,
        timestamp_ms: u64,
        is_final: bool,
        language: String,
        emotion: String,
        audio_events: Vec<String>,
        is_turn_complete: bool,
        turn_confidence: f32,
    },
    /// Audio level update (for visualization)
    #[serde(rename_all = "camelCase")]
    AudioLevel {
        source: String,
        rms: f32,
        is_speech: bool,
    },
    /// Real-time AI suggestion
    #[serde(rename_all = "camelCase")]
    Suggestion {
        insight: Option<String>,
        question: Option<String>,
        related_info: Option<String>,
    },
    /// Recording status change
    #[serde(rename_all = "camelCase")]
    Status {
        recording: bool,
        message: String,
    },
}

mod audio;
mod asr;
mod chunker;
mod embeddings;
mod entities;
mod knowledge_base;
mod llm_agent;
mod models;
mod smart_turn;
mod speaker_diarization;
mod user_store;
mod web_crawler;
mod agent_queue;
mod agent_workers;
mod screenshot;

use audio::{AudioCapture, AudioSample, AudioSource, AudioCapabilities, AudioCaptureMode, check_audio_capabilities};
use asr::{AsrEngine, AsrConfig};
use embeddings::EmbeddingEngine;
use entities::{EntityEngine, Entity, ExtractionResult};
use knowledge_base::{KnowledgeBase, SearchResult, ActionItem, Decision, KnowledgeSource, KnowledgeSearchResult, Meeting, TranscriptSegment, Topic, Person, MeetingStats};
use llm_agent::{MeetingAssistant, RealtimeSuggestion, MeetingHighlights};
use models::{ModelStatus, get_models_status, all_models_installed, download_all_models, get_models_dir};
use smart_turn::{SmartTurnEngine, SmartTurnConfig};
use speaker_diarization::{SpeakerDiarizationEngine, SpeakerDiarizationConfig};
use user_store::{UserStore, UserSettings, Note, Integration, SavedSearch};
use web_crawler::{WebCrawler, SearchResult as WebSearchResult, CrawledPage};
use screenshot::{capture_screen, ScreenshotResult};
use agent_queue::{AgentQueue, QueueStats};
use std::sync::Arc;
// Note: We use parking_lot::RwLock (imported above) for sync access
// and tokio::sync::RwLock only for KnowledgeBase (async access)

// App state
// Uses parking_lot primitives for high-performance synchronization:
// - RwLock for engines (initialized once, read many times during processing)
// - Mutex for frequently-changing state (audio buffers, etc.)
pub struct AppState {
    pub is_recording: std::sync::atomic::AtomicBool,
    // Audio capture - Mutex (write-heavy, single writer)
    pub audio_capture: Mutex<AudioCapture>,
    pub audio_sender: Mutex<Option<mpsc::UnboundedSender<AudioSample>>>,
    // ML Engines - RwLock (initialized once, read-heavy during processing)
    pub asr_engine: RwLock<Option<AsrEngine>>,
    pub smart_turn_engine: RwLock<Option<SmartTurnEngine>>,
    pub entity_engine: RwLock<Option<Arc<EntityEngine>>>,
    pub embedding_engine: RwLock<Option<Arc<EmbeddingEngine>>>,
    pub diarization_engine: RwLock<Option<SpeakerDiarizationEngine>>,
    pub llm_assistant: RwLock<Option<Arc<MeetingAssistant>>>,
    // UserStore uses rusqlite::Connection which is not Sync, so it must use Mutex
    pub user_store: Mutex<Option<UserStore>>,
    // Knowledge base - already uses tokio::RwLock for async access
    pub knowledge_base: Arc<tokio::sync::RwLock<Option<KnowledgeBase>>>,
    // Frequently-changing state - Mutex (write-heavy)
    pub current_meeting_id: Mutex<Option<String>>,
    pub recording_start_time: Mutex<Option<u64>>,  // Timestamp when recording started
    pub mic_audio_buffer: Mutex<Vec<f32>>,     // Buffer microphone for diarization
    pub system_audio_buffer: Mutex<Vec<f32>>,  // Buffer system audio for diarization
    pub current_audio_chunk: Mutex<Vec<f32>>,  // Buffer for Smart Turn analysis
    pub recent_transcripts: Mutex<Vec<String>>,  // Recent transcripts for LLM suggestions (max 10)
    pub current_meeting_context: Mutex<Option<String>>,  // Context/agenda for current meeting
    pub transcription_channel: Mutex<Option<Channel<TranscriptionEvent>>>,  // Channel for streaming
    // Agent queue - RwLock (initialized once, submit is async)
    pub agent_queue: RwLock<Option<Arc<AgentQueue>>>,
    // Config - immutable after init
    pub adaptive_chunk_config: AdaptiveChunkConfig,
    // Worker pool handle for graceful shutdown
    pub worker_pool: Mutex<Option<Arc<tokio::sync::Mutex<Option<agent_queue::WorkerPool>>>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            is_recording: std::sync::atomic::AtomicBool::new(false),
            // Audio (Mutex - write heavy)
            audio_capture: Mutex::new(AudioCapture::new()),
            audio_sender: Mutex::new(None),
            // ML Engines (RwLock - read heavy after init)
            asr_engine: RwLock::new(None),
            smart_turn_engine: RwLock::new(None),
            entity_engine: RwLock::new(None),
            embedding_engine: RwLock::new(None),
            diarization_engine: RwLock::new(None),
            llm_assistant: RwLock::new(None),
            user_store: Mutex::new(None),
            // Knowledge base (tokio RwLock for async)
            knowledge_base: Arc::new(tokio::sync::RwLock::new(None)),
            // Frequently-changing state (Mutex)
            current_meeting_id: Mutex::new(None),
            recording_start_time: Mutex::new(None),
            mic_audio_buffer: Mutex::new(Vec::new()),      // Buffer for microphone diarization
            system_audio_buffer: Mutex::new(Vec::new()),   // Buffer for system audio diarization
            current_audio_chunk: Mutex::new(Vec::new()),
            recent_transcripts: Mutex::new(Vec::new()),
            current_meeting_context: Mutex::new(None),
            transcription_channel: Mutex::new(None),
            // Agent queue (RwLock)
            agent_queue: RwLock::new(None),
            // Config
            adaptive_chunk_config: AdaptiveChunkConfig::default(),
            // Worker pool
            worker_pool: Mutex::new(None),
        }
    }
}

// Initialize ASR engine (SenseVoice)
#[tauri::command]
fn initialize_asr(state: tauri::State<AppState>) -> Result<(), String> {
    let mut asr_guard = state.asr_engine.write();

    if asr_guard.is_some() {
        return Ok(()); // Already initialized
    }

    let config = AsrConfig::default();
    let mut engine = AsrEngine::new(config);
    engine.initialize()?;

    *asr_guard = Some(engine);
    println!("[ASR] SenseVoice engine initialized");
    Ok(())
}

// Initialize Smart Turn v3 engine
#[tauri::command]
fn initialize_smart_turn(state: tauri::State<AppState>) -> Result<(), String> {
    let mut turn_guard = state.smart_turn_engine.write();

    if turn_guard.is_some() {
        return Ok(()); // Already initialized
    }

    let config = SmartTurnConfig::default();
    let mut engine = SmartTurnEngine::new(config);

    let models_dir = get_models_dir();
    engine.initialize(&models_dir)?;

    *turn_guard = Some(engine);
    println!("[SmartTurn] v3 engine initialized");
    Ok(())
}

// Initialize Entity extraction engine
#[tauri::command]
fn initialize_entities(state: tauri::State<AppState>) -> Result<(), String> {
    let mut entity_guard = state.entity_engine.write();

    if entity_guard.is_some() {
        return Ok(()); // Already initialized
    }

    let models_dir = get_models_dir();
    let engine = EntityEngine::new(&models_dir)?;

    *entity_guard = Some(Arc::new(engine));
    println!("Entity extraction engine initialized");
    Ok(())
}

// Initialize Embedding engine
#[tauri::command]
fn initialize_embeddings(state: tauri::State<AppState>) -> Result<(), String> {
    let mut embed_guard = state.embedding_engine.write();

    if embed_guard.is_some() {
        return Ok(()); // Already initialized
    }

    let models_dir = get_models_dir();
    let engine = EmbeddingEngine::new(&models_dir)?;

    *embed_guard = Some(Arc::new(engine));
    println!("Embedding engine initialized");
    Ok(())
}

// Initialize Speaker Diarization engine
#[tauri::command]
fn initialize_diarization(state: tauri::State<AppState>) -> Result<(), String> {
    let mut diar_guard = state.diarization_engine.write();

    if diar_guard.is_some() {
        return Ok(()); // Already initialized
    }

    let config = SpeakerDiarizationConfig::default();
    let mut engine = SpeakerDiarizationEngine::new(config);

    // Try to initialize, but don't fail if models aren't downloaded yet
    match engine.initialize() {
        Ok(_) => {
            *diar_guard = Some(engine);
            println!("Speaker diarization engine initialized");
        }
        Err(e) => {
            println!("Speaker diarization not available (models may not be downloaded): {}", e);
            // Don't return error - diarization is optional
        }
    }

    Ok(())
}

// Initialize Knowledge Base (requires entities and embeddings first)
#[tauri::command]
async fn initialize_knowledge_base(state: tauri::State<'_, AppState>) -> Result<(), String> {
    {
        let kb_guard = state.knowledge_base.read().await;
        if kb_guard.is_some() {
            return Ok(()); // Already initialized
        }
    }

    let entity_engine = {
        let guard = state.entity_engine.read();
        guard.clone().ok_or("Entity engine not initialized. Call initialize_entities first.")?
    };

    let embedding_engine = {
        let guard = state.embedding_engine.read();
        guard.clone().ok_or("Embedding engine not initialized. Call initialize_embeddings first.")?
    };

    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("second-brain");

    std::fs::create_dir_all(&data_dir).ok();

    let kb = KnowledgeBase::new(&data_dir, embedding_engine, entity_engine).await?;

    // Auto-end any stale meetings (older than 1 hour without end_time)
    // This handles cases where app crashed or was closed without ending meetings
    match kb.auto_end_stale_meetings(1).await {
        Ok(count) if count > 0 => {
            println!("[Startup] Auto-ended {} stale meeting(s)", count);
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("[Startup] Warning: Failed to auto-end stale meetings: {}", e);
        }
    }

    {
        let mut kb_guard = state.knowledge_base.write().await;
        *kb_guard = Some(kb);
    }

    println!("Knowledge base initialized");
    Ok(())
}

// Extract entities from text
#[tauri::command]
fn extract_entities(
    state: tauri::State<AppState>,
    text: String,
    timestamp_ms: Option<u64>,
    source: Option<String>,
) -> Result<ExtractionResult, String> {
    let entity_guard = state.entity_engine.read();

    let engine = entity_guard.as_ref()
        .ok_or("Entity engine not initialized. Call initialize_entities first.")?;

    let ts = timestamp_ms.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    });

    let src = source.unwrap_or_else(|| "manual".to_string());

    engine.extract_with_metadata(&text, ts, &src)
}

// Extract entities from multiple texts (batch)
#[tauri::command]
fn extract_entities_batch(
    state: tauri::State<AppState>,
    texts: Vec<String>,
) -> Result<Vec<Vec<Entity>>, String> {
    let entity_guard = state.entity_engine.read();

    let engine = entity_guard.as_ref()
        .ok_or("Entity engine not initialized. Call initialize_entities first.")?;

    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    engine.extract_batch(&text_refs)
}

// Start a new meeting
#[tauri::command]
async fn start_meeting(
    state: tauri::State<'_, AppState>,
    title: String,
    participants: Vec<String>,
) -> Result<String, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    let meeting_id = kb.create_meeting(&title, participants).await?;
    println!("[MEETING] Created meeting with ID: {}", meeting_id);

    {
        let mut current = state.current_meeting_id.lock();
        *current = Some(meeting_id.clone());
        println!("[MEETING] Set current_meeting_id to: {:?}", *current);
    }

    println!("[MEETING] Started meeting: {} (ID: {})", title, meeting_id);
    Ok(meeting_id)
}

// End the current meeting
#[tauri::command]
async fn end_meeting(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    summary: Option<String>,
) -> Result<(), String> {
    // Emit recording-stopped event to close overlay window
    let _ = app.emit("recording-stopped", ());

    // Get and immediately clear meeting ID to prevent race conditions
    let meeting_id = {
        let mut current = state.current_meeting_id.lock();
        let id = current.clone().ok_or("No meeting in progress")?;
        *current = None; // Clear immediately to prevent duplicate calls
        id
    };

    // Get recording start time for timestamp alignment
    let recording_start_time = {
        let mut start_time_guard = state.recording_start_time.lock();
        let start = start_time_guard.take();
        start
    };

    // Check audio capture mode to determine diarization strategy
    let audio_caps = check_audio_capabilities();
    let is_combined_mode = audio_caps.capture_mode == AudioCaptureMode::Combined;

    // Run speaker diarization based on audio capture mode
    let diarization_results = {
        let mic_audio = {
            let mut buffer = state.mic_audio_buffer.lock();
            let audio = buffer.clone();
            buffer.clear();
            audio
        };
        let system_audio = {
            let mut buffer = state.system_audio_buffer.lock();
            let audio = buffer.clone();
            buffer.clear();
            audio
        };

        // Determine which audio to diarize based on mode
        let (audio_to_diarize, mode_description) = if is_combined_mode {
            // Combined mode: mic contains BOTH user and system audio
            // We need to diarize everything to identify speakers
            println!("[Diarization] Combined audio mode detected - diarizing all {} mic samples", mic_audio.len());
            (mic_audio, "combined (mic + system)")
        } else if !system_audio.is_empty() {
            // Separate mode: system audio contains remote participants
            // Mic audio is the user (stays as "You")
            if !mic_audio.is_empty() {
                println!("[Diarization] Separate mode - {} mic samples (user=You), {} system samples to diarize", mic_audio.len(), system_audio.len());
            }
            (system_audio, "system only (remote participants)")
        } else if !mic_audio.is_empty() {
            // No system audio but we have mic audio
            // Might be in-person meeting or combined device not detected
            println!("[Diarization] Only mic audio available ({} samples) - will diarize to identify speakers", mic_audio.len());
            (mic_audio, "mic only (checking for multiple speakers)")
        } else {
            println!("[Diarization] No audio to process");
            (Vec::new(), "none")
        };

        if !audio_to_diarize.is_empty() {
            println!("[Diarization] Processing {} samples from {} source...", audio_to_diarize.len(), mode_description);
            let mut diar_guard = state.diarization_engine.write();
            if let Some(ref mut diar_engine) = *diar_guard {
                match diar_engine.process(audio_to_diarize, 16000) {
                    Ok(segments) => {
                        let speaker_count = segments.iter()
                            .map(|s| s.speaker_id)
                            .collect::<std::collections::HashSet<_>>()
                            .len();
                        println!("[Diarization] Found {} segments from {} unique speakers", segments.len(), speaker_count);

                        // Convert diarization timestamps to wall clock
                        let labeled_segments: Vec<_> = if let Some(start_ts) = recording_start_time {
                            segments.into_iter().map(|mut seg| {
                                seg.start_ms += start_ts;
                                seg.end_ms += start_ts;
                                seg
                            }).collect()
                        } else {
                            segments
                        };

                        Some((labeled_segments, is_combined_mode))
                    }
                    Err(e) => {
                        eprintln!("[Diarization] Error processing audio: {}", e);
                        None
                    }
                }
            } else {
                println!("[Diarization] Engine not initialized - speaker identification unavailable");
                println!("[Diarization] Check if 'speaker-segmentation' and 'speaker-embedding' models are downloaded");
                None
            }
        } else {
            None
        }
    };

    // Apply diarization results to knowledge base
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    if let Some((ref segments, combined_mode)) = diarization_results {
        let diar_tuples: Vec<(u64, u64, i32, String)> = segments
            .iter()
            .map(|s| (s.start_ms, s.end_ms, s.speaker_id, s.speaker_label.clone()))
            .collect();

        if combined_mode {
            // Combined mode: relabel ALL segments since we can't distinguish user from others by source
            match kb.relabel_all_speakers(&meeting_id, &diar_tuples).await {
                Ok(count) => println!("[Diarization] Relabeled {} segments (combined mode)", count),
                Err(e) => eprintln!("[Diarization] Relabeling failed: {}", e),
            }
        } else {
            // Separate mode: only relabel "Guest" segments, keep "You" as is
            match kb.relabel_speakers(&meeting_id, &diar_tuples).await {
                Ok(count) => println!("[Diarization] Relabeled {} 'Guest' segments to unique speakers", count),
                Err(e) => eprintln!("[Diarization] Relabeling failed: {}", e),
            }
        }
    }

    kb.end_meeting(&meeting_id, summary).await?;

    // Clear meeting context
    {
        let mut context = state.current_meeting_context.lock();
        *context = None;
    }

    println!("[Meeting] Ended meeting: {}", meeting_id);
    Ok(())
}

// Add transcript segment to current meeting
#[tauri::command]
async fn add_transcript_segment(
    state: tauri::State<'_, AppState>,
    speaker: String,
    text: String,
    start_ms: u64,
    end_ms: u64,
) -> Result<String, String> {
    let meeting_id = {
        let current = state.current_meeting_id.lock();
        current.clone().ok_or("No meeting in progress")?
    };

    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.add_segment(&meeting_id, &speaker, &text, start_ms, end_ms).await
}

// Search knowledge base
#[tauri::command]
async fn search_knowledge(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.search_similar(&query, limit.unwrap_or(10)).await
}

// Get open action items
#[tauri::command]
async fn get_action_items(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ActionItem>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_open_actions().await
}

// Get recent decisions
#[tauri::command]
async fn get_decisions(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<Decision>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_recent_decisions(limit.unwrap_or(10)).await
}

// ==================== Meeting Query Commands ====================

// Get all meetings
#[tauri::command]
async fn get_meetings(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<Meeting>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_meetings(limit).await
}

// Get a single meeting by ID
#[tauri::command]
async fn get_meeting(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<Meeting>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_meeting(&meeting_id).await
}

// Get transcript segments for a meeting
#[tauri::command]
async fn get_meeting_segments(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<TranscriptSegment>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_meeting_segments(&meeting_id).await
}

// Get action items for a meeting
#[tauri::command]
async fn get_meeting_action_items(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<ActionItem>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_meeting_action_items(&meeting_id).await
}

// Get decisions for a meeting
#[tauri::command]
async fn get_meeting_decisions(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<Decision>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_meeting_decisions(&meeting_id).await
}

// Get topics discussed in a meeting
#[tauri::command]
async fn get_meeting_topics(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<Topic>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_meeting_topics(&meeting_id).await
}

// Get people mentioned in a meeting
#[tauri::command]
async fn get_meeting_people(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<Person>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_meeting_people(&meeting_id).await
}

// Get meeting statistics
#[tauri::command]
async fn get_meeting_stats(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<MeetingStats, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_meeting_stats(&meeting_id).await
}

// Delete a meeting and all associated data
#[tauri::command]
async fn delete_meeting(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<(), String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.delete_meeting(&meeting_id).await
}

// Get ALL action items across all meetings
#[tauri::command]
async fn get_all_action_items(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_all_action_items(limit.unwrap_or(50)).await
}

// Get ALL decisions across all meetings
#[tauri::command]
async fn get_all_decisions(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<serde_json::Value>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_all_decisions(limit.unwrap_or(20)).await
}

// Get overall knowledge base statistics
#[tauri::command]
async fn get_knowledge_stats(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.get_global_stats().await
}

// Update action item status
#[tauri::command]
async fn update_action_item_status(
    state: tauri::State<'_, AppState>,
    action_id: String,
    status: String,
) -> Result<(), String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    kb.update_action_item_status(&action_id, &status).await
}

// Get current meeting ID
#[tauri::command]
fn get_current_meeting_id(state: tauri::State<AppState>) -> Option<String> {
    state.current_meeting_id.lock().clone()
}

// Initialize LLM Assistant
#[tauri::command]
fn initialize_llm(
    state: tauri::State<AppState>,
    api_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
) -> Result<(), String> {
    let mut llm_guard = state.llm_assistant.write();

    // Get settings from user store
    let (stored_url, stored_model, stored_api_key) = {
        let store_guard = state.user_store.lock();
        if let Some(ref store) = *store_guard {
            if let Ok(settings) = store.get_settings() {
                (settings.llm_url.clone(), settings.llm_model.clone(), settings.llm_api_key.clone())
            } else {
                (String::new(), String::new(), String::new())
            }
        } else {
            (String::new(), String::new(), String::new())
        }
    };

    // Get URL from param or user settings
    let url = match api_url {
        Some(u) if !u.trim().is_empty() => u,
        _ => {
            if !stored_url.trim().is_empty() {
                stored_url
            } else {
                return Err("LLM URL not configured. Please configure in settings.".to_string());
            }
        }
    };

    // Get model from param or user settings
    let model_name = match model {
        Some(m) if !m.trim().is_empty() => m,
        _ => {
            if !stored_model.trim().is_empty() {
                stored_model
            } else {
                "default".to_string()
            }
        }
    };

    // Get API key from param or user settings
    let key = match api_key {
        Some(k) => k,
        _ => stored_api_key,
    };

    // Re-initialize even if already initialized (allows changing settings)
    let assistant = Arc::new(MeetingAssistant::new(&url, &model_name, &key));
    *llm_guard = Some(assistant);

    println!("LLM assistant initialized with URL: {} and model: {}", url, model_name);
    Ok(())
}

// Ask the LLM assistant a question
#[tauri::command]
async fn ask_assistant(
    state: tauri::State<'_, AppState>,
    question: String,
) -> Result<String, String> {
    let assistant = {
        let guard = state.llm_assistant.read();
        guard.as_ref()
            .ok_or("LLM assistant not initialized. Call initialize_llm first.")?
            .clone()
    };

    let kb = state.knowledge_base.clone();
    assistant.ask(&question, kb).await
}

// Summarize a meeting
#[tauri::command]
async fn summarize_meeting(
    state: tauri::State<'_, AppState>,
    segments: Vec<String>,
) -> Result<String, String> {
    let assistant = {
        let guard = state.llm_assistant.read();
        guard.as_ref()
            .ok_or("LLM assistant not initialized")?
            .clone()
    };

    assistant.summarize_meeting(&segments).await
}

// Get suggested questions
#[tauri::command]
async fn suggest_questions(
    state: tauri::State<'_, AppState>,
    current_topic: String,
) -> Result<Vec<String>, String> {
    let assistant = {
        let guard = state.llm_assistant.read();
        guard.as_ref()
            .ok_or("LLM assistant not initialized")?
            .clone()
    };

    let kb = state.knowledge_base.clone();
    assistant.suggest_questions(&current_topic, kb).await
}

// Ask a question about a specific meeting
#[tauri::command]
async fn ask_meeting_question(
    state: tauri::State<'_, AppState>,
    question: String,
    meeting_title: String,
    transcript: Vec<String>,
    action_items: Vec<String>,
    decisions: Vec<String>,
) -> Result<String, String> {
    let assistant = {
        let guard = state.llm_assistant.read();
        guard.as_ref()
            .ok_or("LLM assistant not initialized. Call initialize_llm first.")?
            .clone()
    };

    assistant.ask_about_meeting(&question, &meeting_title, &transcript, &action_items, &decisions).await
}

// Get real-time suggestions based on recent transcript
#[tauri::command]
async fn get_realtime_suggestions(
    state: tauri::State<'_, AppState>,
    meeting_context: Option<String>,
) -> Result<RealtimeSuggestion, String> {
    let assistant = {
        let guard = state.llm_assistant.read();
        guard.as_ref()
            .ok_or("LLM assistant not initialized")?
            .clone()
    };

    let recent = {
        let guard = state.recent_transcripts.lock();
        guard.clone()
    };

    if recent.is_empty() {
        return Ok(RealtimeSuggestion::default());
    }

    let kb = state.knowledge_base.clone();
    assistant.generate_realtime_suggestions(&recent, meeting_context.as_deref(), kb).await
}

// Clear recent transcripts (call when meeting ends)
#[tauri::command]
fn clear_recent_transcripts(state: tauri::State<AppState>) {
    let mut guard = state.recent_transcripts.lock();
    guard.clear();
}

// Set meeting context (agenda, notes, linked doc summaries)
#[tauri::command]
fn set_meeting_context(state: tauri::State<AppState>, context: Option<String>) {
    let mut guard = state.current_meeting_context.lock();
    *guard = context;
    println!("[Meeting] Context set: {} chars", guard.as_ref().map(|c| c.len()).unwrap_or(0));
}

// Get meeting context
#[tauri::command]
fn get_meeting_context(state: tauri::State<AppState>) -> Option<String> {
    let guard = state.current_meeting_context.lock();
    guard.clone()
}

// Initialize agent queue with background worker pool
#[tauri::command]
fn initialize_agent_queue(
    state: tauri::State<AppState>,
    num_workers: Option<usize>,
) -> Result<(), String> {
    // Check if already initialized
    {
        let queue_guard = state.agent_queue.read();
        if queue_guard.is_some() {
            return Ok(()); // Already initialized
        }
    }

    // Get dependencies for workers
    let llm = {
        let guard = state.llm_assistant.read();
        guard.clone()
    };
    // Note: Entity engine requires type refactoring to work with workers
    // Currently uses Option<Arc<EntityEngine>> but workers need Arc<RwLock<Option<EntityEngine>>>
    // TODO: Refactor entity engine storage for worker compatibility
    let entity_engine = None::<Arc<parking_lot::RwLock<Option<EntityEngine>>>>;
    let kb = Some(state.knowledge_base.clone());

    // Create queue and get receiver
    let (queue, job_rx) = AgentQueue::new(100);
    let queue = Arc::new(queue);
    let queue_stats = Arc::new(tokio::sync::RwLock::new(QueueStats::default()));

    // Create worker dependencies
    let deps = agent_workers::WorkerDependencies {
        llm,
        kb,
        entity_engine,
    };

    // Determine worker count (default to CPU count / 2, min 2, max 8)
    let worker_count = num_workers.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|p| (p.get() / 2).clamp(2, 8))
            .unwrap_or(2)
    });

    // Start worker pool in a separate thread with its own tokio runtime
    let job_rx_arc = Arc::new(tokio::sync::Mutex::new(job_rx));
    let queue_stats_clone = queue_stats.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(worker_count)
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for agent workers");

        rt.block_on(async move {
            println!("[AgentQueue] Starting {} workers", worker_count);

            // Create worker tasks
            let mut handles = Vec::with_capacity(worker_count);

            for worker_id in 0..worker_count {
                let rx = job_rx_arc.clone();
                let stats = queue_stats_clone.clone();
                let worker_deps = deps.clone();

                let handle = tokio::spawn(async move {
                    println!("[Worker-{}] Started", worker_id);

                    loop {
                        let job = {
                            let mut rx_guard = rx.lock().await;
                            rx_guard.recv().await
                        };

                        match job {
                            Some(agent_queue::AgentJob::Shutdown) => {
                                println!("[Worker-{}] Received shutdown signal", worker_id);
                                break;
                            }
                            Some(job) => {
                                // Update active workers count
                                {
                                    let mut s = stats.write().await;
                                    s.workers_active += 1;
                                }

                                // Process the job using spawn_blocking for CPU-intensive work
                                let stats_clone = stats.clone();
                                let deps_clone = worker_deps.clone();

                                tokio::task::spawn_blocking(move || {
                                    // Create a runtime for async operations within the blocking task
                                    let rt = tokio::runtime::Handle::current();
                                    rt.block_on(async {
                                        agent_workers::process_agent_job(
                                            job,
                                            stats_clone,
                                            deps_clone.llm,
                                            deps_clone.kb,
                                            deps_clone.entity_engine,
                                        ).await;
                                    });
                                }).await.ok();

                                // Update active workers count
                                {
                                    let mut s = stats.write().await;
                                    s.workers_active = s.workers_active.saturating_sub(1);
                                }
                            }
                            None => {
                                println!("[Worker-{}] Channel closed, shutting down", worker_id);
                                break;
                            }
                        }
                    }

                    println!("[Worker-{}] Stopped", worker_id);
                });

                handles.push(handle);
            }

            // Wait for all workers to complete
            for handle in handles {
                let _ = handle.await;
            }

            println!("[AgentQueue] All workers stopped");
        });
    });

    // Store the queue
    {
        let mut queue_guard = state.agent_queue.write();
        *queue_guard = Some(queue);
    }

    println!("[AgentQueue] Initialized with {} background workers", worker_count);
    Ok(())
}

// Get queue statistics
#[tauri::command]
async fn get_queue_stats(state: tauri::State<'_, AppState>) -> Result<QueueStats, String> {
    let queue = {
        let queue_guard = state.agent_queue.read();
        queue_guard.clone()
    };
    match queue {
        Some(q) => Ok(q.get_stats().await),
        None => Ok(QueueStats::default()),
    }
}

// Submit a question to the agent queue (async processing)
// Note: For now, processes inline since workers need complex async setup
#[tauri::command]
async fn queue_ask_question(
    state: tauri::State<'_, AppState>,
    question: String,
    context: Option<String>,
) -> Result<agent_queue::AnswerResult, String> {
    // Process inline - use LLM directly
    let assistant = {
        let guard = state.llm_assistant.read();
        guard.clone().ok_or("LLM not initialized")?
    };

    let kb = state.knowledge_base.clone();

    let full_context = match context {
        Some(ctx) => format!("Context: {}\n\nQuestion: {}", ctx, question),
        None => question.clone(),
    };

    match assistant.ask(&full_context, kb).await {
        Ok(answer) => Ok(agent_queue::AnswerResult {
            answer,
            sources: vec![],
            error: None,
        }),
        Err(e) => Ok(agent_queue::AnswerResult {
            answer: String::new(),
            sources: vec![],
            error: Some(e),
        }),
    }
}

// Submit realtime suggestions request to queue
// Note: Processes inline for now
#[tauri::command]
async fn queue_realtime_suggestions(
    state: tauri::State<'_, AppState>,
    meeting_context: Option<String>,
) -> Result<agent_queue::RealtimeSuggestionResult, String> {
    let assistant = {
        let guard = state.llm_assistant.read();
        guard.clone().ok_or("LLM not initialized")?
    };

    let recent_transcripts = state.recent_transcripts.lock().clone();

    if recent_transcripts.is_empty() {
        return Ok(agent_queue::RealtimeSuggestionResult::default());
    }

    let kb = state.knowledge_base.clone();

    match assistant.generate_realtime_suggestions(&recent_transcripts, meeting_context.as_deref(), kb).await {
        Ok(suggestion) => Ok(agent_queue::RealtimeSuggestionResult {
            insight: suggestion.insight,
            question: suggestion.question,
            related_info: suggestion.related_info,
            error: None,
        }),
        Err(e) => Ok(agent_queue::RealtimeSuggestionResult {
            error: Some(e),
            ..Default::default()
        }),
    }
}

// Submit post-meeting highlights extraction to queue
// Note: Processes inline for now
#[tauri::command]
async fn queue_meeting_highlights(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<agent_queue::HighlightsResult, String> {
    let assistant = {
        let guard = state.llm_assistant.read();
        guard.clone().ok_or("LLM not initialized")?
    };

    // Get meeting segments
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    let segments = kb.get_meeting_segments(&meeting_id).await
        .map_err(|e| format!("Failed to get segments: {}", e))?;

    if segments.is_empty() {
        return Ok(agent_queue::HighlightsResult {
            error: Some("No segments found".to_string()),
            ..Default::default()
        });
    }

    // Format transcript
    let formatted: Vec<String> = segments
        .iter()
        .map(|s| format!("{}: {}", s.speaker, s.text))
        .collect();

    let meeting = kb.get_meeting(&meeting_id).await
        .map_err(|e| format!("Failed to get meeting: {}", e))?
        .ok_or("Meeting not found")?;

    drop(kb_guard); // Release lock before LLM call

    // Process with LLM
    match assistant.process_meeting_end(&formatted, &meeting.title).await {
        Ok(highlights) => Ok(agent_queue::HighlightsResult {
            summary: highlights.summary,
            key_topics: highlights.key_topics,
            action_items: highlights.action_items.into_iter().map(|a| agent_queue::ActionItemResult {
                task: a.task,
                assignee: a.assignee,
                deadline: a.deadline,
            }).collect(),
            decisions: highlights.decisions,
            highlights: highlights.highlights,
            follow_ups: highlights.follow_ups,
            error: None,
        }),
        Err(e) => Ok(agent_queue::HighlightsResult {
            error: Some(e),
            ..Default::default()
        }),
    }
}

// Submit entity extraction to queue
// Note: Processes inline using Arc<EntityEngine>
#[tauri::command]
async fn queue_entity_extraction(
    state: tauri::State<'_, AppState>,
    text: String,
    _source: String,
) -> Result<agent_queue::EntityResult, String> {
    let guard = state.entity_engine.read();
    let entity_engine = guard.as_ref()
        .ok_or("Entity engine not initialized")?;

    match entity_engine.extract_with_relations(&text) {
        Ok((entities, relationships)) => Ok(agent_queue::EntityResult {
            entities: entities.into_iter().map(|e| agent_queue::ExtractedEntity {
                text: e.text,
                label: e.label,
                confidence: e.confidence,
            }).collect(),
            relationships: relationships.into_iter().map(|r| agent_queue::ExtractedRelationship {
                source: r.source,
                relation: r.relation,
                target: r.target,
                confidence: r.confidence,
            }).collect(),
            error: None,
        }),
        Err(e) => Ok(agent_queue::EntityResult {
            error: Some(e),
            ..Default::default()
        }),
    }
}

// Process meeting after it ends - extract highlights via LLM
#[tauri::command]
async fn process_meeting_highlights(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<MeetingHighlights, String> {
    println!("[Highlights] Starting post-meeting processing for: {}", meeting_id);
    let start = std::time::Instant::now();

    let assistant = {
        let guard = state.llm_assistant.read();
        guard.as_ref()
            .ok_or("LLM assistant not initialized")?
            .clone()
    };

    // Get meeting and segments
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref()
        .ok_or("Knowledge base not initialized")?;

    let meeting = kb.get_meeting(&meeting_id).await?
        .ok_or("Meeting not found")?;
    println!("[Highlights] Found meeting: {}", meeting.title);

    let segments = kb.get_meeting_segments(&meeting_id).await?;
    println!("[Highlights] Found {} transcript segments", segments.len());

    if segments.is_empty() {
        println!("[Highlights] No segments found, returning empty highlights");
        return Ok(MeetingHighlights::default());
    }

    // Format segments for LLM
    let formatted: Vec<String> = segments
        .iter()
        .map(|s| format!("{}: {}", s.speaker, s.text))
        .collect();

    // Process with LLM
    let highlights = assistant.process_meeting_end(&formatted, &meeting.title).await?;

    // Store extracted action items and decisions in KB
    for action in &highlights.action_items {
        let _ = kb.add_action_item(
            &meeting_id,
            &action.task,
            action.assignee.as_deref(),
            action.deadline.as_deref(),
        ).await;
    }

    for decision in &highlights.decisions {
        let _ = kb.add_decision(&meeting_id, decision).await;
    }

    // Update meeting summary if we got one
    if let Some(ref summary) = highlights.summary {
        let _ = kb.update_meeting_summary(&meeting_id, summary).await;
    }

    println!("[Highlights] Post-processing complete in {:?}: {} action items, {} decisions, {} key topics, summary: {}",
        start.elapsed(),
        highlights.action_items.len(),
        highlights.decisions.len(),
        highlights.key_topics.len(),
        highlights.summary.is_some());

    Ok(highlights)
}

// Commands

/// Subscribe to transcription events via Tauri Channel (more efficient than emit)
/// Call this before start_recording to receive events via the channel
#[tauri::command]
fn subscribe_transcription(
    state: tauri::State<AppState>,
    on_event: Channel<TranscriptionEvent>,
) -> Result<(), String> {
    let mut channel_guard = state.transcription_channel.lock();
    *channel_guard = Some(on_event);
    println!("[Channel] Transcription channel subscribed");
    Ok(())
}

/// Unsubscribe from transcription channel
#[tauri::command]
fn unsubscribe_transcription(state: tauri::State<AppState>) -> Result<(), String> {
    let mut channel_guard = state.transcription_channel.lock();
    *channel_guard = None;
    println!("[Channel] Transcription channel unsubscribed");
    Ok(())
}

#[tauri::command]
fn start_recording(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<(), String> {
    if state.is_recording.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }

    // Track when recording started (for timestamp alignment with diarization)
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    *state.recording_start_time.lock() = Some(start_time);

    // Create channel for audio samples
    let (tokio_tx, mut tokio_rx) = mpsc::unbounded_channel::<AudioSample>();
    *state.audio_sender.lock() = Some(tokio_tx.clone());

    // Start audio capture
    let mut capture = state.audio_capture.lock();
    capture.start(tokio_tx)?;

    state.is_recording.store(true, std::sync::atomic::Ordering::SeqCst);

    // Channel for ASR processing
    let (asr_tx, asr_rx) = std::sync::mpsc::channel::<(Vec<f32>, u32, String)>();

    // Spawn thread to bridge tokio channel to std channel and process audio
    let app_handle = app.clone();
    let asr_tx_clone = asr_tx.clone();
    std::thread::spawn(move || {
        // Create a small tokio runtime just for receiving from the channel
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async move {
            let mut mic_buffer: Vec<f32> = Vec::with_capacity(16000);
            let mut system_buffer: Vec<f32> = Vec::with_capacity(16000);
            let mut mic_channels: u16 = 1;
            let mut system_channels: u16 = 1;

            // Adaptive chunking state for each audio source
            let adaptive_config = AdaptiveChunkConfig::default();
            let mut mic_chunk_state = AdaptiveChunkState::new(adaptive_config.clone());
            let mut system_chunk_state = AdaptiveChunkState::new(adaptive_config);

            // Audio level emission throttle (send at most every 100ms for visualization)
            let mut last_level_emit = std::time::Instant::now();

            // Helper to convert stereo to mono
            fn stereo_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
                if channels <= 1 {
                    return samples.to_vec();
                }
                // Average all channels together
                let channels = channels as usize;
                let num_frames = samples.len() / channels;
                let mut mono = Vec::with_capacity(num_frames);
                for frame in 0..num_frames {
                    let mut sum = 0.0f32;
                    for ch in 0..channels {
                        sum += samples[frame * channels + ch];
                    }
                    mono.push(sum / channels as f32);
                }
                mono
            }

            while let Some(sample) = tokio_rx.recv().await {
                let source_str = match sample.source {
                    AudioSource::Microphone => "microphone",
                    AudioSource::SystemAudio => "system",
                };

                // Track channel count and add to appropriate buffer
                // Note: We store raw data and convert to mono before sending to ASR
                match sample.source {
                    AudioSource::Microphone => {
                        mic_channels = sample.channels;
                        mic_buffer.extend_from_slice(&sample.data);
                    }
                    AudioSource::SystemAudio => {
                        system_channels = sample.channels;
                        system_buffer.extend_from_slice(&sample.data);
                    }
                }

                // ============================================================
                // ADAPTIVE CHUNKING: Use energy-based chunk sizing
                // - During speech: smaller chunks (50ms) for responsiveness
                // - During silence: larger chunks (250ms) for efficiency
                // ============================================================

                // Process microphone with adaptive chunking
                if !mic_buffer.is_empty() {
                    let mono_samples = stereo_to_mono(&mic_buffer, mic_channels);
                    if mic_chunk_state.should_emit(&mono_samples) {
                        let _ = asr_tx_clone.send((mono_samples, sample.sample_rate, "microphone".to_string()));
                        mic_buffer.clear();
                    }
                }

                // Process system audio with adaptive chunking
                if !system_buffer.is_empty() {
                    let mono_samples = stereo_to_mono(&system_buffer, system_channels);
                    if system_chunk_state.should_emit(&mono_samples) {
                        let _ = asr_tx_clone.send((mono_samples, sample.sample_rate, "system".to_string()));
                        system_buffer.clear();
                    }
                }

                // Emit audio level updates for visualization (throttled)
                if last_level_emit.elapsed().as_millis() >= 100 {
                    let mic_rms = AdaptiveChunkState::calculate_rms(&mic_buffer);
                    let system_rms = AdaptiveChunkState::calculate_rms(&system_buffer);

                    // Emit via traditional event (for backward compatibility)
                    let _ = app_handle.emit("audio-sample", serde_json::json!({
                        "source": source_str,
                        "timestamp_ms": sample.timestamp_ms,
                        "sample_count": sample.data.len(),
                        "sample_rate": sample.sample_rate,
                        "mic_rms": mic_rms,
                        "system_rms": system_rms,
                        "mic_speech": mic_chunk_state.in_speech,
                        "system_speech": system_chunk_state.in_speech,
                    }));

                    last_level_emit = std::time::Instant::now();
                }
            }
        });
    });

    // Spawn ASR processing thread
    let app_handle2 = app.clone();
    std::thread::spawn(move || {
        // Create a tokio runtime for async KB operations
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for ASR");

        let mut sample_count = 0u64;
        let mut mic_chunk_count = 0u64;
        let mut system_chunk_count = 0u64;
        while let Ok((samples, sample_rate, source)) = asr_rx.recv() {
            sample_count += 1;

            // Calculate RMS level for debugging
            let rms: f32 = if !samples.is_empty() {
                (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
            } else {
                0.0
            };

            if source == "microphone" {
                mic_chunk_count += 1;
            } else {
                system_chunk_count += 1;
                // Log more frequently for system audio to debug
                if system_chunk_count % 20 == 0 || system_chunk_count <= 5 {
                    println!("[ASR] SYSTEM audio chunk #{}: {} samples at {}Hz, RMS={:.6} ({}dB)",
                        system_chunk_count, samples.len(), sample_rate, rms,
                        if rms > 0.0 { (20.0 * rms.log10()) as i32 } else { -100 });
                }
            }

            if sample_count % 100 == 0 {
                println!("[ASR] Stats: {} total chunks (mic: {}, system: {})",
                    sample_count, mic_chunk_count, system_chunk_count);
            }

            // Get state from app handle inside the thread
            let state: tauri::State<AppState> = app_handle2.state();

            // Buffer ALL audio for post-meeting diarization (before ASR processing)
            // This allows speaker identification across all audio sources
            if source == "microphone" {
                let mut buffer = state.mic_audio_buffer.lock();
                buffer.extend_from_slice(&samples);
            } else {
                let mut buffer = state.system_audio_buffer.lock();
                buffer.extend_from_slice(&samples);
            }

            let mut asr_guard = state.asr_engine.write();
            if let Some(ref mut engine) = *asr_guard {
                let result = if source == "microphone" {
                    engine.process_microphone(&samples, sample_rate)
                } else {
                    engine.process_system(&samples, sample_rate)
                };

                if let Some(mut transcription) = result {
                    // Run Smart Turn analysis on the audio chunk
                    let turn_guard = state.smart_turn_engine.read();
                    if let Some(ref turn_engine) = *turn_guard {
                        if let Ok(turn_result) = turn_engine.predict(&samples) {
                            transcription.is_turn_complete = turn_result.is_complete;
                            transcription.turn_confidence = turn_result.probability;
                        }
                    }
                    drop(turn_guard);

                    // Format emotion and events for logging
                    let emotion_str = format!("{:?}", transcription.emotion);
                    let events_str: Vec<String> = transcription.audio_events.iter()
                        .map(|e| format!("{:?}", e)).collect();

                    // Verify source is correctly set
                    if source != transcription.source {
                        eprintln!("[ASR] WARNING: source mismatch! input='{}' but transcription.source='{}'",
                            source, transcription.source);
                    }

                    println!("[ASR] TRANSCRIPTION: \"{}\" (source: {}, lang: {}, emotion: {}, turn_done: {} ({:.2}))",
                        transcription.text, transcription.source, transcription.language,
                        emotion_str, transcription.is_turn_complete, transcription.turn_confidence);

                    // Create TranscriptionEvent for channel streaming
                    let event = TranscriptionEvent::Transcription {
                        text: transcription.text.clone(),
                        source: transcription.source.clone(),
                        timestamp_ms: transcription.timestamp_ms,
                        is_final: transcription.is_final,
                        language: transcription.language.clone(),
                        emotion: emotion_str.clone(),
                        audio_events: events_str.clone(),
                        is_turn_complete: transcription.is_turn_complete,
                        turn_confidence: transcription.turn_confidence,
                    };

                    // Send via Channel if subscribed
                    let channel_result = {
                        let channel_guard = state.transcription_channel.lock();
                        if let Some(ref channel) = *channel_guard {
                            match channel.send(event.clone()) {
                                Ok(_) => {
                                    println!("[Channel] Sent transcription event");
                                    Some(true)
                                }
                                Err(e) => {
                                    eprintln!("[Channel] Failed to send: {:?}", e);
                                    Some(false)
                                }
                            }
                        } else {
                            None // No channel subscribed
                        }
                    };

                    // ALWAYS emit for backward compatibility (emit is reliable)
                    // Channel is an optimization, not a replacement
                    let _ = app_handle2.emit("transcription", serde_json::json!({
                        "text": transcription.text,
                        "source": transcription.source,
                        "timestamp_ms": transcription.timestamp_ms,
                        "is_final": transcription.is_final,
                        "language": transcription.language,
                        "emotion": emotion_str,
                        "audio_events": events_str,
                        "is_turn_complete": transcription.is_turn_complete,
                        "turn_confidence": transcription.turn_confidence,
                    }));

                    if channel_result.is_none() {
                        println!("[Transcription] Sent via emit (no channel subscribed)");
                    }

                    // Track recent transcripts for LLM suggestions
                    if transcription.is_final && !transcription.text.trim().is_empty() {
                        let speaker = if source == "microphone" { "You" } else { "Guest" };
                        let formatted = format!("{}: {}", speaker, transcription.text);

                        let should_generate_suggestions = {
                            let mut recent = state.recent_transcripts.lock();
                            recent.push(formatted);
                            // Keep only last 10 transcripts
                            if recent.len() > 10 {
                                recent.remove(0);
                            }
                            // Generate suggestions:
                            // - On FIRST transcript (instant feedback)
                            // - When turn completes (natural conversation break)
                            // - Every 3 transcripts (more responsive than 5)
                            recent.len() == 1 || transcription.is_turn_complete || recent.len() % 3 == 0
                        };

                        // Generate and emit real-time suggestions asynchronously
                        if should_generate_suggestions {
                            let app_handle3 = app_handle2.clone();
                            let state_for_suggestions: tauri::State<AppState> = app_handle2.state();
                            let llm = {
                                let guard = state_for_suggestions.llm_assistant.read();
                                guard.clone()
                            };
                            let recent_transcripts = state_for_suggestions.recent_transcripts.lock().clone();
                            let meeting_context = state_for_suggestions.current_meeting_context.lock().clone();
                            let kb = state_for_suggestions.knowledge_base.clone();

                            if let Some(assistant) = llm {
                                if !recent_transcripts.is_empty() {
                                    // Spawn async task for suggestion generation
                                    std::thread::spawn(move || {
                                        let rt = tokio::runtime::Builder::new_current_thread()
                                            .enable_all()
                                            .build()
                                            .unwrap();

                                        rt.block_on(async {
                                            match assistant.generate_realtime_suggestions(&recent_transcripts, meeting_context.as_deref(), kb).await {
                                                Ok(suggestion) => {
                                                    // Only emit if there's actual content
                                                    if suggestion.insight.is_some() || suggestion.question.is_some() || suggestion.related_info.is_some() {
                                                        let _ = app_handle3.emit("realtime-suggestion", serde_json::json!({
                                                            "insight": suggestion.insight,
                                                            "question": suggestion.question,
                                                            "related_info": suggestion.related_info,
                                                        }));
                                                        println!("[Suggestions] Emitted real-time suggestion");
                                                    }
                                                }
                                                Err(e) => {
                                                    eprintln!("[Suggestions] Error generating: {}", e);
                                                }
                                            }
                                        });
                                    });
                                }
                            }
                        }
                    }

                    // Save final transcripts to knowledge base
                    if transcription.is_final && !transcription.text.trim().is_empty() {
                        let meeting_id = state.current_meeting_id.lock().clone();

                        if let Some(meeting_id) = meeting_id {
                            let kb = state.knowledge_base.clone();
                            let text = transcription.text.clone();
                            let speaker = if source == "microphone" { "You" } else { "Guest" }.to_string();
                            let timestamp = transcription.timestamp_ms;
                            let emotion = emotion_str.clone();
                            let is_turn_complete = transcription.is_turn_complete;

                            println!("[KB] Saving segment: speaker={}, text_len={}, emotion={}, turn_done={}",
                                speaker, text.len(), emotion, is_turn_complete);

                            // Run async KB operation
                            rt.block_on(async {
                                let kb_guard = kb.read().await;
                                if let Some(ref kb) = *kb_guard {
                                    match kb.add_segment(
                                        &meeting_id,
                                        &speaker,
                                        &text,
                                        timestamp,
                                        timestamp + 1000, // Approximate end time
                                    ).await {
                                        Ok(segment_id) => {
                                            println!("[KB] Segment saved successfully: {}", segment_id);
                                        }
                                        Err(e) => {
                                            eprintln!("[KB] ERROR saving segment: {}", e);
                                        }
                                    }
                                } else {
                                    eprintln!("[KB] Knowledge base not available in save loop");
                                }
                            });
                        }
                    }

                }
            }
        }
    });

    // Emit recording-started event
    let _ = app.emit("recording-started", ());

    println!("Recording started with audio capture and ASR");
    Ok(())
}

#[tauri::command]
fn stop_recording(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<(), String> {
    if !state.is_recording.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }

    // Stop audio capture
    let mut capture = state.audio_capture.lock();
    capture.stop();

    // Clear sender
    *state.audio_sender.lock() = None;

    // Note: Don't clear recording_start_time here - end_meeting uses it for diarization
    // It will be cleared in end_meeting

    state.is_recording.store(false, std::sync::atomic::Ordering::SeqCst);

    // Emit recording-stopped event
    let _ = app.emit("recording-stopped", ());

    println!("Recording stopped");
    Ok(())
}

#[tauri::command]
fn is_recording(state: tauri::State<AppState>) -> bool {
    state.is_recording.load(std::sync::atomic::Ordering::SeqCst)
}

#[tauri::command]
fn set_screen_share_protection(window: tauri::Window, enabled: bool) -> Result<(), String> {
    window.set_content_protected(enabled).map_err(|e| e.to_string())?;
    println!("Screen share protection: {}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

#[tauri::command]
fn check_models_status() -> Vec<ModelStatus> {
    get_models_status()
}

#[tauri::command]
fn are_models_ready() -> bool {
    all_models_installed()
}

#[tauri::command]
async fn download_models(app: tauri::AppHandle) -> Result<(), String> {
    download_all_models(app).await
}

#[tauri::command]
fn get_models_path() -> String {
    get_models_dir().to_string_lossy().to_string()
}

// ==================== AUDIO & DIARIZATION DIAGNOSTICS ====================

/// Check audio capture capabilities
#[tauri::command]
fn get_audio_capabilities() -> AudioCapabilities {
    check_audio_capabilities()
}

/// Check if diarization engine is initialized and ready
#[tauri::command]
fn get_diarization_status(state: tauri::State<AppState>) -> serde_json::Value {
    let diar_guard = state.diarization_engine.read();
    let is_initialized = diar_guard.is_some() && diar_guard.as_ref().map(|e| e.is_initialized()).unwrap_or(false);

    // Check if models are downloaded
    let models_dir = get_models_dir();
    let segmentation_exists = models_dir.join("sherpa-onnx-pyannote-segmentation-3-0").join("model.onnx").exists();
    let embedding_exists = models_dir.join("3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx").exists();

    serde_json::json!({
        "is_initialized": is_initialized,
        "segmentation_model_exists": segmentation_exists,
        "embedding_model_exists": embedding_exists,
        "models_dir": models_dir.to_string_lossy(),
        "ready": is_initialized && segmentation_exists && embedding_exists,
    })
}

// ==================== SCREENSHOT COMMANDS ====================

/// Capture a screenshot of the primary screen
#[tauri::command]
fn take_screenshot() -> Result<ScreenshotResult, String> {
    capture_screen()
}

/// Capture screenshot and analyze with LLM
#[tauri::command]
async fn analyze_screenshot(
    state: tauri::State<'_, AppState>,
    question: Option<String>,
) -> Result<String, String> {
    // Capture the screen
    let screenshot = capture_screen()?;

    // Get the LLM assistant (clone the Arc to release the lock before await)
    let assistant = {
        let llm_guard = state.llm_assistant.read();
        llm_guard.as_ref()
            .ok_or("LLM not initialized. Please configure your LLM endpoint in Settings.")?
            .clone()
    };

    // Build the prompt with the image
    let prompt = question.unwrap_or_else(|| {
        "I just captured my screen during a meeting. Please analyze this screenshot and tell me:\n\
         1. What is being discussed or shown?\n\
         2. Any important information visible (data, charts, text)?\n\
         3. Any action items or key points I should note?".to_string()
    });

    // Create a message with the image for vision models
    // Format as a data URL for the LLM
    let image_data_url = format!("data:image/png;base64,{}", screenshot.base64_data);

    // For vision-capable LLMs, we send the image as part of the message
    // The rig-core library handles multimodal messages
    let full_prompt = format!(
        "{}\n\n[Screenshot attached: {}x{} image]",
        prompt, screenshot.width, screenshot.height
    );

    // Use the assistant to analyze
    // Note: This requires a vision-capable model (GPT-4V, Claude 3, etc.)
    let response = assistant
        .ask_with_image(&full_prompt, &image_data_url)
        .await
        .map_err(|e| format!("LLM analysis failed: {}", e))?;

    println!("[Screenshot] LLM analysis complete ({} chars)", response.len());

    Ok(response)
}

// ==================== USER STORE COMMANDS ====================

// Initialize the user store (SQLite)
#[tauri::command]
fn initialize_user_store(state: tauri::State<AppState>) -> Result<(), String> {
    let mut store_guard = state.user_store.lock();

    if store_guard.is_some() {
        return Ok(()); // Already initialized
    }

    let data_dir = dirs::data_dir()
        .ok_or("Could not find data directory")?
        .join("second-brain");

    let store = UserStore::new(&data_dir)?;
    *store_guard = Some(store);

    println!("User store initialized");
    Ok(())
}

// Get user settings
#[tauri::command]
fn get_user_settings(state: tauri::State<AppState>) -> Result<UserSettings, String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.get_settings()
}

// Update user settings
#[tauri::command]
fn update_user_settings(state: tauri::State<AppState>, settings: UserSettings) -> Result<(), String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.update_settings(&settings)
}

// Set a single setting
#[tauri::command]
fn set_user_setting(state: tauri::State<AppState>, key: String, value: String) -> Result<(), String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.set_setting(&key, &value)
}

// Create a note
#[tauri::command]
fn create_note(state: tauri::State<AppState>, content: String, tags: Vec<String>) -> Result<Note, String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.create_note(&content, &tags)
}

// Get all notes
#[tauri::command]
fn get_notes(state: tauri::State<AppState>, limit: Option<usize>) -> Result<Vec<Note>, String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.get_notes(limit)
}

// Update a note
#[tauri::command]
fn update_note(state: tauri::State<AppState>, id: i64, content: String, tags: Vec<String>) -> Result<Note, String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.update_note(id, &content, &tags)
}

// Toggle note pin
#[tauri::command]
fn toggle_note_pin(state: tauri::State<AppState>, id: i64) -> Result<Note, String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.toggle_note_pin(id)
}

// Delete a note
#[tauri::command]
fn delete_note(state: tauri::State<AppState>, id: i64) -> Result<(), String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.delete_note(id)
}

// Get integrations
#[tauri::command]
fn get_integrations(state: tauri::State<AppState>) -> Result<Vec<Integration>, String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.get_integrations()
}

// Upsert integration
#[tauri::command]
fn upsert_integration(state: tauri::State<AppState>, integration: Integration) -> Result<(), String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.upsert_integration(&integration)
}

// Disconnect integration
#[tauri::command]
fn disconnect_integration(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.disconnect_integration(&id)
}

// Save a search
#[tauri::command]
fn save_search(state: tauri::State<AppState>, query: String, name: String) -> Result<SavedSearch, String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.save_search(&query, &name)
}

// Get saved searches
#[tauri::command]
fn get_saved_searches(state: tauri::State<AppState>) -> Result<Vec<SavedSearch>, String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.get_saved_searches()
}

// Delete saved search
#[tauri::command]
fn delete_saved_search(state: tauri::State<AppState>, id: i64) -> Result<(), String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.delete_saved_search(id)
}

// Get app state value
#[tauri::command]
fn get_app_state(state: tauri::State<AppState>, key: String) -> Result<Option<String>, String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.get_state(&key)
}

// Set app state value
#[tauri::command]
fn set_app_state(state: tauri::State<AppState>, key: String, value: String) -> Result<(), String> {
    let store_guard = state.user_store.lock();
    let store = store_guard.as_ref().ok_or("User store not initialized")?;
    store.set_state(&key, &value)
}

// ==================== Web Crawler Commands ====================

// Search the web using DuckDuckGo
#[tauri::command]
async fn search_web(
    query: String,
    limit: Option<usize>,
) -> Result<Vec<WebSearchResult>, String> {
    // Create a new crawler for each request (stateless)
    let crawler = WebCrawler::new();
    crawler.search(&query, limit.unwrap_or(10)).await
}

// Crawl a single URL and return content
#[tauri::command]
async fn crawl_url(
    url: String,
) -> Result<CrawledPage, String> {
    // Create a new crawler for each request (stateless)
    let crawler = WebCrawler::new();
    crawler.crawl_url(&url).await
}

// Crawl a URL and store it in the knowledge base
#[tauri::command]
async fn crawl_and_store(
    state: tauri::State<'_, AppState>,
    url: String,
    tags: Vec<String>,
) -> Result<String, String> {
    // Create a new crawler for each request (stateless)
    let crawler = WebCrawler::new();
    let crawled = crawler.crawl_url(&url).await?;

    // Then store in knowledge base
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    kb.add_knowledge_source(
        &crawled.url,
        &crawled.title,
        &crawled.markdown,
        "url",
        tags,
    ).await
}

// Upload and process a document (PDF, TXT, MD)
#[tauri::command]
async fn upload_document(
    state: tauri::State<'_, AppState>,
    file_path: String,
    tags: Vec<String>,
) -> Result<String, String> {
    use std::fs;
    use std::path::Path;

    let path = Path::new(&file_path);
    let file_name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();
    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Read file content based on type
    let content = match extension.as_str() {
        "txt" | "md" | "markdown" => {
            fs::read_to_string(&file_path)
                .map_err(|e| format!("Failed to read file: {}", e))?
        }
        "pdf" => {
            // Use pdf-extract crate for PDF parsing
            extract_pdf_text(&file_path)?
        }
        _ => return Err(format!("Unsupported file type: {}", extension)),
    };

    let source_type = if extension == "pdf" { "pdf" } else { "file" };

    // Store in knowledge base
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    kb.add_knowledge_source(
        &format!("file://{}", file_path),
        &file_name,
        &content,
        source_type,
        tags,
    ).await
}

// Extract text from PDF using pdf-extract
fn extract_pdf_text(file_path: &str) -> Result<String, String> {
    let bytes = std::fs::read(file_path)
        .map_err(|e| format!("Failed to read PDF: {}", e))?;

    pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| format!("Failed to extract PDF text: {}", e))
}

// Get all knowledge sources
#[tauri::command]
async fn get_knowledge_sources(
    state: tauri::State<'_, AppState>,
    tags: Option<Vec<String>>,
) -> Result<Vec<KnowledgeSource>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    kb.get_knowledge_sources(tags).await
}

// Delete a knowledge source
#[tauri::command]
async fn delete_knowledge_source(
    state: tauri::State<'_, AppState>,
    source_id: String,
) -> Result<(), String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    kb.delete_knowledge_source(&source_id).await
}

// Update tags for a knowledge source
#[tauri::command]
async fn update_source_tags(
    state: tauri::State<'_, AppState>,
    source_id: String,
    tags: Vec<String>,
) -> Result<(), String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    kb.update_source_tags(&source_id, tags).await
}

// Search knowledge chunks
#[tauri::command]
async fn search_knowledge_chunks(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<usize>,
    tags: Option<Vec<String>>,
) -> Result<Vec<KnowledgeSearchResult>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    kb.search_knowledge(&query, limit.unwrap_or(10), tags).await
}

// Cleanup orphaned chunks (chunks whose source was deleted)
#[tauri::command]
async fn cleanup_orphaned_chunks(
    state: tauri::State<'_, AppState>,
) -> Result<usize, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    kb.cleanup_orphaned_chunks().await
}

// Link knowledge source to meeting
#[tauri::command]
async fn link_knowledge_to_meeting(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    source_id: String,
) -> Result<(), String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    kb.link_knowledge_to_meeting(&meeting_id, &source_id, "user").await
}

// Get knowledge sources linked to a meeting
#[tauri::command]
async fn get_meeting_knowledge(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<KnowledgeSource>, String> {
    let kb_guard = state.knowledge_base.read().await;
    let kb = kb_guard.as_ref().ok_or("Knowledge base not initialized")?;

    kb.get_meeting_knowledge(&meeting_id).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::default())
        .setup(|app| {
            // Create tray menu
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let start = MenuItem::with_id(app, "start", "Start Recording", true, None::<&str>)?;
            let stop = MenuItem::with_id(app, "stop", "Stop Recording", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&show, &start, &stop, &quit])?;

            // Enable screen share protection - window won't appear in screen recordings/shares
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    let _ = window.set_content_protected(true);
                    println!("Screen share protection enabled");
                }
            }

            // Register global shortcuts
            use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

            let app_handle = app.handle().clone();

            // Screenshot shortcut: Cmd+Shift+S (macOS) / Ctrl+Shift+S (Windows)
            #[cfg(target_os = "macos")]
            let screenshot_shortcut = "Command+Shift+S";
            #[cfg(not(target_os = "macos"))]
            let screenshot_shortcut = "Ctrl+Shift+S";

            let shortcut: Shortcut = screenshot_shortcut.parse().unwrap();
            let screenshot_app = app_handle.clone();

            app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    println!("[Hotkey] Screenshot shortcut triggered");
                    // Emit event to frontend to trigger screenshot analysis
                    let _ = screenshot_app.emit("hotkey-screenshot", ());
                }
            })?;

            // Toggle recording shortcut: Cmd+Shift+R (macOS) / Ctrl+Shift+R (Windows)
            #[cfg(target_os = "macos")]
            let record_shortcut = "Command+Shift+R";
            #[cfg(not(target_os = "macos"))]
            let record_shortcut = "Ctrl+Shift+R";

            let shortcut: Shortcut = record_shortcut.parse().unwrap();
            let record_app = app_handle.clone();

            app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    println!("[Hotkey] Toggle recording shortcut triggered");
                    // Emit event to frontend to toggle recording
                    let _ = record_app.emit("hotkey-toggle-recording", ());
                }
            })?;

            println!("Global shortcuts registered: {} (screenshot), {} (toggle recording)", screenshot_shortcut, record_shortcut);

            // Build tray icon
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "start" => {
                        let state = app.state::<AppState>();
                        state.is_recording.store(true, std::sync::atomic::Ordering::SeqCst);
                        println!("Recording started from tray");
                    }
                    "stop" => {
                        let state = app.state::<AppState>();
                        state.is_recording.store(false, std::sync::atomic::Ordering::SeqCst);
                        println!("Recording stopped from tray");
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            initialize_asr,
            initialize_smart_turn,
            initialize_entities,
            initialize_embeddings,
            initialize_diarization,
            initialize_knowledge_base,
            initialize_llm,
            extract_entities,
            extract_entities_batch,
            start_meeting,
            end_meeting,
            add_transcript_segment,
            search_knowledge,
            get_action_items,
            get_decisions,
            // Meeting query commands
            get_meetings,
            get_meeting,
            get_meeting_segments,
            get_meeting_action_items,
            get_meeting_decisions,
            get_meeting_topics,
            get_meeting_people,
            get_meeting_stats,
            delete_meeting,
            get_all_action_items,
            get_all_decisions,
            get_knowledge_stats,
            update_action_item_status,
            get_current_meeting_id,
            // LLM commands
            ask_assistant,
            summarize_meeting,
            suggest_questions,
            ask_meeting_question,
            get_realtime_suggestions,
            clear_recent_transcripts,
            set_meeting_context,
            get_meeting_context,
            process_meeting_highlights,
            start_recording,
            stop_recording,
            is_recording,
            subscribe_transcription,
            unsubscribe_transcription,
            set_screen_share_protection,
            check_models_status,
            are_models_ready,
            download_models,
            get_models_path,
            // Audio & diarization diagnostics
            get_audio_capabilities,
            get_diarization_status,
            // Screenshot commands
            take_screenshot,
            analyze_screenshot,
            // User store commands
            initialize_user_store,
            get_user_settings,
            update_user_settings,
            set_user_setting,
            create_note,
            get_notes,
            update_note,
            toggle_note_pin,
            delete_note,
            get_integrations,
            upsert_integration,
            disconnect_integration,
            save_search,
            get_saved_searches,
            delete_saved_search,
            get_app_state,
            set_app_state,
            // Web crawler commands
            search_web,
            crawl_url,
            crawl_and_store,
            upload_document,
            get_knowledge_sources,
            delete_knowledge_source,
            update_source_tags,
            search_knowledge_chunks,
            cleanup_orphaned_chunks,
            link_knowledge_to_meeting,
            get_meeting_knowledge,
            // Agent queue commands
            initialize_agent_queue,
            get_queue_stats,
            queue_ask_question,
            queue_realtime_suggestions,
            queue_meeting_highlights,
            queue_entity_extraction
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
