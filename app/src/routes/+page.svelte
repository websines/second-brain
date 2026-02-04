<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import ModelSetup from "$lib/components/ModelSetup.svelte";
  import LLMSetup from "$lib/components/LLMSetup.svelte";
  import SecondBrain from "$lib/components/SecondBrain.svelte";
  import { audioPipeline } from "$lib/audio-pipeline";

  let isRecording = $state(false);
  let modelsReady = $state(false);
  let checkingModels = $state(true);
  let llmConfigured = $state(false);
  let checkingLLM = $state(false);
  let appInitialized = $state(false);
  let pipelineError = $state("");

  let unlistenSample: (() => void) | null = null;

  onMount(async () => {
    // Check if models are ready
    try {
      modelsReady = await invoke("are_models_ready");
    } catch (e) {
      console.error("Failed to check models:", e);
      modelsReady = false;
    }
    checkingModels = false;

    if (modelsReady) {
      await checkLLMConfiguration();
    }
  });

  async function checkLLMConfiguration() {
    checkingLLM = true;
    try {
      // Initialize user store first if not already done
      try {
        await invoke("initialize_user_store");
      } catch (e) {
        console.log("User store may already be initialized:", e);
      }

      // Check if LLM is configured
      const settings = await invoke<{
        llm_url: string;
        llm_model: string;
      }>("get_user_settings");

      // Check if setup was completed or skipped
      const setupState = await invoke<string | null>("get_app_state", { key: "llm_setup_complete" });

      if ((settings.llm_url && settings.llm_url.trim() !== "") || setupState === "skipped" || setupState === "true") {
        llmConfigured = true;
        await initializeApp();
      } else {
        llmConfigured = false;
      }
    } catch (e) {
      console.error("Failed to check LLM configuration:", e);
      llmConfigured = false;
    }
    checkingLLM = false;
  }

  function handleLLMSetupComplete() {
    llmConfigured = true;
    initializeApp();
  }

  async function initializeApp() {
    // Check initial recording state
    isRecording = await invoke("is_recording");

    // Initialize user store (SQLite) first
    try {
      await invoke("initialize_user_store");
      console.log("User store initialized");
    } catch (e) {
      console.error("Failed to initialize user store:", e);
    }

    // Initialize ASR engine (SenseVoice) in Rust backend
    try {
      await invoke("initialize_asr");
      console.log("ASR engine (SenseVoice) initialized");
    } catch (e) {
      console.error("Failed to initialize ASR:", e);
      pipelineError = `ASR init failed: ${e}`;
    }

    // Initialize Smart Turn v3 (turn detection)
    try {
      await invoke("initialize_smart_turn");
      console.log("Smart Turn v3 initialized");
    } catch (e) {
      console.log("Smart Turn not available:", e);
    }

    // Initialize Embedding engine (for semantic search)
    try {
      await invoke("initialize_embeddings");
      console.log("Embedding engine initialized");
    } catch (e) {
      console.error("Failed to initialize embeddings:", e);
    }

    // Initialize Entity extraction engine (MUST be before Knowledge Base)
    try {
      await invoke("initialize_entities");
      console.log("Entity engine initialized");
    } catch (e) {
      console.error("Failed to initialize entities:", e);
    }

    // Initialize Knowledge Base (SurrealDB) - REQUIRES entity engine
    try {
      await invoke("initialize_knowledge_base");
      console.log("Knowledge base initialized");
    } catch (e) {
      console.error("Failed to initialize knowledge base:", e);
      pipelineError = `Knowledge base init failed: ${e}`;
    }

    // Initialize LLM Assistant (for AI questions)
    try {
      await invoke("initialize_llm", { apiUrl: null, model: null });
      console.log("LLM assistant initialized");
    } catch (e) {
      console.error("Failed to initialize LLM:", e);
    }

    // Initialize Speaker Diarization (optional - may not have models yet)
    try {
      await invoke("initialize_diarization");
      console.log("Speaker diarization initialized");
    } catch (e) {
      console.log("Speaker diarization not available:", e);
    }

    // Initialize audio pipeline (for transcription event listening)
    try {
      await audioPipeline.initialize();
    } catch (e) {
      console.error("Failed to initialize pipeline:", e);
      pipelineError = String(e);
    }

    // Listen for audio sample events
    unlistenSample = await listen<{
      source: string;
      timestamp_ms: number;
      sample_count: number;
      sample_rate: number;
    }>("audio-sample", (event) => {
      // Audio is being captured
    });

    // Listen for recording state changes
    await listen("recording-started", () => {
      isRecording = true;
    });

    await listen("recording-stopped", () => {
      isRecording = false;
    });

    // Mark app as fully initialized
    appInitialized = true;
    console.log("App fully initialized");
  }

  function handleSetupComplete() {
    modelsReady = true;
    checkLLMConfiguration();
  }

  onDestroy(() => {
    if (unlistenSample) unlistenSample();
    audioPipeline.destroy();
  });
</script>

{#if checkingModels}
  <!-- Loading state -->
  <div class="flex items-center justify-center min-h-screen bg-zinc-950">
    <div class="w-10 h-10 border-3 border-indigo-500/20 border-t-indigo-500 rounded-full animate-spin"></div>
  </div>
{:else if !modelsReady}
  <!-- First boot - download AI models -->
  <ModelSetup onComplete={handleSetupComplete} />
{:else if checkingLLM}
  <!-- Checking LLM configuration -->
  <div class="flex flex-col items-center justify-center min-h-screen bg-zinc-950 gap-4">
    <div class="w-10 h-10 border-3 border-indigo-500/20 border-t-indigo-500 rounded-full animate-spin"></div>
    <p class="text-zinc-500 text-sm">Checking configuration...</p>
  </div>
{:else if !llmConfigured}
  <!-- LLM Setup Wizard -->
  <LLMSetup onComplete={handleLLMSetupComplete} />
{:else if !appInitialized}
  <!-- Initializing engines -->
  <div class="flex flex-col items-center justify-center min-h-screen bg-zinc-950 gap-4">
    <div class="w-10 h-10 border-3 border-indigo-500/20 border-t-indigo-500 rounded-full animate-spin"></div>
    <p class="text-zinc-500 text-sm">Initializing engines...</p>
  </div>
{:else}
  <!-- Main App -->
  <SecondBrain {isRecording} />
{/if}
