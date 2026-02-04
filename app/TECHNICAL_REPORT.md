# Second Brain - Technical Report

> **Real-time meeting assistant with contextual intelligence**

This document provides a comprehensive technical overview of the Second Brain application architecture, technology choices, and implementation patterns. It's designed to help team members quickly understand and contribute to the codebase.

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Architecture Overview](#architecture-overview)
3. [Technology Stack](#technology-stack)
4. [Frontend Architecture](#frontend-architecture)
5. [Backend Architecture (Rust/Tauri)](#backend-architecture-rusttauri)
6. [Data Layer](#data-layer)
7. [ML & AI Pipeline](#ml--ai-pipeline)
8. [Real-time Audio Processing](#real-time-audio-processing)
9. [State Management](#state-management)
10. [API Surface (Tauri Commands)](#api-surface-tauri-commands)
11. [Styling & UI Patterns](#styling--ui-patterns)
12. [Security & Privacy](#security--privacy)
13. [Initialization Flow](#initialization-flow)
14. [Directory Structure](#directory-structure)
15. [Key Architectural Decisions](#key-architectural-decisions)
16. [Performance Considerations](#performance-considerations)
17. [Getting Started for Developers](#getting-started-for-developers)

---

## Executive Summary

Second Brain is a **native desktop application** built with a modern hybrid architecture: **SvelteKit** for the frontend and **Rust/Tauri** for the backend. The application captures audio from meetings, transcribes them in real-time using on-device ML models, extracts insights and entities, and provides intelligent assistance through LLM integration.

### Key Capabilities

- **Real-time transcription** with emotion and language detection
- **Named Entity Recognition** (people, topics, organizations)
- **Semantic search** across meetings and knowledge base
- **Action item and decision extraction**
- **LLM-powered assistant** for meeting insights
- **Screenshot analysis** with vision capabilities
- **Local-first architecture** - all data stays on device

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                        Desktop Application                        │
├──────────────────────────────────────────────────────────────────┤
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                    SvelteKit Frontend                       │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │  │
│  │  │   Main   │  │ Meeting  │  │Knowledge │  │  Notes   │   │  │
│  │  │   View   │  │  Detail  │  │   Base   │  │  & More  │   │  │
│  │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │  │
│  │       └──────────────┴────────────┴─────────────┘          │  │
│  │                          │                                  │  │
│  │                   Tauri IPC Bridge                          │  │
│  └──────────────────────────┼──────────────────────────────────┘  │
│                             │                                     │
│  ┌──────────────────────────┴──────────────────────────────────┐  │
│  │                    Rust Backend (Tauri)                      │  │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐        │  │
│  │  │  Audio  │  │   ASR   │  │   ML    │  │   LLM   │        │  │
│  │  │ Capture │  │ Engine  │  │ Engines │  │  Agent  │        │  │
│  │  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘        │  │
│  │       └────────────┴────────────┴─────────────┘             │  │
│  │                          │                                   │  │
│  │  ┌───────────────────────┴───────────────────────┐          │  │
│  │  │              Data Layer                        │          │  │
│  │  │  ┌─────────────┐        ┌─────────────┐       │          │  │
│  │  │  │  SurrealDB  │        │   SQLite    │       │          │  │
│  │  │  │ (Knowledge) │        │(User Store) │       │          │  │
│  │  │  └─────────────┘        └─────────────┘       │          │  │
│  │  └───────────────────────────────────────────────┘          │  │
│  └──────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### Why This Architecture?

| Choice | Rationale |
|--------|-----------|
| **Tauri over Electron** | 10x smaller binary size, native Rust performance, better security model, lower memory footprint | Anti Detection Measures cannot anticipate this
| **SvelteKit over React/Vue** | Smaller bundle size, no virtual DOM overhead, excellent TypeScript support, Svelte 5 runes for fine-grained reactivity |
| **Rust Backend** | Memory safety, high performance for audio processing, excellent async support with Tokio |
| **Local ML Models** | Privacy-first design, no internet required for core features, low latency transcription |

---

## Technology Stack

### Frontend

| Technology | Version | Purpose |
|------------|---------|---------|
| **SvelteKit** | 2.9.0 | Application framework |
| **Svelte** | 5.0.0 | UI component framework with runes |
| **TypeScript** | 5.6.2 | Type safety |
| **Vite** | 6.0.3 | Build tool and dev server |
| **Tailwind CSS** | 4.1.17 | Utility-first styling |
| **Lucide Svelte** | 0.555.0 | Icon library |
| **Tauri API** | 2.x | Frontend-backend communication |

### Backend (Rust)

| Technology | Version | Purpose |
|------------|---------|---------|
| **Tauri** | 2.0 | Desktop framework |
| **Tokio** | 1.x | Async runtime |
| **CPAL** | 0.15 | Cross-platform audio capture |
| **sherpa-rs** | 0.6 | SenseVoice ASR engine |
| **ORT (ONNX Runtime)** | 2.0.0-rc.9 | ML model inference |
| **gline-rs** | 1.x | GLiNER for NER |
| **SurrealDB** | 2.1 | Knowledge database |
| **rusqlite** | 0.32 | User data storage |
| **rig-core** | 0.8 | LLM agent framework |
| **parking_lot** | 0.12 | High-performance synchronization |

### Key Libraries by Domain

**Audio Processing**
- `cpal` - Cross-platform audio I/O (microphone + system audio)
- `sherpa-rs` - Speech recognition with SenseVoice

**Machine Learning**
- `ort` - ONNX Runtime for embeddings
- `gline-rs` - GLiNER Named Entity Recognition
- `tokenizers` - Text tokenization

**Content Processing**
- `text-splitter` - Intelligent document chunking
- `pdf-extract` - PDF text extraction
- `spider` - Web crawling

**Networking**
- `reqwest` - HTTP client with streaming
- `duckduckgo_search` - Web search integration

---

## Frontend Architecture

### Framework Configuration

The frontend uses **SvelteKit with Static Adapter** - this is critical for Tauri compatibility since Tauri doesn't provide a Node.js server for SSR.

**Key Configuration Points:**

1. **SSR Disabled** - All routes export `ssr = false`
2. **Static Adapter** - Builds to static HTML/JS with SPA fallback
3. **Vite Dev Server** - Runs on port 1420 (Tauri's expected port)
4. **HMR** - Hot Module Replacement on port 1421

### Component Architecture

The application follows a **single-page component-based architecture**:

```
src/
├── routes/
│   ├── +layout.svelte     → Root wrapper
│   ├── +page.svelte       → Main entry point
│   └── overlay/
│       └── +page.svelte   → Floating suggestions window
└── lib/
    ├── components/
    │   ├── SecondBrain.svelte        → Main app
    │   ├── MeetingDetailPage.svelte  → Meeting view
    │   ├── MeetingPrepPage.svelte    → Pre-meeting setup
    │   ├── ModelSetup.svelte         → AI model downloader
    │   ├── LLMSetup.svelte           → LLM configuration
    │   ├── KnowledgeBaseView.svelte  → Document management
    │   ├── KnowledgeSearch.svelte    → Semantic search
    │   └── TranscriptView.svelte     → Live transcript
    ├── audio-pipeline.ts             → Audio orchestration
    ├── transcription.ts              → Transcription service
    └── smart-turn.ts                 → Turn detection
```

### Navigation Pattern

Instead of traditional file-based routing, the app uses **view-based navigation** within `SecondBrain.svelte`:

- Views: `home` | `meetings` | `notes` | `insights` | `tools` | `knowledge`
- Navigation handled by `activeView` state variable
- Single-component architecture reduces complexity

### Svelte 5 Runes

The codebase uses **Svelte 5's new reactivity system**:

| Rune | Usage |
|------|-------|
| `$state()` | Reactive state declaration |
| `$derived()` | Computed values |
| `$derived.by()` | Complex computed values |
| `$effect()` | Side effects |

This replaces Svelte 4's `let`/`$:` syntax with explicit, more predictable reactivity.

---

## Backend Architecture (Rust/Tauri)

### AppState Structure

The backend maintains a centralized state with careful synchronization:

**Read-Heavy Resources (RwLock)**
- ASR Engine
- ML Engines (Embeddings, Entities, Diarization)
- LLM Assistant
- Agent Queue

**Write-Heavy Resources (Mutex)**
- Audio Buffers
- Current Meeting ID
- Transcription Channel
- User Store

### Why These Synchronization Choices?

| Type | Use Case | Rationale |
|------|----------|-----------|
| `parking_lot::RwLock` | ML Engines | Allow concurrent reads during inference, rare writes for initialization |
| `parking_lot::Mutex` | Audio Buffers | Frequent writes from audio thread, short-held locks |
| `tokio::RwLock` | Knowledge Base | Async-aware locking for database operations |
| `AtomicBool` | Recording State | Lock-free status checks from multiple threads |

### Tauri Plugin System

The app uses several Tauri plugins for native functionality:

- **dialog** - File picker, save dialogs
- **fs** - File system operations
- **opener** - Open external apps/URLs
- **global-shortcut** - System-wide hotkeys

---

## Data Layer

### Dual Database Strategy

The application uses **two specialized databases** for different concerns:

#### SurrealDB (Knowledge Base)

**Purpose:** Store meetings, transcripts, entities, and enable semantic search

**Why SurrealDB?**
- Multi-model: Document + Graph + Vector search in one database
- Embedded mode with RocksDB backend
- Built-in full-text search
- Excellent for relationship mapping between entities

**Tables:**
| Table | Purpose |
|-------|---------|
| `meetings` | Meeting records with metadata |
| `transcript_segments` | Transcript chunks with embeddings |
| `action_items` | Extracted action items |
| `decisions` | Key decisions made |
| `entities` | Named entities (people, topics, orgs) |
| `knowledge_sources` | User-added documents |
| `topics` | Discovered topics with embeddings |
| `people` | Identified speakers |

#### SQLite (User Store)

**Purpose:** Store user preferences, notes, and OAuth tokens

**Why SQLite?**
- Battle-tested reliability
- Simple relational queries
- Excellent for key-value settings
- Lightweight for user data

**Tables:**
| Table | Purpose |
|-------|---------|
| `users` | User profile |
| `settings` | App preferences |
| `notes` | User notes with tags |
| `integrations` | OAuth tokens for integrations |
| `saved_searches` | Search query history |

---

## ML & AI Pipeline

### On-Device Models

| Model | Library | Purpose |
|-------|---------|---------|
| **SenseVoice** | sherpa-rs | Speech-to-text with emotion/language detection |
| **E5 Embeddings** | ONNX Runtime | Semantic search vectors |
| **GLiNER** | gline-rs | Named Entity + Relation Recognition |
| **Speaker Diarization** | ONNX | Speaker identification |
| **Silero VAD + PipeCat Smart Turn** | Custom | Vad & Turn-taking detection |

### Model Management

Models are downloaded on first launch:
1. Check model status via `check_models_status()`
2. Display download progress in `ModelSetup.svelte`
3. Extract and cache in platform-specific data directory
4. Models persist across app updates

### Entity Extraction Pipeline

```
Raw Transcript
    ↓
[GLiNER NER] → People, Organizations, Topics, Actions
    ↓
[Embedding Engine] → Vector representations
    ↓
[SurrealDB] → Stored with relationships
    ↓
[Semantic Search] ← User queries
```

### LLM Integration

The app supports **OpenAI-compatible APIs**:
- OpenAI
- Ollama (local)
- LM Studio (local)
- OpenRouter
- Any compatible endpoint

**Use Cases:**
1. Meeting Q&A - Ask questions about past meetings
2. Real-time suggestions - Insights during recording
3. Screenshot analysis - Vision-powered screen understanding
4. Meeting highlights - Post-meeting summaries

---

## Real-time Audio Processing

### Audio Pipeline Architecture

```
┌─────────────────┐     ┌─────────────────┐
│   Microphone    │     │  System Audio   │
│     (cpal)      │     │   (cpal/WASAPI) │
└────────┬────────┘     └────────┬────────┘
         │                       │
         ▼                       ▼
    ┌────────────────────────────────┐
    │     Adaptive Audio Chunking     │
    │   (RMS-based speech detection)  │
    └────────────────┬───────────────┘
                     │
                     ▼
    ┌────────────────────────────────┐
    │       SenseVoice ASR           │
    │  (transcription + emotion +    │
    │   language + audio events)     │
    └────────────────┬───────────────┘
                     │
                     ▼
    ┌────────────────────────────────┐
    │      Smart Turn Detection      │
    │   (turn-taking confidence)     │
    └────────────────┬───────────────┘
                     │
                     ▼
    ┌────────────────────────────────┐
    │       Tauri Channel            │
    │    (stream to frontend)        │
    └────────────────┬───────────────┘
                     │
                     ▼
    ┌────────────────────────────────┐
    │       Frontend Listener        │
    │      (real-time UI update)     │
    └────────────────────────────────┘
```

### Adaptive Chunking Strategy

| Scenario | Chunk Size | Rationale |
|----------|------------|-----------|
| Active Speech | 50ms | Responsive transcription |
| Silence | 250ms | Reduce processing overhead |
| Min emission interval | 40ms | Prevent event flooding |

### Smart Turn Detection

Inspired by Pipecat's turn detection, the system identifies when speakers complete their turns:

**Signals Analyzed:**
- Silence duration (700ms - 2000ms thresholds)
- Sentence completeness
- Turn-taking phrases
- Confidence scoring (0.0 - 1.0)

---

## State Management

### Frontend State (Svelte)

The application uses **component-local state** with Svelte 5 runes - no external state management library (Redux, Pinia, etc.).

**Key State Categories:**

| Category | Examples |
|----------|----------|
| Meeting State | `currentMeetingId`, `meetingTitle`, `meetingContext` |
| Recording State | `isRecording`, `liveTranscript` |
| Navigation | `activeView` |
| UI State | `isLoading`, `errorMessage`, modals |
| Data | `meetings`, `actionItems`, `decisions`, `notes` |

### Data Flow Pattern

```
User Action
    ↓
Component State Update
    ↓
Tauri invoke() Call
    ↓
Rust Command Handler
    ↓
Database/Engine Operation
    ↓
Response to Frontend
    ↓
State Update
    ↓
UI Re-render
```

### Real-time Updates

For streaming data (transcription), the app uses **Tauri Channels**:

1. Frontend subscribes via `subscribe_transcription(channel)`
2. Backend streams `TranscriptionEvent` objects
3. Frontend listener updates UI in real-time
4. Cleanup via `unsubscribe_transcription()` on unmount

---

## API Surface (Tauri Commands)

The backend exposes **50+ commands** organized by domain:

### Model Management
| Command | Returns |
|---------|---------|
| `check_models_status()` | `Vec<ModelStatus>` |
| `are_models_ready()` | `bool` |
| `download_models()` | Progress events |

### Recording & Audio
| Command | Returns |
|---------|---------|
| `start_recording()` | - |
| `stop_recording()` | - |
| `is_recording()` | `bool` |
| `subscribe_transcription(channel)` | Stream |

### Knowledge Base
| Command | Returns |
|---------|---------|
| `start_meeting(title, participants)` | `String` (ID) |
| `end_meeting(summary)` | - |
| `get_meetings(limit)` | `Vec<Meeting>` |
| `search_knowledge(query, limit)` | Search results |

### User Store
| Command | Returns |
|---------|---------|
| `get_user_settings()` | `UserSettings` |
| `get_notes(limit)` | `Vec<Note>` |
| `create_note(content, tags)` | `Note` |

### LLM Assistant
| Command | Returns |
|---------|---------|
| `ask_assistant(question)` | `String` |
| `get_realtime_suggestions(context)` | Suggestions |
| `analyze_screenshot(question?)` | `String` |

---

## Styling & UI Patterns

### Tailwind CSS Configuration

- **Theme:** Dark mode only (hardcoded)
- **Font Stack:** Inter (sans), JetBrains Mono (mono)
- **Color Palette:** Zinc-based with indigo accents

### Common Patterns

| Pattern | Implementation |
|---------|----------------|
| Backgrounds | `bg-[#191919]`, `bg-[#252525]`, `bg-[#2C2C2C]` |
| Text | `text-zinc-100`, `text-zinc-400` |
| Borders | `border-[#333]` |
| Accents | `text-indigo-400`, `bg-indigo-500` |

### UI Components

All components are **custom-built** - no external component library (shadcn, Material, etc.). This was intentional for:
- Bundle size optimization
- Full design control
- Consistent dark theme

---

## Security & Privacy

### Privacy-First Design

| Feature | Implementation |
|---------|----------------|
| **Local Storage** | All data in platform-specific local directories |
| **On-Device ML** | No cloud processing for transcription |
| **Stealth Mode** | Hide window from screen recordings |
| **No Telemetry** | No usage tracking or analytics |

### LLM Security

- API keys stored in encrypted SQLite
- Connection validated before saving
- Optional local LLM support (Ollama, LM Studio)

### Tauri Security Model

- Process isolation between frontend and backend
- Explicit command allowlist
- No arbitrary code execution

---

## Initialization Flow

```
App Launch
    │
    ▼
┌─────────────────────────┐
│  Check Models Status    │─── Missing? → Download with progress
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  LLM Configuration      │─── Not configured? → Setup wizard
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  Initialize User Store  │ (SQLite)
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  Initialize ASR Engine  │ (SenseVoice)
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  Initialize ML Engines  │ (Embeddings, Entities, Diarization)
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│ Initialize Knowledge DB │ (SurrealDB)
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│  Initialize LLM Agent   │
└───────────┬─────────────┘
            │
            ▼
┌─────────────────────────┐
│    Show Main App UI     │
└─────────────────────────┘
```

---

## Directory Structure

```
app/
├── src/                           # SvelteKit frontend
│   ├── routes/                    # File-based routing
│   │   ├── +layout.svelte        # Root wrapper
│   │   ├── +layout.ts            # SSR disabled
│   │   ├── +page.svelte          # Main entry
│   │   └── overlay/+page.svelte  # Suggestions overlay
│   ├── lib/
│   │   ├── components/           # Svelte components
│   │   ├── audio-pipeline.ts     # Audio coordination
│   │   ├── transcription.ts      # Transcription service
│   │   └── smart-turn.ts         # Turn detection
│   └── app.css                   # Global styles
│
├── src-tauri/                     # Rust backend
│   ├── src/
│   │   ├── lib.rs               # Tauri commands
│   │   ├── audio.rs             # Audio capture
│   │   ├── asr.rs               # ASR engine
│   │   ├── smart_turn.rs        # Turn detection
│   │   ├── embeddings.rs        # Embedding engine
│   │   ├── entities.rs          # NER engine
│   │   ├── knowledge_base.rs    # SurrealDB ops
│   │   ├── user_store.rs        # SQLite ops
│   │   ├── llm_agent.rs         # LLM integration
│   │   └── main.rs              # Entry point
│   ├── Cargo.toml               # Rust dependencies
│   └── tauri.conf.json          # Tauri config
│
├── build/                        # Compiled output
├── package.json                  # Frontend deps
├── vite.config.js               # Vite config
├── svelte.config.js             # SvelteKit config
└── tsconfig.json                # TypeScript config
```

---

## Key Architectural Decisions

| Decision | Rationale |
|----------|-----------|
| **No external state management** | Svelte 5 runes provide sufficient reactivity without Redux/Pinia overhead |
| **SPA with static adapter** | Required for Tauri - no Node.js server available |
| **Dual database (SurrealDB + SQLite)** | Separation of concerns: knowledge graph vs. user preferences |
| **parking_lot over std::sync** | 10-20% faster locks, crucial for audio processing |
| **Tauri Channels + Events** | Channels for performance, events for reliability fallback |
| **Local-first architecture** | Privacy, offline capability, no subscription required |
| **Modular ML engines** | Initialize on-demand to reduce memory footprint |
| **Adaptive audio chunking** | Balance responsiveness with CPU efficiency |

---

## Performance Considerations

| Concern | Solution |
|---------|----------|
| **Memory - Transcripts** | Max 50 segments in live buffer |
| **Memory - LLM Context** | Max 10 recent transcripts for suggestions |
| **Startup Time** | Lazy model loading on first use |
| **Audio Latency** | Adaptive chunking (50ms speech, 250ms silence) |
| **Event Flooding** | Min 40ms between audio emissions |
| **Database Queries** | Connection pooling for both SQLite and SurrealDB |
| **UI Responsiveness** | Async operations don't block main thread |

---

## Getting Started for Developers

### Prerequisites

- **Node.js** 18+
- **Rust** 1.70+
- **Tauri CLI** 2.x

### Development Setup

```bash
# Install frontend dependencies
npm install

# Run in development mode (starts both Vite and Tauri)
npm run tauri dev

# Type checking
npm run check

# Build for production
npm run tauri build
```

### Key Files to Understand First

1. `src/lib/components/SecondBrain.svelte` - Main application component
2. `src-tauri/src/lib.rs` - All Tauri commands
3. `src-tauri/src/audio.rs` - Audio capture logic
4. `src-tauri/src/knowledge_base.rs` - SurrealDB operations
5. `src/lib/transcription.ts` - Frontend transcription handling

### Adding a New Feature

1. **Backend Command** - Add to `lib.rs` with `#[tauri::command]`
2. **Frontend Service** - Create in `src/lib/` if complex
3. **UI Component** - Add to `SecondBrain.svelte` or create new component
4. **Database Schema** - Modify `knowledge_base.rs` or `user_store.rs`

---

## Appendix: Third-Party Dependencies

### Frontend (package.json)

| Package | Purpose |
|---------|---------|
| `@tauri-apps/api` | Tauri IPC communication |
| `@tauri-apps/plugin-dialog` | File dialogs |
| `@tauri-apps/plugin-fs` | File system access |
| `@tauri-apps/plugin-opener` | Open external apps |
| `lucide-svelte` | Icon library |

### Backend (Cargo.toml)

| Crate | Purpose |
|-------|---------|
| `tauri` | Desktop framework |
| `cpal` | Audio capture |
| `sherpa-rs` | SenseVoice ASR |
| `ort` | ONNX inference |
| `gline-rs` | GLiNER NER |
| `surrealdb` | Knowledge database |
| `rusqlite` | User store |
| `rig-core` | LLM agents |
| `tokio` | Async runtime |
| `reqwest` | HTTP client |
| `spider` | Web crawling |
| `text-splitter` | Document chunking |

---

*Last updated: January 2026*
*Generated for team onboarding and reference*
