<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { Brain, Check, Download, AlertCircle, Loader2 } from "lucide-svelte";

  interface ModelStatus {
    id: string;
    name: string;
    installed: boolean;
    size_bytes: number;
  }

  interface DownloadProgress {
    model_id: string;
    model_name: string;
    downloaded_bytes: number;
    total_bytes: number;
    progress_percent: number;
    status: string;
  }

  let { onComplete }: { onComplete: () => void } = $props();

  let models = $state<ModelStatus[]>([]);
  let currentDownload = $state<DownloadProgress | null>(null);
  let overallProgress = $state(0);
  let status = $state<"checking" | "downloading" | "complete" | "error">("checking");
  let errorMessage = $state("");
  let unlisten: (() => void) | null = null;

  onMount(async () => {
    // Listen for download progress events
    unlisten = await listen<DownloadProgress>("download-progress", (event) => {
      currentDownload = event.payload;

      // Calculate overall progress
      const completedModels = models.filter(m => m.installed).length;
      const currentProgress = event.payload.progress_percent / 100;
      overallProgress = ((completedModels + currentProgress) / models.length) * 100;

      if (event.payload.status === "complete") {
        // Mark model as installed
        models = models.map(m =>
          m.id === event.payload.model_id ? { ...m, installed: true } : m
        );
      }
    });

    // Check model status
    await checkAndDownload();
  });

  onDestroy(() => {
    if (unlisten) unlisten();
  });

  async function checkAndDownload() {
    try {
      status = "checking";

      // Check if models are already installed
      const ready = await invoke<boolean>("are_models_ready");

      if (ready) {
        status = "complete";
        onComplete();
        return;
      }

      // Get model status
      models = await invoke<ModelStatus[]>("check_models_status");

      // Start downloading
      status = "downloading";
      await invoke("download_models");

      // Download complete
      status = "complete";

      // Small delay to show completion
      setTimeout(() => {
        onComplete();
      }, 500);

    } catch (e) {
      status = "error";
      errorMessage = String(e);
      console.error("Model setup error:", e);
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

<div class="flex items-center justify-center min-h-screen bg-[#1F1F1F] p-5">
  <div class="bg-[#252525] rounded-3xl p-12 max-w-md w-full text-center border border-[#333333] shadow-2xl relative overflow-hidden">
    <!-- Background blurred blob for effect -->
    <div class="absolute -top-20 -right-20 w-64 h-64 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none"></div>
    <div class="absolute -bottom-20 -left-20 w-64 h-64 bg-purple-500/10 rounded-full blur-3xl pointer-events-none"></div>

    <div class="text-[#EBEBEB] mb-6 flex justify-center relative z-10">
      <div class="p-4 bg-[#1F1F1F] rounded-2xl border border-[#333333] shadow-lg">
        <Brain size={48} />
      </div>
    </div>

    <h1 class="text-2xl font-semibold text-[#EBEBEB] mb-2 relative z-10">Second Brain</h1>
    
    {#if status === "checking"}
      <p class="text-[#9B9B9B] text-sm mb-8 relative z-10">Checking for required AI models...</p>
      <div class="flex justify-center relative z-10">
        <Loader2 class="animate-spin text-indigo-500" size={32} />
      </div>

    {:else if status === "downloading"}
      <p class="text-[#9B9B9B] text-sm mb-8 relative z-10">Downloading AI models for first-time setup</p>

      <div class="relative z-10">
        <div class="flex items-center justify-between text-xs text-[#9B9B9B] mb-2">
          <span>Overall Progress</span>
          <span class="text-[#EBEBEB] font-medium">{overallProgress.toFixed(0)}%</span>
        </div>
        <div class="h-2 bg-[#1F1F1F] rounded-full overflow-hidden mb-8 border border-[#333333]">
          <div class="h-full bg-indigo-500 rounded-full transition-all duration-300" style="width: {overallProgress}%"></div>
        </div>

        {#if currentDownload}
          <div class="flex justify-between items-center p-3 px-4 bg-[#1F1F1F] border border-[#333333] rounded-xl mb-4 text-left">
            <div class="flex items-center gap-3">
              <Download size={16} class="text-indigo-400" />
              <div>
                <div class="text-[#EBEBEB] text-sm font-medium">{currentDownload.model_name}</div>
                <div class="text-[#666666] text-xs mt-0.5">
                  {#if currentDownload.status === "downloading"}
                    {formatBytes(currentDownload.downloaded_bytes)} / {formatBytes(currentDownload.total_bytes)}
                  {:else if currentDownload.status === "extracting"}
                    Extracting...
                  {:else if currentDownload.status === "complete"}
                    Download Complete
                  {/if}
                </div>
              </div>
            </div>
            
            {#if currentDownload.status === "downloading"}
              <div class="w-8 h-8 flex items-center justify-center">
                 <Loader2 size={16} class="animate-spin text-[#9B9B9B]" />
              </div>
            {:else if currentDownload.status === "complete"}
               <Check size={16} class="text-green-500" />
            {/if}
          </div>
        {/if}

        <div class="flex flex-col gap-2 relative z-10">
          {#each models as model}
            <div class="flex justify-between items-center p-2.5 px-3.5 bg-[#1F1F1F] border border-transparent rounded-lg transition-all {model.installed ? 'border-green-500/20 bg-green-500/5' : ''}">
              <span class="text-[#9B9B9B] text-xs flex items-center gap-2">
                {#if model.installed}
                  <Check size={12} class="text-green-500" />
                {:else}
                  <div class="w-3 h-3 rounded-full border border-[#444444]"></div>
                {/if}
                {model.name}
              </span>
              
              {#if currentDownload?.model_id === model.id && !model.installed}
                <Loader2 size={12} class="animate-spin text-indigo-500" />
              {:else if !model.installed}
                <span class="text-[#444444] text-[10px] uppercase font-medium">Pending</span>
              {/if}
            </div>
          {/each}
        </div>
      </div>

    {:else if status === "complete"}
      <div class="relative z-10">
        <p class="text-green-500 text-sm mb-6 flex items-center justify-center gap-2">
          <Check size={16} /> Setup complete!
        </p>
        <div class="w-16 h-16 bg-green-500/10 rounded-full flex items-center justify-center mx-auto mb-6">
          <Check size={32} class="text-green-500" />
        </div>
        <p class="text-[#9B9B9B] text-sm">Launching application...</p>
      </div>

    {:else if status === "error"}
      <div class="relative z-10">
        <div class="flex items-center justify-center gap-2 text-red-500 text-sm mb-4">
           <AlertCircle size={16} /> Setup failed
        </div>
        <div class="text-red-400 text-xs mb-6 p-4 bg-red-500/10 border border-red-500/20 rounded-lg text-left break-words">
          {errorMessage}
        </div>
        <button 
          class="bg-[#EBEBEB] hover:bg-white text-black border-none py-2.5 px-6 rounded-lg text-sm font-medium cursor-pointer transition-colors" 
          onclick={checkAndDownload}
        >
          Retry
        </button>
      </div>
    {/if}
  </div>
</div>
