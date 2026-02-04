<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Search, Bot, CheckSquare, Play, Square, AlertCircle, Loader2 } from "lucide-svelte";

  // Props
  let {
    isRecording = false,
    onMeetingStart = () => {},
    onMeetingEnd = () => {},
  }: {
    isRecording?: boolean;
    onMeetingStart?: (meetingId: string) => void;
    onMeetingEnd?: () => void;
  } = $props();

  // State
  let activeTab = $state<'search' | 'ask' | 'actions'>('search');
  let meetingTitle = $state("");
  let currentMeetingId = $state<string | null>(null);
  let searchQuery = $state("");
  let askQuery = $state("");
  let searchResults = $state<SearchResult[]>([]);
  let assistantResponse = $state("");
  let actionItems = $state<ActionItem[]>([]);
  let isLoading = $state(false);
  let error = $state("");
  let kbInitialized = $state(false);
  let llmInitialized = $state(false);

  // Types
  interface SearchResult {
    segment: {
      meeting_id: string;
      speaker: string;
      text: string;
      start_ms: number;
      end_ms: number;
    };
    meeting_title: string;
    similarity: number;
  }

  interface ActionItem {
    id?: { tb: string; id: { String: string } };
    meeting_id: string;
    text: string;
    assignee?: string;
    deadline?: string;
    status: string;
    created_at: number;
  }

  // Initialize KB and LLM on mount
  $effect(() => {
    initializeServices();
  });

  async function initializeServices() {
    try {
      // Initialize entity extraction
      await invoke("initialize_entities");
      console.log("Entity engine initialized");
    } catch (e) {
      console.warn("Entity init skipped:", e);
    }

    try {
      // Initialize embeddings
      await invoke("initialize_embeddings");
      console.log("Embedding engine initialized");
    } catch (e) {
      console.warn("Embedding init skipped:", e);
    }

    try {
      // Initialize knowledge base
      await invoke("initialize_knowledge_base");
      kbInitialized = true;
      console.log("Knowledge base initialized");
    } catch (e) {
      console.warn("KB init skipped:", e);
    }

    try {
      // Initialize LLM with self-hosted endpoint
      await invoke("initialize_llm", {
        url: "https://lmstudio.subh-dev.xyz/llm/v1",
        modelName: "openai/gpt-oss-20b"
      });
      llmInitialized = true;
      console.log("LLM assistant initialized");
    } catch (e) {
      console.warn("LLM init skipped:", e);
    }
  }

  async function startMeeting() {
    if (!meetingTitle.trim()) {
      error = "Please enter a meeting title";
      return;
    }

    try {
      isLoading = true;
      error = "";
      const meetingId = await invoke<string>("start_meeting", {
        title: meetingTitle,
        participants: []
      });
      currentMeetingId = meetingId;
      onMeetingStart(meetingId);
    } catch (e) {
      error = `Failed to start meeting: ${e}`;
    } finally {
      isLoading = false;
    }
  }

  async function endMeeting() {
    if (!currentMeetingId) return;

    try {
      isLoading = true;
      error = "";
      await invoke("end_meeting", { summary: null });
      currentMeetingId = null;
      meetingTitle = "";
      onMeetingEnd();
    } catch (e) {
      error = `Failed to end meeting: ${e}`;
    } finally {
      isLoading = false;
    }
  }

  async function search() {
    if (!searchQuery.trim()) return;

    try {
      isLoading = true;
      error = "";
      searchResults = await invoke<SearchResult[]>("search_knowledge", {
        query: searchQuery,
        limit: 10
      });
    } catch (e) {
      error = `Search failed: ${e}`;
      searchResults = [];
    } finally {
      isLoading = false;
    }
  }

  async function askAssistant() {
    if (!askQuery.trim()) return;

    try {
      isLoading = true;
      error = "";
      assistantResponse = await invoke<string>("ask_assistant", {
        question: askQuery
      });
    } catch (e) {
      error = `Ask failed: ${e}`;
      assistantResponse = "";
    } finally {
      isLoading = false;
    }
  }

  async function loadActionItems() {
    try {
      isLoading = true;
      error = "";
      actionItems = await invoke<ActionItem[]>("get_action_items");
    } catch (e) {
      error = `Failed to load actions: ${e}`;
      actionItems = [];
    } finally {
      isLoading = false;
    }
  }

  function formatTimestamp(ms: number): string {
    const date = new Date(ms);
    return date.toLocaleString();
  }
</script>

<div class="bg-[#252525] border border-[#333333] rounded-xl p-4 flex flex-col gap-4 relative">
  <!-- Meeting Controls -->
  <div class="pb-3 border-b border-[#333333]">
    {#if !currentMeetingId}
      <div class="flex gap-2 items-center">
        <input
          type="text"
          bind:value={meetingTitle}
          placeholder="Meeting title..."
          class="flex-1 px-3 py-2 text-sm bg-[#1F1F1F] border border-[#333333] rounded-lg text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
          disabled={isLoading}
        />
        <button
          class="flex items-center gap-2 px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white text-sm font-medium rounded-lg transition-colors disabled:opacity-50"
          onclick={startMeeting}
          disabled={isLoading || !kbInitialized}
        >
          <Play size={16} />
          Start
        </button>
      </div>
    {:else}
      <div class="flex gap-2 items-center justify-between">
        <div class="flex items-center gap-2 px-3 py-1.5 bg-green-500/10 rounded-full text-green-400 border border-green-500/20">
          <div class="w-2 h-2 bg-green-500 rounded-full animate-pulse"></div>
          <span class="text-sm font-medium">{meetingTitle}</span>
        </div>
        <button
          class="flex items-center gap-2 px-4 py-2 bg-red-500/10 hover:bg-red-500/20 text-red-400 border border-red-500/20 text-sm font-medium rounded-lg transition-colors disabled:opacity-50"
          onclick={endMeeting}
          disabled={isLoading}
        >
          <Square size={16} />
          End Meeting
        </button>
      </div>
    {/if}
  </div>

  <!-- Tabs -->
  <div class="flex gap-1 p-1 bg-[#1F1F1F] rounded-lg border border-[#333333]">
    <button
      class="flex-1 py-1.5 px-3 text-sm font-medium rounded-md transition-colors flex items-center justify-center gap-2 {activeTab === 'search' ? 'bg-[#333333] text-[#EBEBEB] shadow-sm' : 'text-[#9B9B9B] hover:text-[#EBEBEB]'}"
      onclick={() => activeTab = 'search'}
    >
      <Search size={14} /> Search
    </button>
    <button
      class="flex-1 py-1.5 px-3 text-sm font-medium rounded-md transition-colors flex items-center justify-center gap-2 {activeTab === 'ask' ? 'bg-[#333333] text-[#EBEBEB] shadow-sm' : 'text-[#9B9B9B] hover:text-[#EBEBEB]'}"
      onclick={() => activeTab = 'ask'}
      disabled={!llmInitialized}
    >
      <Bot size={14} /> Ask AI
    </button>
    <button
      class="flex-1 py-1.5 px-3 text-sm font-medium rounded-md transition-colors flex items-center justify-center gap-2 {activeTab === 'actions' ? 'bg-[#333333] text-[#EBEBEB] shadow-sm' : 'text-[#9B9B9B] hover:text-[#EBEBEB]'}"
      onclick={() => { activeTab = 'actions'; loadActionItems(); }}
    >
      <CheckSquare size={14} /> Actions
    </button>
  </div>

  <!-- Tab Content -->
  <div class="min-h-[200px]">
    {#if activeTab === 'search'}
      <div class="flex flex-col gap-3">
        <div class="flex gap-2">
          <input
            type="text"
            bind:value={searchQuery}
            placeholder="Search past meetings..."
            class="flex-1 px-3 py-2 text-sm bg-[#1F1F1F] border border-[#333333] rounded-lg text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
            onkeydown={(e) => e.key === 'Enter' && search()}
            disabled={isLoading || !kbInitialized}
          />
          <button
            class="px-4 py-2 bg-[#EBEBEB] hover:bg-white text-black text-sm font-medium rounded-lg transition-colors disabled:opacity-50"
            onclick={search}
            disabled={isLoading || !kbInitialized}
          >
            Search
          </button>
        </div>

        {#if searchResults.length > 0}
          <div class="flex flex-col gap-2 max-h-[300px] overflow-y-auto pr-1">
            {#each searchResults as result}
              <div class="bg-[#1F1F1F] border border-[#333333] rounded-lg p-3 hover:border-[#444444] transition-colors">
                <div class="flex justify-between items-center mb-2">
                  <span class="text-xs font-medium text-indigo-400">{result.meeting_title}</span>
                  <span class="text-[10px] text-[#666666] uppercase">{result.segment.speaker}</span>
                </div>
                <p class="text-sm text-[#CCCCCC] leading-relaxed m-0">{result.segment.text}</p>
              </div>
            {/each}
          </div>
        {:else if !isLoading && searchQuery}
          <p class="text-center text-[#666666] py-10 text-sm">No results found</p>
        {/if}
      </div>

    {:else if activeTab === 'ask'}
      <div class="flex flex-col gap-3">
        <div class="flex gap-2">
          <input
            type="text"
            bind:value={askQuery}
            placeholder="Ask about past meetings..."
            class="flex-1 px-3 py-2 text-sm bg-[#1F1F1F] border border-[#333333] rounded-lg text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
            onkeydown={(e) => e.key === 'Enter' && askAssistant()}
            disabled={isLoading || !llmInitialized}
          />
          <button
            class="px-4 py-2 bg-[#EBEBEB] hover:bg-white text-black text-sm font-medium rounded-lg transition-colors disabled:opacity-50"
            onclick={askAssistant}
            disabled={isLoading || !llmInitialized}
          >
            Ask
          </button>
        </div>

        {#if assistantResponse}
          <div class="bg-[#1F1F1F] border border-[#333333] rounded-lg p-4 mt-2">
            <p class="text-sm text-[#EBEBEB] leading-relaxed whitespace-pre-wrap m-0">{assistantResponse}</p>
          </div>
        {:else if !llmInitialized}
          <p class="text-center text-[#666666] py-10 text-sm px-8">LLM not initialized. Check your connection to the self-hosted endpoint.</p>
        {/if}
      </div>

    {:else if activeTab === 'actions'}
      <div class="flex flex-col gap-2">
        {#if actionItems.length > 0}
          <div class="flex flex-col gap-2 max-h-[300px] overflow-y-auto pr-1">
            {#each actionItems as action}
              <div class="bg-[#1F1F1F] border border-[#333333] rounded-lg p-3">
                <div class="text-sm text-[#EBEBEB] mb-2">{action.text}</div>
                <div class="flex flex-wrap gap-2">
                  <span class="text-[10px] px-1.5 py-0.5 rounded border border-[#333333] {action.status === 'open' ? 'bg-amber-500/10 text-amber-400 border-amber-500/20' : 'bg-[#252525] text-[#666666]'}">
                    {action.status}
                  </span>
                  {#if action.assignee}
                    <span class="text-[10px] text-[#666666] flex items-center gap-1">
                      <span class="w-1 h-1 rounded-full bg-[#666666]"></span> {action.assignee}
                    </span>
                  {/if}
                  {#if action.deadline}
                    <span class="text-[10px] text-[#666666] flex items-center gap-1">
                       <span class="w-1 h-1 rounded-full bg-[#666666]"></span> {action.deadline}
                    </span>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {:else if !isLoading}
          <p class="text-center text-[#666666] py-10 text-sm">No action items found</p>
        {/if}
      </div>
    {/if}
  </div>

  {#if error}
    <div class="flex items-center gap-2 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-500 text-sm">
      <AlertCircle size={16} /> {error}
    </div>
  {/if}

  {#if isLoading}
    <div class="absolute inset-0 flex items-center justify-center bg-black/60 rounded-xl z-20 backdrop-blur-sm">
      <Loader2 size={32} class="animate-spin text-indigo-500" />
    </div>
  {/if}

  {#if !kbInitialized}
    <div class="p-2 text-center bg-amber-500/10 border border-amber-500/20 rounded-lg text-amber-500 text-xs">
      Knowledge base not ready. Make sure all models are downloaded.
    </div>
  {/if}
</div>
