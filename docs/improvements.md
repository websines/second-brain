  ---
  What the App Does

  Second Brain is a real-time meeting intelligence desktop application that:

  1. Captures & Transcribes Meetings

  - Records from microphone (your voice) and system audio (meeting participants via Zoom/Teams)
  - Uses SenseVoice (via sherpa-rs) for real-time ASR with emotion detection and audio event detection (laughter, applause)
  - Silero VAD for voice activity detection

  2. Extracts Knowledge Automatically

  - GLiNER for named entity recognition (people, topics, projects, action items, decisions)
  - Extracts relationships between entities (e.g., "John works on Project X")
  - Smart Turn v3 model detects conversation turn boundaries

  3. Stores in a Knowledge Graph

  - SurrealDB (RocksDB backend) for graph + vector + full-text search
  - Embeddings via ONNX Runtime for semantic search
  - Stores meetings, transcripts, action items, decisions, topics, people with relationships

  4. Provides AI-Powered Assistance

  - Graph-RAG (Graph + Retrieval Augmented Generation) for rich context retrieval
  - Real-time suggestions during meetings via LLM (OpenAI-compatible API)
  - Post-meeting highlights extraction (summary, action items, decisions)
  - Natural language Q&A about your knowledge base

  5. User Interface

  - Tauri 2 desktop app with Svelte 5 frontend
  - Dashboard, Meetings, Notes, Insights, Knowledge Base views
  - Live transcript with emotion indicators
  - Floating overlay window for AI suggestions during meetings

  ---
  Potential Improvements

  Performance & Architecture

  | Area              | Issue                                                                  | Improvement
                                                                              |
  |-------------------|------------------------------------------------------------------------|-------------------------------------
  ----------------------------------------------------------------------------|
  | Mutex contention  | Heavy use of std::sync::Mutex in lib.rs:40-80 for engines              | Consider using tokio::sync::RwLock
  for read-heavy access patterns, or parking_lot::Mutex for better performance |
  | Blocking in async | state.asr_engine.lock().unwrap() called in async context (lib.rs:1163) | Use spawn_blocking for CPU-intensive
   ASR processing to avoid blocking the async runtime                         |
  | Agent queue       | Currently processes inline (comments at lib.rs:745-746)                | Implement actual background worker
  pool for true async processing                                               |
  | Resampling        | Simple linear resampling (asr.rs:345-365)                              | Use rubato crate for higher-quality
  resampling                                                                  |

  Audio Processing

  | Area                | Current                                        | Improvement
                                                  |
  |---------------------|------------------------------------------------|-----------------------------------------------------------
  ------------------------------------------------|
  | macOS system audio  | Requires BlackHole/Loopback (audio.rs:161-211) | Implement native ScreenCaptureKit integration for seamless
   system audio capture without third-party tools |
  | Speaker diarization | Post-meeting only                              | Add real-time speaker diarization using speaker embeddings
   during recording                               |
  | Audio chunking      | Fixed 100ms chunks                             | Adaptive chunking based on VAD confidence
                                                  |

  LLM & RAG

  | Area                | Current                                   | Improvement                                                |
  |---------------------|-------------------------------------------|------------------------------------------------------------|
  | Streaming responses | Full response wait (llm_agent.rs:766-771) | Implement SSE/streaming for progressive response display   |
  | Model fallback      | Single LLM endpoint                       | Add fallback chain (local → cloud) with timeout handling   |
  | Context window      | Full transcript sent                      | Implement sliding window + summarization for long meetings |
  | Prompt caching      | None                                      | Cache embeddings and frequent query patterns               |

  Knowledge Base

  | Area                  | Current           | Improvement                                                           |
  |-----------------------|-------------------|-----------------------------------------------------------------------|
  | Duplicate detection   | None apparent     | Add embedding-based deduplication for similar chunks                  |
  | Source freshness      | Manual only       | Add scheduled re-crawling for URLs with last_updated tracking         |
  | Cross-meeting linking | Via entities only | Add explicit meeting references ("In our last discussion about X...") |

  Frontend/UX

  | Area               | Current           | Improvement                                                  |
  |--------------------|-------------------|--------------------------------------------------------------|
  | Real-time sync     | Polling/events    | Implement Tauri's new SSE-like channels for smoother updates |
  | Offline support    | Not implemented   | Add service worker + local caching for offline access        |
  | Keyboard shortcuts | Limited           | Add Cmd+R to record, Cmd+Space for AI assistant              |
  | Search             | Basic text filter | Add fuzzy search with highlighting                           |

  Security & Privacy

  | Area                  | Suggestion                                              |
  |-----------------------|---------------------------------------------------------|
  | Screen protection     | Already implemented (good!) but make it configurable    |
  | Transcript encryption | Add at-rest encryption for SurrealDB                    |
  | LLM data              | Add option to use fully local LLMs (Ollama integration) |
  | Audit logging         | Log who accessed what data and when                     |

  Code Quality

  | Area             | Suggestion                                                                                           |
  |------------------|------------------------------------------------------------------------------------------------------|
  | Error handling   | Many .ok() calls silently ignore errors (e.g., knowledge_base.rs:500-552) - add proper error logging |
  | Testing          | Add integration tests for audio pipeline and KB operations                                           |
  | Type consistency | Meeting IDs sometimes include meeting: prefix, sometimes not - standardize                           |

  ---
  Quick Wins (Low Effort, High Impact)

  1. Add streaming LLM responses - Much better UX during AI queries
  2. Implement keyboard shortcuts - Power users will love this
  3. Add error toasts - Currently errors are only logged to console
  4. Better empty states - Guide users when KB is empty
  5. Export functionality - Allow exporting meetings to markdown/PDF

  This is an impressive and well-architected application. The Graph-RAG implementation is particularly sophisticated!