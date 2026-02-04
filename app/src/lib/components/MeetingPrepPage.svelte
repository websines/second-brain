<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import {
    ArrowLeft,
    FileText,
    Link as LinkIcon,
    Plus,
    Mic,
    X,
    Trash2,
    Upload
  } from "lucide-svelte";

  interface KnowledgeSource {
    id?: { tb: string; id: { String: string } };
    url: string;
    title: string;
    source_type: string;
    tags: string[];
    created_at: number;
  }

  // Helper to get string ID from SurrealDB Thing
  function getSourceId(source: KnowledgeSource): string {
    if (!source.id) return '';
    return `${source.id.tb}:${source.id.id.String}`;
  }

  let {
    meetingId,
    initialTitle = "",
    onCancel,
    onStartRecording
  }: {
    meetingId: string;
    initialTitle: string;
    onCancel: () => void;
    onStartRecording: () => void;
  } = $props();

  let title = $state(initialTitle);
  let context = $state("");
  let linkedSources = $state<KnowledgeSource[]>([]);
  let availableSources = $state<KnowledgeSource[]>([]);
  let isLoading = $state(false);
  let showSourcePicker = $state(false);

  // Load available sources on mount
  $effect(() => {
    loadAvailableSources();
  });

  async function loadAvailableSources() {
    try {
      availableSources = await invoke<KnowledgeSource[]>("get_knowledge_sources", { limit: 100 });
    } catch (e) {
      console.error("Failed to load knowledge sources:", e);
      availableSources = [];
    }
  }

  async function linkSource(source: KnowledgeSource) {
    const sourceId = getSourceId(source);
    if (!sourceId) return;

    try {
      await invoke("link_knowledge_to_meeting", {
        meetingId: meetingId,
        sourceId: sourceId
      });
      if (!linkedSources.find(s => getSourceId(s) === sourceId)) {
        linkedSources = [...linkedSources, source];
      }
      showSourcePicker = false;
    } catch (e) {
      console.error("Failed to link source:", e);
    }
  }

  function unlinkSource(source: KnowledgeSource) {
    const sourceId = getSourceId(source);
    linkedSources = linkedSources.filter(s => getSourceId(s) !== sourceId);
  }

  async function handleCancel() {
    try {
      // Delete the meeting since we're canceling prep
      await invoke("end_meeting", { summary: null });
    } catch (e) {
      console.error("Failed to cancel meeting:", e);
    }
    onCancel();
  }

  async function openSuggestionsOverlay() {
    try {
      const existing = await WebviewWindow.getByLabel('suggestions-overlay');
      if (existing) {
        console.log("Overlay already exists, focusing...");
        await existing.setFocus();
        return;
      }

      console.log("Creating new suggestions overlay window...");
      const overlay = new WebviewWindow('suggestions-overlay', {
        url: '/overlay',
        title: 'AI Assistant',
        width: 320,
        height: 400,
        minWidth: 280,
        minHeight: 200,
        resizable: true,
        decorations: true,
        titleBarStyle: 'overlay',
        hiddenTitle: true,
        alwaysOnTop: true,
        x: 50,
        y: 100,
        focus: false,
      });

      overlay.once('tauri://created', () => {
        console.log("Suggestions overlay window created successfully");
      });

      overlay.once('tauri://error', (e) => {
        console.error("Failed to create overlay window:", e);
      });
    } catch (e) {
      console.error("Failed to open suggestions overlay:", e);
    }
  }

  async function handleStartRecording() {
    try {
      isLoading = true;
      // Update meeting title if changed
      // TODO: Add update_meeting_title command if needed

      // Build and set meeting context for AI suggestions
      let meetingContext = "";
      if (context.trim()) {
        meetingContext += `MEETING AGENDA/NOTES:\n${context}\n\n`;
      }
      if (linkedSources.length > 0) {
        meetingContext += `LINKED DOCUMENTS:\n`;
        for (const source of linkedSources) {
          meetingContext += `- ${source.title} (${source.source_type})\n`;
        }
      }
      if (meetingContext) {
        await invoke("set_meeting_context", { context: meetingContext });
      }

      // Start recording
      await invoke("start_recording");

      // Open floating suggestions overlay
      await openSuggestionsOverlay();

      onStartRecording();
    } catch (e) {
      console.error("Failed to start recording:", e);
      isLoading = false;
    }
  }

  // Filter out already linked sources
  let unlinkedSources = $derived(
    availableSources.filter(s => !linkedSources.find(l => getSourceId(l) === getSourceId(s)))
  );
</script>

<div class="h-full bg-zinc-950 overflow-y-auto">
  <div class="max-w-3xl mx-auto p-8">
    <!-- Header -->
    <div class="flex items-center justify-between mb-8">
      <button
        class="flex items-center gap-2 text-[#9B9B9B] hover:text-[#EBEBEB] transition-colors"
        onclick={handleCancel}
      >
        <ArrowLeft size={18} />
        <span class="text-sm">Cancel</span>
      </button>
      <div class="flex items-center gap-2">
        <div class="w-2 h-2 rounded-full bg-amber-500"></div>
        <span class="text-xs font-medium text-amber-500 uppercase tracking-wider">Preparing</span>
      </div>
    </div>

    <!-- Title -->
    <div class="mb-8">
      <label class="text-xs font-medium text-[#666666] uppercase tracking-wider block mb-2">Meeting Title</label>
      <input
        type="text"
        bind:value={title}
        class="w-full bg-[#252525] border border-[#333333] rounded-lg px-4 py-3 text-xl font-medium text-[#EBEBEB] placeholder:text-[#555555] focus:outline-none focus:border-[#555555]"
        placeholder="Enter meeting title..."
      />
    </div>

    <!-- Context/Notes -->
    <div class="mb-8">
      <label class="text-xs font-medium text-[#666666] uppercase tracking-wider block mb-2">
        Meeting Context & Agenda
      </label>
      <textarea
        bind:value={context}
        class="w-full bg-[#252525] border border-[#333333] rounded-lg px-4 py-3 text-sm text-[#EBEBEB] placeholder:text-[#555555] resize-none focus:outline-none focus:border-[#555555]"
        rows="6"
        placeholder="Add notes, agenda items, topics to discuss, or any context that will help during the meeting..."
      ></textarea>
      <p class="text-xs text-[#666666] mt-2">This context will be used to provide relevant suggestions during the meeting.</p>
    </div>

    <!-- Linked Documents -->
    <div class="mb-8">
      <div class="flex items-center justify-between mb-3">
        <label class="text-xs font-medium text-[#666666] uppercase tracking-wider">
          Linked Documents
        </label>
        <button
          class="flex items-center gap-1.5 text-xs text-[#9B9B9B] hover:text-[#EBEBEB] transition-colors"
          onclick={() => showSourcePicker = !showSourcePicker}
        >
          <Plus size={14} />
          Add Document
        </button>
      </div>

      {#if linkedSources.length > 0}
        <div class="space-y-2 mb-4">
          {#each linkedSources as source}
            <div class="flex items-center justify-between bg-[#252525] border border-[#333333] rounded-lg px-4 py-3 group">
              <div class="flex items-center gap-3">
                <FileText size={16} class="text-[#666666]" />
                <div>
                  <p class="text-sm text-[#EBEBEB]">{source.title}</p>
                  <p class="text-xs text-[#666666]">{source.source_type}</p>
                </div>
              </div>
              <button
                class="text-[#666666] hover:text-red-400 opacity-0 group-hover:opacity-100 transition-all"
                onclick={() => unlinkSource(source)}
              >
                <X size={16} />
              </button>
            </div>
          {/each}
        </div>
      {:else}
        <div class="border-2 border-dashed border-[#333333] rounded-lg p-6 text-center mb-4">
          <LinkIcon size={24} class="text-[#444444] mx-auto mb-2" />
          <p class="text-sm text-[#666666]">No documents linked yet</p>
          <p class="text-xs text-[#555555] mt-1">Add relevant documents to get context-aware suggestions</p>
        </div>
      {/if}

      <!-- Source Picker -->
      {#if showSourcePicker}
        <div class="bg-[#1F1F1F] border border-[#333333] rounded-lg p-4 max-h-64 overflow-y-auto">
          <div class="flex items-center justify-between mb-3">
            <span class="text-xs font-medium text-[#9B9B9B]">Select from Knowledge Base</span>
            <button
              class="text-[#666666] hover:text-[#EBEBEB]"
              onclick={() => showSourcePicker = false}
            >
              <X size={14} />
            </button>
          </div>
          {#if unlinkedSources.length > 0}
            <div class="space-y-1">
              {#each unlinkedSources as source}
                <button
                  class="w-full text-left flex items-center gap-3 px-3 py-2 rounded hover:bg-[#2C2C2C] transition-colors"
                  onclick={() => linkSource(source)}
                >
                  <FileText size={14} class="text-[#666666]" />
                  <div class="flex-1 min-w-0">
                    <p class="text-sm text-[#EBEBEB] truncate">{source.title}</p>
                    <p class="text-xs text-[#666666]">{source.source_type}</p>
                  </div>
                  <Plus size={14} class="text-[#666666]" />
                </button>
              {/each}
            </div>
          {:else}
            <p class="text-sm text-[#666666] text-center py-4">No documents in Knowledge Base. Add some first.</p>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Actions -->
    <div class="flex items-center justify-between pt-6 border-t border-[#333333]">
      <button
        class="flex items-center gap-2 px-4 py-2 text-[#9B9B9B] hover:text-red-400 transition-colors"
        onclick={handleCancel}
      >
        <Trash2 size={16} />
        <span class="text-sm">Cancel Meeting</span>
      </button>

      <button
        class="flex items-center gap-2 px-6 py-3 bg-[#EBEBEB] hover:bg-white text-black font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        onclick={handleStartRecording}
        disabled={isLoading || !title.trim()}
      >
        <div class="w-2.5 h-2.5 rounded-full bg-red-500"></div>
        <span>{isLoading ? 'Starting...' : 'Start Recording'}</span>
      </button>
    </div>
  </div>
</div>
