<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { readFile } from "@tauri-apps/plugin-fs";
  import { onMount } from "svelte";
  import { Book, Search, Bot, Plus, Globe, FileText, RefreshCw, Trash2, Sparkles, Filter, Upload, File } from "lucide-svelte";

  // Types
  interface KnowledgeSource {
    id?: { tb: string; id: { String: string } };
    url: string;
    title: string;
    source_type: string;
    raw_content: string;
    tags: string[];
    created_at: number;
    last_updated: number;
  }

  interface SearchResult {
    title: string;
    url: string;
    snippet: string;
  }

  interface KnowledgeSearchResult {
    chunk: {
      text: string;
      chunk_index: number;
    };
    source_title: string;
    source_url: string;
    similarity: number;
  }

  // State
  let activeTab = $state<'sources' | 'search' | 'ask' | 'add'>('sources');
  let sources = $state<KnowledgeSource[]>([]);
  let searchQuery = $state("");
  let webSearchQuery = $state("");
  let webSearchResults = $state<SearchResult[]>([]);
  let knowledgeSearchResults = $state<KnowledgeSearchResult[]>([]);
  let isLoading = $state(false);
  let error = $state("");
  let success = $state("");

  // Add source form
  let newUrl = $state("");
  let newTags = $state("");
  let filterTags = $state("");

  // Crawl status
  let isCrawling = $state(false);
  let crawlStatus = $state("");

  // File upload
  let isUploading = $state(false);
  let uploadStatus = $state("");

  // Initialization state
  let kbInitialized = $state(false);
  let initStatus = $state("");

  // AI Assistant state
  let aiQuestion = $state("");
  let aiAnswer = $state("");
  let isAiThinking = $state(false);
  let llmInitialized = $state(false);
  let llmInitializing = $state(false);
  let webSearchSummary = $state("");
  let isSummarizing = $state(false);

  // Initialize LLM
  async function initializeLLM() {
    if (llmInitialized || llmInitializing) return;

    try {
      llmInitializing = true;
      await invoke("initialize_llm");
      llmInitialized = true;
      console.log("LLM initialized");
    } catch (e) {
      console.error("Failed to initialize LLM:", e);
      error = `Failed to initialize LLM: ${e}`;
    } finally {
      llmInitializing = false;
    }
  }

  // Summarize web search results with AI
  async function summarizeWebSearch() {
    if (webSearchResults.length === 0 || isSummarizing) return;

    // Initialize LLM if needed
    if (!llmInitialized) {
      await initializeLLM();
      if (!llmInitialized) return;
    }

    try {
      isSummarizing = true;
      error = "";
      webSearchSummary = "";

      // Format search results for the LLM
      const resultsText = webSearchResults
        .map((r, i) => `${i + 1}. ${r.title}\nURL: ${r.url}\nSnippet: ${r.snippet || 'No snippet available'}`)
        .join('\n\n');

      const prompt = `Based on these web search results for "${webSearchQuery}", provide a concise summary of the key information found. Highlight the most relevant points.\n\nSearch Results:\n${resultsText}`;

      const response = await invoke<string>("ask_assistant", {
        question: prompt
      });

      webSearchSummary = response;
    } catch (e) {
      error = `Failed to summarize: ${e}`;
      webSearchSummary = "";
    } finally {
      isSummarizing = false;
    }
  }

  // Ask AI a question about the knowledge base
  async function askAI() {
    if (!aiQuestion.trim() || isAiThinking) return;

    // Ensure knowledge base is initialized first
    if (!kbInitialized) {
      error = "Knowledge base is not initialized yet. Please wait for initialization to complete.";
      return;
    }

    // Initialize LLM if needed
    if (!llmInitialized) {
      await initializeLLM();
      if (!llmInitialized) return;
    }

    try {
      isAiThinking = true;
      error = "";
      aiAnswer = "";

      const response = await invoke<string>("ask_assistant", {
        question: aiQuestion
      });

      aiAnswer = response;
    } catch (e) {
      error = `AI query failed: ${e}`;
      aiAnswer = "";
    } finally {
      isAiThinking = false;
    }
  }

  // Load sources on mount
  onMount(async () => {
    await initializeKB();
  });

  async function initializeKB() {
    error = "";

    // Entity engine is optional (for NER)
    try {
      initStatus = "Initializing entity engine...";
      await invoke("initialize_entities");
      console.log("Entity engine initialized");
    } catch (e) {
      console.warn("Entity init skipped (optional):", e);
    }

    // Embeddings are REQUIRED for KB
    try {
      initStatus = "Initializing embeddings (this may take a moment)...";
      await invoke("initialize_embeddings");
      console.log("Embeddings initialized");
    } catch (e) {
      error = `Failed to initialize embeddings: ${e}. Make sure models are downloaded.`;
      initStatus = "";
      return; // Can't continue without embeddings
    }

    // Knowledge base requires embeddings
    try {
      initStatus = "Initializing knowledge base...";
      await invoke("initialize_knowledge_base");
      kbInitialized = true;
      initStatus = "";
      console.log("Knowledge base initialized");
      await loadSources();
    } catch (e) {
      error = `Failed to initialize knowledge base: ${e}`;
      initStatus = "";
    }
  }

  async function loadSources() {
    try {
      isLoading = true;
      error = "";
      const tagsArray = filterTags.trim()
        ? filterTags.split(',').map(t => t.trim()).filter(t => t)
        : null;
      sources = await invoke<KnowledgeSource[]>("get_knowledge_sources", {
        tags: tagsArray
      });
    } catch (e) {
      error = `Failed to load sources: ${e}`;
      sources = [];
    } finally {
      isLoading = false;
    }
  }

  async function searchWeb() {
    if (!webSearchQuery.trim()) return;

    try {
      isLoading = true;
      error = "";
      webSearchSummary = ""; // Clear previous summary
      webSearchResults = await invoke<SearchResult[]>("search_web", {
        query: webSearchQuery,
        limit: 10
      });
    } catch (e) {
      error = `Web search failed: ${e}`;
      webSearchResults = [];
    } finally {
      isLoading = false;
    }
  }

  async function searchKnowledge() {
    if (!searchQuery.trim()) return;

    try {
      isLoading = true;
      error = "";
      const tagsArray = filterTags.trim()
        ? filterTags.split(',').map(t => t.trim()).filter(t => t)
        : null;
      knowledgeSearchResults = await invoke<KnowledgeSearchResult[]>("search_knowledge_chunks", {
        query: searchQuery,
        limit: 10,
        tags: tagsArray
      });
    } catch (e) {
      error = `Knowledge search failed: ${e}`;
      knowledgeSearchResults = [];
    } finally {
      isLoading = false;
    }
  }

  async function crawlAndStore(url: string) {
    try {
      isCrawling = true;
      crawlStatus = `Crawling ${url}...`;
      error = "";
      const tagsArray = newTags.trim()
        ? newTags.split(',').map(t => t.trim()).filter(t => t)
        : [];

      const sourceId = await invoke<string>("crawl_and_store", {
        url,
        tags: tagsArray
      });

      success = `Added: ${url}`;
      crawlStatus = "";
      newUrl = "";
      newTags = "";
      await loadSources();
      activeTab = 'sources';
    } catch (e) {
      error = `Failed to crawl: ${e}`;
      crawlStatus = "";
    } finally {
      isCrawling = false;
    }
  }

  async function deleteSource(sourceId: string) {
    if (!confirm("Delete this source and all its chunks?")) return;

    try {
      isLoading = true;
      error = "";
      await invoke("delete_knowledge_source", { sourceId });
      success = "Source deleted";
      await loadSources();
    } catch (e) {
      error = `Failed to delete: ${e}`;
    } finally {
      isLoading = false;
    }
  }

  async function cleanupOrphanedChunks() {
    if (!confirm("This will delete all chunks that reference deleted sources. Continue?")) return;

    try {
      isLoading = true;
      error = "";
      const count = await invoke<number>("cleanup_orphaned_chunks");
      success = `Cleaned up ${count} orphaned source groups`;
    } catch (e) {
      error = `Failed to cleanup: ${e}`;
    } finally {
      isLoading = false;
    }
  }

  function formatDate(timestamp: number): string {
    return new Date(timestamp).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric'
    });
  }

  function getSourceIcon(type: string) {
    switch (type) {
      case 'web': return Globe;
      case 'url': return Globe;
      case 'file': return FileText;
      case 'pdf': return FileText; // Could use distinct icon if available
      case 'search': return Search;
      default: return File;
    }
  }

  async function uploadDocument() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'Documents',
          extensions: ['pdf', 'txt', 'md', 'markdown']
        }]
      });

      if (!selected) return;

      isUploading = true;
      uploadStatus = "Reading file...";
      error = "";

      const filePath = selected as string;
      const fileName = filePath.split('/').pop() || filePath;
      const extension = fileName.split('.').pop()?.toLowerCase();

      uploadStatus = "Processing document...";

      const tagsArray = newTags.trim()
        ? newTags.split(',').map(t => t.trim()).filter(t => t)
        : [];

      // Call Rust to process the document
      const sourceId = await invoke<string>("upload_document", {
        filePath,
        tags: tagsArray
      });

      success = `Uploaded: ${fileName}`;
      uploadStatus = "";
      newTags = "";
      await loadSources();
      activeTab = 'sources';
    } catch (e) {
      error = `Failed to upload: ${e}`;
      uploadStatus = "";
    } finally {
      isUploading = false;
    }
  }

  // Auto-clear messages
  $effect(() => {
    if (success) {
      setTimeout(() => success = "", 3000);
    }
  });
</script>

<div class="max-w-6xl mx-auto p-8 h-full flex flex-col bg-[#1F1F1F]">
  <!-- Initialization Banner -->
  {#if initStatus}
    <div class="flex items-center gap-3 p-4 bg-indigo-500/10 border border-indigo-500/20 rounded-lg mb-6 text-indigo-400 text-sm">
      <div class="w-4 h-4 border-2 border-indigo-500/30 border-t-indigo-500 rounded-full animate-spin"></div>
      <span>{initStatus}</span>
    </div>
  {/if}

  {#if !kbInitialized && !initStatus}
    <div class="flex items-center justify-between p-4 bg-amber-500/10 border border-amber-500/20 rounded-lg mb-6 text-amber-500 text-sm">
      <span>Knowledge base not initialized. Models may need to be downloaded first.</span>
      <button class="px-3 py-1.5 border border-amber-500/30 rounded hover:bg-amber-500/10 transition-colors" onclick={initializeKB}>Retry</button>
    </div>
  {/if}

  <!-- Header -->
  <header class="flex justify-between items-start mb-8 border-b border-[#2C2C2C] pb-6">
    <div>
      <h1 class="text-2xl font-semibold text-[#EBEBEB]">Knowledge Base</h1>
      <p class="text-[#9B9B9B] text-sm mt-1">Store and search web content and documents</p>
    </div>
    <div class="flex items-center gap-3">
      <div class="relative">
        <Filter size={14} class="absolute left-3 top-1/2 -translate-y-1/2 text-[#666666]" />
        <input
          type="text"
          placeholder="Filter by tags..."
          class="bg-[#252525] border border-transparent rounded px-3 py-1.5 pl-9 text-sm text-[#EBEBEB] w-48 focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
          bind:value={filterTags}
          onchange={loadSources}
        />
      </div>
      <button
        class="p-1.5 bg-[#252525] border border-transparent rounded hover:bg-[#333333] text-[#9B9B9B] hover:text-[#EBEBEB] transition-colors"
        onclick={loadSources}
        title="Refresh"
      >
        <RefreshCw size={16} />
      </button>
    </div>
  </header>

  <!-- Tabs -->
  <div class="flex gap-1 p-1 bg-[#252525] rounded-lg mb-6 w-fit border border-[#2C2C2C]">
    <button
      class="px-4 py-2 text-sm font-medium rounded-md transition-colors flex items-center gap-2 {activeTab === 'sources' ? 'bg-[#333333] text-[#EBEBEB] shadow-sm' : 'text-[#9B9B9B] hover:text-[#EBEBEB]'}"
      onclick={() => activeTab = 'sources'}
    >
      <Book size={16} />
      <span>Sources</span>
      <span class="text-xs bg-[#1F1F1F] px-1.5 py-0.5 rounded-full text-[#777777]">{sources.length}</span>
    </button>
    <button
      class="px-4 py-2 text-sm font-medium rounded-md transition-colors flex items-center gap-2 {activeTab === 'search' ? 'bg-[#333333] text-[#EBEBEB] shadow-sm' : 'text-[#9B9B9B] hover:text-[#EBEBEB]'}"
      onclick={() => activeTab = 'search'}
    >
      <Search size={16} />
      <span>Search</span>
    </button>
    <button
      class="px-4 py-2 text-sm font-medium rounded-md transition-colors flex items-center gap-2 {activeTab === 'ask' ? 'bg-[#333333] text-[#EBEBEB] shadow-sm' : 'text-[#9B9B9B] hover:text-[#EBEBEB]'}"
      onclick={() => activeTab = 'ask'}
    >
      <Bot size={16} />
      <span>Ask AI</span>
    </button>
    <button
      class="px-4 py-2 text-sm font-medium rounded-md transition-colors flex items-center gap-2 {activeTab === 'add' ? 'bg-[#333333] text-[#EBEBEB] shadow-sm' : 'text-[#9B9B9B] hover:text-[#EBEBEB]'}"
      onclick={() => activeTab = 'add'}
    >
      <Plus size={16} />
      <span>Add Source</span>
    </button>
  </div>

  <!-- Messages -->
  {#if error}
    <div class="p-3 mb-4 rounded bg-red-500/10 border border-red-500/20 text-red-500 text-sm">{error}</div>
  {/if}
  {#if success}
    <div class="p-3 mb-4 rounded bg-green-500/10 border border-green-500/20 text-green-500 text-sm">{success}</div>
  {/if}

  <!-- Tab Content -->
  <div class="flex-1">
    {#if activeTab === 'sources'}
      <!-- Sources List -->
      <div class="flex justify-end mb-4">
        <button
          class="flex items-center gap-2 px-3 py-1.5 text-xs font-medium rounded bg-[#252525] border border-transparent text-[#9B9B9B] hover:text-[#EBEBEB] hover:bg-[#333333] transition-colors disabled:opacity-50"
          onclick={cleanupOrphanedChunks}
          disabled={isLoading}
          title="Remove orphaned chunks from deleted sources"
        >
          <Trash2 size={12} /> Cleanup Orphans
        </button>
      </div>
      
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {#if sources.length > 0}
          {#each sources as source}
            {@const Icon = getSourceIcon(source.source_type)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="bg-[#252525] border border-transparent rounded-lg p-4 hover:bg-[#2C2C2C] transition-all group">
              <div class="flex items-start gap-3 mb-3">
                  <div class="text-[#9B9B9B]">
                    <Icon size={20} />
                  </div>
                <div class="flex-1 min-w-0">
                  <h3 class="text-sm font-medium text-[#EBEBEB] truncate" title={source.title}>{source.title}</h3>
                  <a href={source.url} target="_blank" rel="noopener" class="text-xs text-[#777777] hover:text-[#EBEBEB] hover:underline truncate block">
                    {source.url}
                  </a>
                </div>
                <button
                  class="p-1 rounded hover:bg-red-500/10 text-[#666666] hover:text-red-500 opacity-0 group-hover:opacity-100 transition-all"
                  onclick={() => deleteSource(source.id?.id?.String || '')}
                  title="Delete"
                >
                  <Trash2 size={14} />
                </button>
              </div>
              <div class="flex items-center gap-2 mb-2">
                <span class="text-[10px] uppercase tracking-wider font-medium text-[#666666] bg-[#1F1F1F] px-1.5 py-0.5 rounded border border-[#2C2C2C]">{source.source_type}</span>
                <span class="text-xs text-[#666666]">{formatDate(source.created_at)}</span>
              </div>
              {#if source.tags.length > 0}
                <div class="flex flex-wrap gap-1">
                  {#each source.tags as tag}
                    <span class="text-[10px] px-1.5 py-0.5 rounded bg-[#333333] text-[#9B9B9B]">#{tag}</span>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        {:else if !isLoading}
          <div class="col-span-full flex flex-col items-center justify-center py-12 border border-dashed border-[#2C2C2C] rounded-lg">
            <Book size={48} class="text-[#333333] mb-4" />
            <p class="text-[#666666] mb-4">No knowledge sources yet</p>
            <button class="px-4 py-2 bg-[#EBEBEB] hover:bg-white text-black text-sm font-medium rounded transition-colors" onclick={() => activeTab = 'add'}>
              Add your first source
            </button>
          </div>
        {/if}
      </div>

    {:else if activeTab === 'search'}
      <!-- Search -->
      <div class="space-y-8 max-w-3xl mx-auto">
        <!-- Knowledge Base Search -->
        <div class="bg-[#252525] border border-transparent rounded-lg p-6">
          <h3 class="text-sm font-medium text-[#9B9B9B] uppercase tracking-wider mb-4 flex items-center gap-2">
             <Book size={16} /> Search Knowledge Base
          </h3>
          <div class="flex gap-2 mb-4">
            <input
              type="text"
              placeholder="Search stored content..."
              class="flex-1 bg-[#1F1F1F] border border-[#333333] rounded px-3 py-2 text-sm text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
              bind:value={searchQuery}
              onkeydown={(e) => e.key === 'Enter' && searchKnowledge()}
            />
            <button class="px-4 py-2 bg-[#EBEBEB] hover:bg-white text-black text-sm font-medium rounded transition-colors disabled:opacity-50" onclick={searchKnowledge} disabled={isLoading}>
              Search
            </button>
          </div>

          {#if knowledgeSearchResults.length > 0}
            <div class="space-y-3 max-h-96 overflow-y-auto pr-2">
              {#each knowledgeSearchResults as result}
                <div class="bg-[#1F1F1F] border border-[#2C2C2C] rounded p-4 hover:border-[#444444] transition-colors">
                  <div class="flex justify-between items-start mb-2">
                    <span class="text-sm font-medium text-[#EBEBEB]">{result.source_title}</span>
                    <span class="text-xs px-1.5 py-0.5 rounded bg-indigo-500/10 text-indigo-400">{Math.round(result.similarity * 100)}% match</span>
                  </div>
                  <p class="text-sm text-[#9B9B9B] leading-relaxed mb-2">{result.chunk.text.substring(0, 300)}{result.chunk.text.length > 300 ? '...' : ''}</p>
                  <a href={result.source_url} target="_blank" rel="noopener" class="text-xs text-[#777777] hover:text-[#EBEBEB] hover:underline flex items-center gap-1">
                     <Globe size={10} />
                     {result.source_url}
                  </a>
                </div>
              {/each}
            </div>
          {:else if searchQuery && !isLoading}
            <p class="text-center text-[#666666] py-8">No results found in knowledge base</p>
          {/if}
        </div>

        <!-- Web Search -->
        <div class="bg-[#252525] border border-transparent rounded-lg p-6">
          <h3 class="text-sm font-medium text-[#9B9B9B] uppercase tracking-wider mb-4 flex items-center gap-2">
            <Globe size={16} /> Web Search (DuckDuckGo)
          </h3>
          <div class="flex gap-2 mb-4">
            <input
              type="text"
              placeholder="Search the web..."
              class="flex-1 bg-[#1F1F1F] border border-[#333333] rounded px-3 py-2 text-sm text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
              bind:value={webSearchQuery}
              onkeydown={(e) => e.key === 'Enter' && searchWeb()}
            />
            <button class="px-4 py-2 bg-[#333333] hover:bg-[#444444] text-[#EBEBEB] text-sm font-medium rounded transition-colors disabled:opacity-50" onclick={searchWeb} disabled={isLoading}>
              Search Web
            </button>
          </div>

          {#if webSearchResults.length > 0}
            <!-- AI Summary Button -->
            <div class="flex items-center gap-3 mb-4 p-3 bg-[#1F1F1F]/50 rounded border border-[#2C2C2C]">
              <button
                class="flex items-center gap-2 px-3 py-1.5 bg-[#EBEBEB] hover:bg-white text-black text-xs font-medium rounded transition-colors disabled:opacity-70"
                onclick={summarizeWebSearch}
                disabled={isSummarizing}
              >
                {#if isSummarizing}
                  <div class="w-3 h-3 border-2 border-black/30 border-t-black rounded-full animate-spin"></div>
                  Summarizing...
                {:else}
                  <Sparkles size={12} />
                  <span>AI Summary</span>
                {/if}
              </button>
              <span class="text-xs text-[#666666]">Get an AI-powered summary of these results</span>
            </div>

            <!-- AI Summary Display -->
            {#if webSearchSummary}
              <div class="mb-6 bg-[#1F1F1F] border border-transparent rounded-lg overflow-hidden">
                <div class="px-4 py-2 bg-[#252525] border-b border-[#2C2C2C] flex items-center gap-2">
                  <Sparkles size={14} class="text-[#EBEBEB]" />
                  <span class="text-xs font-medium text-[#9B9B9B] uppercase tracking-wide">AI Summary</span>
                </div>
                <div class="p-4 text-sm text-[#CCCCCC] leading-relaxed whitespace-pre-wrap bg-[#1F1F1F]">
                  {webSearchSummary}
                </div>
              </div>
            {/if}

            <div class="space-y-3 max-h-96 overflow-y-auto pr-2">
              {#each webSearchResults as result}
                <div class="bg-[#1F1F1F] border-l-2 border-l-[#EBEBEB] border-y border-r border-[#2C2C2C] rounded-r p-4">
                  <div class="flex justify-between items-start mb-1">
                    <span class="text-sm font-medium text-[#EBEBEB]">{result.title}</span>
                    <button
                      class="px-2 py-1 text-xs border border-[#444444] text-[#9B9B9B] rounded hover:bg-[#333333] hover:text-[#EBEBEB] transition-colors flex items-center gap-1"
                      onclick={() => { newUrl = result.url; activeTab = 'add'; }}
                      title="Add to knowledge base"
                    >
                      <Plus size={10} /> Add
                    </button>
                  </div>
                  <a href={result.url} target="_blank" rel="noopener" class="text-xs text-[#777777] hover:text-[#EBEBEB] hover:underline block truncate">{result.url}</a>
                </div>
              {/each}
            </div>
          {:else if webSearchQuery && !isLoading}
            <p class="text-center text-[#666666] py-8">No web results found</p>
          {/if}
        </div>
      </div>

    {:else if activeTab === 'ask'}
      <!-- Ask AI -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 max-w-6xl mx-auto">
        <div class="lg:col-span-2 bg-[#252525] border border-transparent rounded-lg p-6">
          <h3 class="text-lg font-medium text-[#EBEBEB] mb-2 flex items-center gap-2">
            <Bot size={20} /> Ask AI About Your Knowledge Base
          </h3>
          <p class="text-sm text-[#9B9B9B] mb-6">
            Ask questions about your stored documents, web content, and meeting transcripts.
          </p>

          <div class="flex gap-2 mb-4">
            <input
              type="text"
              placeholder="e.g., What did we discuss about the Q4 roadmap?"
              class="flex-1 bg-[#1F1F1F] border border-[#333333] rounded px-3 py-2 text-sm text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
              bind:value={aiQuestion}
              onkeydown={(e) => e.key === 'Enter' && askAI()}
              disabled={!kbInitialized || isAiThinking}
            />
            <button
              class="px-4 py-2 bg-[#EBEBEB] hover:bg-white text-black text-sm font-medium rounded transition-colors disabled:opacity-50 flex items-center gap-2"
              onclick={askAI}
              disabled={!kbInitialized || isAiThinking || !aiQuestion.trim()}
            >
              {#if isAiThinking}
                <div class="w-4 h-4 border-2 border-black/30 border-t-black rounded-full animate-spin"></div>
              {:else if llmInitializing}
                Init...
              {:else}
                <Sparkles size={16} />
                Ask AI
              {/if}
            </button>
          </div>

          {#if !llmInitialized && !llmInitializing}
            <p class="text-xs text-[#666666] mb-4">
              The AI will connect to your LLM server when you ask your first question.
            </p>
          {/if}

          {#if aiAnswer}
            <div class="bg-[#1F1F1F] border border-[#2C2C2C] rounded-lg overflow-hidden mt-6">
              <div class="px-4 py-2 bg-[#252525] border-b border-[#2C2C2C] flex items-center gap-2">
                <Bot size={18} class="text-[#EBEBEB]" />
                <span class="text-xs font-medium text-[#9B9B9B] uppercase tracking-wide">AI Response</span>
              </div>
              <div class="p-4 text-sm text-[#EBEBEB] leading-relaxed whitespace-pre-wrap">
                {aiAnswer}
              </div>
            </div>
          {/if}

          {#if isAiThinking}
            <div class="flex items-center gap-3 p-4 bg-indigo-500/5 rounded-lg mt-4 text-indigo-400 text-sm">
              <div class="w-4 h-4 border-2 border-indigo-500/30 border-t-indigo-500 rounded-full animate-spin"></div>
              <span>Searching knowledge base and generating response...</span>
            </div>
          {/if}
        </div>

        <div class="bg-[#252525] border border-transparent rounded-lg p-6 h-fit">
          <h4 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4">Example Questions</h4>
          <div class="space-y-2">
            <button class="w-full text-left text-xs text-[#9B9B9B] hover:text-[#EBEBEB] p-2 rounded hover:bg-[#333333] transition-colors" onclick={() => { aiQuestion = "What are the key topics in my knowledge base?"; askAI(); }}>
              What are the key topics in my knowledge base?
            </button>
            <button class="w-full text-left text-xs text-[#9B9B9B] hover:text-[#EBEBEB] p-2 rounded hover:bg-[#333333] transition-colors" onclick={() => { aiQuestion = "Summarize what I've stored about [topic]"; }}>
              Summarize what I've stored about [topic]
            </button>
            <button class="w-full text-left text-xs text-[#9B9B9B] hover:text-[#EBEBEB] p-2 rounded hover:bg-[#333333] transition-colors" onclick={() => { aiQuestion = "What action items are pending?"; askAI(); }}>
              What action items are pending?
            </button>
            <button class="w-full text-left text-xs text-[#9B9B9B] hover:text-[#EBEBEB] p-2 rounded hover:bg-[#333333] transition-colors" onclick={() => { aiQuestion = "Find information about [specific topic]"; }}>
              Find information about [specific topic]
            </button>
          </div>
        </div>
      </div>

    {:else if activeTab === 'add'}
      <!-- Add Source -->
      <div class="grid grid-cols-1 md:grid-cols-3 gap-6 max-w-6xl mx-auto">
        <div class="md:col-span-2 space-y-6">
          <!-- URL Form -->
          <div class="bg-[#252525] border border-transparent rounded-lg p-6">
            <h3 class="text-base font-medium text-[#EBEBEB] mb-1 flex items-center gap-2">
              <Globe size={18} /> Add URL
            </h3>
            <p class="text-sm text-[#9B9B9B] mb-4">Crawl a web page and store its content</p>

            <div class="space-y-4">
              <div>
                <label for="url-input" class="block text-xs font-medium text-[#9B9B9B] mb-1.5">URL</label>
                <input
                  id="url-input"
                  type="url"
                  placeholder="https://example.com/article"
                  class="w-full bg-[#1F1F1F] border border-[#333333] rounded px-3 py-2 text-sm text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
                  bind:value={newUrl}
                  disabled={!kbInitialized}
                />
              </div>

              <div>
                <label for="tags-input" class="block text-xs font-medium text-[#9B9B9B] mb-1.5">Tags (comma-separated)</label>
                <input
                  id="tags-input"
                  type="text"
                  placeholder="meeting-prep, project-x, research"
                  class="w-full bg-[#1F1F1F] border border-[#333333] rounded px-3 py-2 text-sm text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
                  bind:value={newTags}
                  disabled={!kbInitialized}
                />
              </div>

              {#if crawlStatus}
                <div class="flex items-center gap-3 p-3 bg-indigo-500/10 rounded text-indigo-400 text-sm">
                  <div class="w-4 h-4 border-2 border-indigo-500/30 border-t-indigo-500 rounded-full animate-spin"></div>
                  <span>{crawlStatus}</span>
                </div>
              {/if}

              <button
                class="w-full py-2 bg-[#EBEBEB] hover:bg-white text-black text-sm font-medium rounded transition-colors disabled:opacity-50"
                onclick={() => crawlAndStore(newUrl)}
                disabled={isCrawling || !newUrl.trim() || !kbInitialized}
              >
                {isCrawling ? 'Crawling...' : 'Crawl & Store'}
              </button>
            </div>
          </div>

          <!-- Document Upload Form -->
          <div class="bg-[#252525] border border-transparent rounded-lg p-6">
            <h3 class="text-base font-medium text-[#EBEBEB] mb-1 flex items-center gap-2">
              <FileText size={18} /> Upload Document
            </h3>
            <p class="text-sm text-[#9B9B9B] mb-4">Upload PDF, TXT, or Markdown files</p>

            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div 
              class="border-2 border-dashed border-[#444444] rounded-lg p-8 text-center mb-4 transition-colors {kbInitialized && !isUploading ? 'hover:border-[#EBEBEB] hover:bg-[#1F1F1F] cursor-pointer' : 'opacity-50 cursor-not-allowed'}"
              onclick={() => { if (kbInitialized && !isUploading) uploadDocument(); }}
            >
              <div class="text-4xl mb-2 flex justify-center text-[#9B9B9B]">
                 <Upload size={32} />
              </div>
              <p class="text-sm text-[#EBEBEB] font-medium">Click to select a file</p>
              <p class="text-xs text-[#9B9B9B] mt-1">PDF, TXT, MD supported</p>
            </div>

            <div class="mb-4">
              <label for="doc-tags-input" class="block text-xs font-medium text-[#9B9B9B] mb-1.5">Tags (comma-separated)</label>
              <input
                id="doc-tags-input"
                type="text"
                placeholder="docs, reference, project-x"
                class="w-full bg-[#1F1F1F] border border-[#333333] rounded px-3 py-2 text-sm text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] placeholder:text-[#666666]"
                bind:value={newTags}
                disabled={!kbInitialized}
              />
            </div>

            {#if uploadStatus}
              <div class="flex items-center gap-3 p-3 bg-indigo-500/10 rounded text-indigo-400 text-sm">
                <div class="w-4 h-4 border-2 border-indigo-500/30 border-t-indigo-500 rounded-full animate-spin"></div>
                <span>{uploadStatus}</span>
              </div>
            {/if}
            
            <button
              class="w-full py-2 bg-[#333333] hover:bg-[#444444] text-[#EBEBEB] text-sm font-medium rounded transition-colors disabled:opacity-50"
              onclick={uploadDocument}
              disabled={!kbInitialized || isUploading}
            >
              {isUploading ? 'Uploading...' : 'Choose File'}
            </button>
          </div>
        </div>

        <div class="bg-[#252525] border border-transparent rounded-lg p-6 h-fit">
          <h4 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4">Tips</h4>
          <ul class="space-y-2 text-sm text-[#9B9B9B] list-disc pl-4">
            <li>Add documentation pages before meetings for context</li>
            <li>Upload PDFs of specs, reports, or reference materials</li>
            <li>Use tags to organize content by project or topic</li>
            <li>Search results from the web can be added directly</li>
            <li>The AI assistant can use this content during meetings</li>
          </ul>
        </div>
      </div>
    {/if}
  </div>

  {#if isLoading}
    <div class="absolute inset-0 bg-black/50 flex items-center justify-center z-50 rounded-lg backdrop-blur-sm">
      <div class="w-10 h-10 border-4 border-indigo-500/30 border-t-indigo-500 rounded-full animate-spin"></div>
    </div>
  {/if}
</div>
