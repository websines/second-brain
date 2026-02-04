<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, onDestroy } from "svelte";
  import { Sparkles, Lightbulb, HelpCircle, Link as LinkIcon, X, Minimize2, RefreshCw } from "lucide-svelte";

  interface RealtimeSuggestion {
    insight: string | null;
    question: string | null;
    related_info: string | null;
  }

  let suggestion = $state<RealtimeSuggestion | null>(null);
  let isLoading = $state(false);
  let lastUpdate = $state<number>(0);
  let transcriptCount = $state(0);
  let isMinimized = $state(false);
  let unlistenSuggestion: (() => void) | null = null;
  let unlistenTranscription: (() => void) | null = null;
  let unlistenRecordingStopped: (() => void) | null = null;

  onMount(async () => {
    console.log("[Overlay] Mounting...");

    // Enable screen capture protection
    try {
      await invoke("set_screen_share_protection", { enabled: true });
    } catch (e) {
      console.error("Failed to set screen share protection:", e);
    }

    // Listen for real-time suggestion events (pushed from backend)
    unlistenSuggestion = await listen<RealtimeSuggestion>("realtime-suggestion", (event) => {
      console.log("[Overlay] Received suggestion:", event.payload);
      suggestion = event.payload;
      lastUpdate = Date.now();
      isLoading = false;
    });

    // Listen for transcription events (to show activity indicator)
    unlistenTranscription = await listen("transcription", (event: any) => {
      if (event.payload.is_final) {
        transcriptCount++;
        // Show loading indicator when we expect a suggestion soon
        if (event.payload.is_turn_complete || transcriptCount % 5 === 0) {
          isLoading = true;
        }
      }
    });

    // Listen for recording stopped event to close overlay
    unlistenRecordingStopped = await listen("recording-stopped", async () => {
      console.log("[Overlay] Recording stopped, closing...");
      try {
        const window = getCurrentWindow();
        await window.close();
      } catch (e) {
        console.error("[Overlay] Failed to close window:", e);
      }
    });

    console.log("[Overlay] Event listeners set up");

    // Immediately fetch first suggestion on mount
    isLoading = true;
    fetchSuggestions();
  });

  onDestroy(() => {
    if (unlistenSuggestion) unlistenSuggestion();
    if (unlistenTranscription) unlistenTranscription();
    if (unlistenRecordingStopped) unlistenRecordingStopped();
  });

  // Manual refresh (fallback)
  async function fetchSuggestions() {
    if (isLoading) return;

    try {
      isLoading = true;
      const result = await invoke<RealtimeSuggestion>("get_realtime_suggestions", {
        meetingContext: null
      });

      if (result.insight || result.question || result.related_info) {
        suggestion = result;
        lastUpdate = Date.now();
      }
    } catch (e) {
      console.error("Failed to fetch suggestions:", e);
    } finally {
      isLoading = false;
    }
  }

  async function closeOverlay() {
    console.log("[Overlay] Close button clicked");
    try {
      const window = getCurrentWindow();
      await window.close();
    } catch (e) {
      console.error("[Overlay] Failed to close:", e);
    }
  }

  function toggleMinimize() {
    isMinimized = !isMinimized;
  }

  function formatTime(timestamp: number): string {
    const now = Date.now();
    const diff = Math.floor((now - timestamp) / 1000);
    if (diff < 60) return "just now";
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    return `${Math.floor(diff / 3600)}h ago`;
  }
</script>

<div class="h-screen w-screen bg-[#1a1a1a] overflow-hidden font-sans">
  <!-- Header bar with drag region - padding-left for macOS traffic lights -->
  <div class="flex items-center justify-between pl-[70px] pr-3 py-2 bg-[#252525] border-b border-[#333333] cursor-move" data-tauri-drag-region>
    <div class="flex items-center gap-2" data-tauri-drag-region>
      <div class="w-2 h-2 rounded-full bg-red-500 animate-pulse"></div>
      <span class="text-xs font-medium text-[#9B9B9B] uppercase tracking-wider" data-tauri-drag-region>AI Assistant</span>
    </div>
    <div class="flex items-center gap-1">
      <button
        class="p-1 hover:bg-[#333333] rounded text-[#666666] hover:text-[#EBEBEB] transition-colors"
        onclick={toggleMinimize}
        title={isMinimized ? "Expand" : "Minimize"}
      >
        <Minimize2 size={14} />
      </button>
    </div>
  </div>

  {#if !isMinimized}
    <div class="p-4 overflow-y-auto max-h-[calc(100vh-44px)]">
      {#if isLoading && !suggestion}
        <div class="flex items-center justify-center gap-2 py-8 text-[#666666]">
          <div class="w-4 h-4 border-2 border-indigo-500/30 border-t-indigo-500 rounded-full animate-spin"></div>
          <span class="text-sm">Analyzing conversation...</span>
        </div>
      {:else if suggestion && (suggestion.insight || suggestion.question || suggestion.related_info)}
        <div class="space-y-4">
          <!-- Insight -->
          {#if suggestion.insight}
            <div class="bg-gradient-to-br from-amber-500/10 to-amber-600/5 border border-amber-500/20 rounded-lg p-3">
              <div class="flex items-center gap-2 mb-2">
                <Lightbulb size={14} class="text-amber-400" />
                <span class="text-[10px] font-bold text-amber-400 uppercase tracking-wider">Insight</span>
              </div>
              <p class="text-sm text-[#CCCCCC] leading-relaxed">{suggestion.insight}</p>
            </div>
          {/if}

          <!-- Suggested Question -->
          {#if suggestion.question}
            <div class="bg-gradient-to-br from-emerald-500/10 to-emerald-600/5 border border-emerald-500/20 rounded-lg p-3">
              <div class="flex items-center gap-2 mb-2">
                <HelpCircle size={14} class="text-emerald-400" />
                <span class="text-[10px] font-bold text-emerald-400 uppercase tracking-wider">Ask This</span>
              </div>
              <p class="text-sm text-[#CCCCCC] leading-relaxed italic">"{suggestion.question}"</p>
            </div>
          {/if}

          <!-- Related Info -->
          {#if suggestion.related_info}
            <div class="bg-gradient-to-br from-blue-500/10 to-blue-600/5 border border-blue-500/20 rounded-lg p-3">
              <div class="flex items-center gap-2 mb-2">
                <LinkIcon size={14} class="text-blue-400" />
                <span class="text-[10px] font-bold text-blue-400 uppercase tracking-wider">Related</span>
              </div>
              <p class="text-sm text-[#AAAAAA] leading-relaxed">{suggestion.related_info}</p>
            </div>
          {/if}

          <!-- Last updated -->
          <div class="flex items-center justify-between pt-2 border-t border-[#333333]">
            <div class="flex items-center gap-2">
              {#if isLoading}
                <div class="w-3 h-3 border-2 border-indigo-500/30 border-t-indigo-500 rounded-full animate-spin"></div>
                <span class="text-[10px] text-indigo-400">Generating...</span>
              {:else if lastUpdate > 0}
                <span class="text-[10px] text-[#555555]">Updated {formatTime(lastUpdate)}</span>
              {/if}
            </div>
            <button
              class="flex items-center gap-1 text-[10px] text-[#666666] hover:text-indigo-400 transition-colors disabled:opacity-50"
              onclick={fetchSuggestions}
              disabled={isLoading}
              title="Manual refresh"
            >
              <RefreshCw size={10} class={isLoading ? "animate-spin" : ""} />
            </button>
          </div>
        </div>
      {:else}
        <div class="flex flex-col items-center justify-center gap-3 py-8 text-center">
          <Sparkles size={24} class="text-[#444444]" />
          <div>
            <p class="text-sm text-[#666666]">Listening to your meeting...</p>
            <p class="text-xs text-[#555555] mt-1">AI suggestions will appear here</p>
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <!-- Minimized state - just a small indicator -->
    <div class="p-2 flex items-center justify-center">
      <div class="flex items-center gap-2">
        <Sparkles size={14} class="text-indigo-400" />
        {#if suggestion?.insight || suggestion?.question}
          <div class="w-2 h-2 rounded-full bg-green-500 animate-pulse" title="New suggestion available"></div>
        {/if}
      </div>
    </div>
  {/if}
</div>
