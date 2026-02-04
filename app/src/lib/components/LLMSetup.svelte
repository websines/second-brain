<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { Brain, Server, Zap, Check, AlertCircle, Loader2, ExternalLink } from "lucide-svelte";

  let { onComplete }: { onComplete: () => void } = $props();

  // Form state
  let llmUrl = $state("");
  let llmModel = $state("");
  let llmApiKey = $state("");
  let isTestingConnection = $state(false);
  let connectionStatus = $state<"idle" | "testing" | "success" | "error">("idle");
  let errorMessage = $state("");
  let isSaving = $state(false);

  // Validate URL - only check for valid format, not hostname reachability
  function validateUrl(url: string): { valid: boolean; warning: string } {
    const trimmed = url.trim();
    if (!trimmed) {
      return { valid: false, warning: "" };
    }

    try {
      new URL(trimmed);
      return { valid: true, warning: "" };
    } catch {
      // Help user fix common format issues
      if (!trimmed.startsWith("http://") && !trimmed.startsWith("https://")) {
        return { valid: false, warning: `Add http:// or https:// prefix` };
      }
      return { valid: false, warning: "Invalid URL format" };
    }
  }

  // Preset configurations
  const presets = [
    {
      name: "LM Studio",
      url: "http://localhost:1234/v1",
      model: "local-model",
      needsKey: false,
      description: "Local LLM server"
    },
    {
      name: "Ollama",
      url: "http://localhost:11434/v1",
      model: "llama3.2",
      needsKey: false,
      description: "Local Ollama server"
    },
    {
      name: "OpenAI",
      url: "https://api.openai.com/v1",
      model: "gpt-4o-mini",
      needsKey: true,
      description: "Requires API key"
    },
    {
      name: "OpenRouter",
      url: "https://openrouter.ai/api/v1",
      model: "anthropic/claude-3.5-sonnet",
      needsKey: true,
      description: "Multi-provider gateway"
    }
  ];

  onMount(async () => {
    // Check if settings already exist
    try {
      const settings = await invoke<{
        llm_url: string;
        llm_model: string;
        llm_api_key: string;
      }>("get_user_settings");

      if (settings.llm_url && settings.llm_url.trim() !== "") {
        llmUrl = settings.llm_url;
        llmModel = settings.llm_model || "";
        llmApiKey = settings.llm_api_key || "";
      }
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  });

  function applyPreset(preset: typeof presets[0]) {
    llmUrl = preset.url;
    llmModel = preset.model;
    if (!preset.needsKey) {
      llmApiKey = "";
    }
    connectionStatus = "idle";
    errorMessage = "";
  }

  // Parse error message for user-friendly feedback
  function parseErrorMessage(error: string): string {
    const errorStr = String(error);

    // DNS errors
    if (errorStr.includes("dns error") || errorStr.includes("nodename nor servname")) {
      const match = errorStr.match(/url \(([^)]+)\)/);
      const url = match ? match[1] : "the URL";
      return `Cannot resolve hostname. The server at ${url} was not found. Check that:\n• The hostname is correct\n• For local servers, use "localhost" or "127.0.0.1"\n• For remote servers, ensure the domain exists`;
    }

    // Connection refused
    if (errorStr.includes("Connection refused") || errorStr.includes("connection refused")) {
      return "Connection refused. The server is not running or not accepting connections on this port. Make sure:\n• LM Studio/Ollama is running\n• The server is started (check the app)\n• The port number is correct";
    }

    // Timeout
    if (errorStr.includes("timeout") || errorStr.includes("timed out")) {
      return "Connection timed out. The server took too long to respond. Check that:\n• The server is running and responding\n• No firewall is blocking the connection\n• The URL is correct";
    }

    // SSL/TLS errors
    if (errorStr.includes("ssl") || errorStr.includes("certificate") || errorStr.includes("https")) {
      return "SSL/TLS error. For local servers, try using http:// instead of https://";
    }

    // 404 Not Found
    if (errorStr.includes("404") || errorStr.includes("not found")) {
      return "Endpoint not found (404). The URL path might be incorrect. OpenAI-compatible APIs typically use /v1 as the base path.";
    }

    // 401 Unauthorized
    if (errorStr.includes("401") || errorStr.includes("unauthorized") || errorStr.includes("api key")) {
      return "Authentication failed. This endpoint requires an API key. Set the OPENAI_API_KEY environment variable or use a local server that doesn't require auth.";
    }

    return errorStr;
  }

  async function testConnection() {
    if (!llmUrl.trim()) {
      errorMessage = "Please enter a URL";
      connectionStatus = "error";
      return;
    }

    // Validate URL format first
    const validation = validateUrl(llmUrl);
    if (!validation.valid) {
      errorMessage = validation.warning || "Invalid URL format";
      connectionStatus = "error";
      return;
    }

    isTestingConnection = true;
    connectionStatus = "testing";
    errorMessage = "";

    try {
      // First save the settings
      await invoke("set_user_setting", { key: "llm_url", value: llmUrl.trim() });
      await invoke("set_user_setting", { key: "llm_model", value: llmModel.trim() || "default" });
      await invoke("set_user_setting", { key: "llm_api_key", value: llmApiKey.trim() });

      // Initialize the LLM with new settings
      await invoke("initialize_llm", {
        apiUrl: llmUrl.trim(),
        model: llmModel.trim() || null,
        apiKey: llmApiKey.trim() || null
      });

      // Try a simple test query
      const response = await invoke<string>("ask_assistant", {
        question: "Say 'OK' if you can hear me."
      });

      if (response && response.length > 0) {
        connectionStatus = "success";
      } else {
        connectionStatus = "error";
        errorMessage = "No response from LLM";
      }
    } catch (e) {
      connectionStatus = "error";
      errorMessage = parseErrorMessage(String(e));
      console.error("Connection test failed:", e);
    } finally {
      isTestingConnection = false;
    }
  }

  async function saveAndContinue() {
    if (!llmUrl.trim()) {
      errorMessage = "Please enter a URL";
      connectionStatus = "error";
      return;
    }

    // Validate URL format first
    const validation = validateUrl(llmUrl);
    if (!validation.valid) {
      errorMessage = validation.warning || "Invalid URL format";
      connectionStatus = "error";
      return;
    }

    isSaving = true;

    try {
      // Save settings
      await invoke("set_user_setting", { key: "llm_url", value: llmUrl.trim() });
      await invoke("set_user_setting", { key: "llm_model", value: llmModel.trim() || "default" });
      await invoke("set_user_setting", { key: "llm_api_key", value: llmApiKey.trim() });

      // Initialize the LLM
      await invoke("initialize_llm", {
        apiUrl: llmUrl.trim(),
        model: llmModel.trim() || null,
        apiKey: llmApiKey.trim() || null
      });

      // Mark setup as complete
      await invoke("set_app_state", { key: "llm_setup_complete", value: "true" });

      onComplete();
    } catch (e) {
      errorMessage = parseErrorMessage(String(e));
      connectionStatus = "error";
      console.error("Failed to save:", e);
    } finally {
      isSaving = false;
    }
  }

  async function skipSetup() {
    // Mark as skipped so we don't show again (but user can configure later in settings)
    await invoke("set_app_state", { key: "llm_setup_complete", value: "skipped" });
    onComplete();
  }
</script>

<div class="flex items-center justify-center min-h-screen bg-[#1F1F1F] p-5">
  <div class="bg-[#252525] rounded-3xl p-10 max-w-lg w-full border border-[#333333] shadow-2xl relative overflow-hidden">
    <!-- Background effects -->
    <div class="absolute -top-20 -right-20 w-64 h-64 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none"></div>
    <div class="absolute -bottom-20 -left-20 w-64 h-64 bg-purple-500/10 rounded-full blur-3xl pointer-events-none"></div>

    <!-- Header -->
    <div class="text-center mb-8 relative z-10">
      <div class="inline-flex p-4 bg-[#1F1F1F] rounded-2xl border border-[#333333] shadow-lg mb-4">
        <Server size={32} class="text-indigo-400" />
      </div>
      <h1 class="text-xl font-semibold text-[#EBEBEB] mb-2">Connect Your LLM</h1>
      <p class="text-sm text-[#9B9B9B]">
        Second Brain needs an OpenAI-compatible API endpoint for AI features
      </p>
    </div>

    <!-- Presets -->
    <div class="mb-6 relative z-10">
      <label class="block text-xs font-medium text-[#9B9B9B] uppercase tracking-wider mb-3">Quick Setup</label>
      <div class="grid grid-cols-2 gap-2">
        {#each presets as preset}
          <button
            class="p-3 rounded-lg border text-left transition-all hover:border-indigo-500/50 hover:bg-indigo-500/5
              {llmUrl === preset.url ? 'border-indigo-500 bg-indigo-500/10' : 'border-[#333333] bg-[#1F1F1F]'}"
            onclick={() => applyPreset(preset)}
          >
            <div class="text-sm font-medium text-[#EBEBEB]">{preset.name}</div>
            <div class="text-xs text-[#666666] mt-0.5">{preset.description}</div>
          </button>
        {/each}
      </div>
    </div>

    <!-- Form -->
    <div class="space-y-4 relative z-10">
      <div>
        <label class="block text-xs font-medium text-[#9B9B9B] uppercase tracking-wider mb-2">
          API Endpoint URL
        </label>
        <input
          type="text"
          placeholder="http://localhost:1234/v1"
          bind:value={llmUrl}
          class="w-full bg-[#1F1F1F] border border-[#333333] rounded-lg px-4 py-3 text-sm text-[#EBEBEB] placeholder:text-[#555555] focus:outline-none focus:border-indigo-500/50 transition-colors"
        />
        <p class="mt-1.5 text-xs text-[#555555]">
          Any OpenAI-compatible endpoint. Appends <code class="text-[#666666]">/chat/completions</code> automatically.
        </p>
      </div>

      <div>
        <label class="block text-xs font-medium text-[#9B9B9B] uppercase tracking-wider mb-2">
          API Key <span class="text-[#555555]">(optional for local servers)</span>
        </label>
        <input
          type="password"
          placeholder="sk-... or leave empty for LM Studio/Ollama"
          bind:value={llmApiKey}
          class="w-full bg-[#1F1F1F] border border-[#333333] rounded-lg px-4 py-3 text-sm text-[#EBEBEB] placeholder:text-[#555555] focus:outline-none focus:border-indigo-500/50 transition-colors"
        />
      </div>

      <div>
        <label class="block text-xs font-medium text-[#9B9B9B] uppercase tracking-wider mb-2">
          Model Name <span class="text-[#555555]">(optional)</span>
        </label>
        <input
          type="text"
          placeholder="gpt-4o-mini, llama3.2, etc."
          bind:value={llmModel}
          class="w-full bg-[#1F1F1F] border border-[#333333] rounded-lg px-4 py-3 text-sm text-[#EBEBEB] placeholder:text-[#555555] focus:outline-none focus:border-indigo-500/50 transition-colors"
        />
      </div>
    </div>

    <!-- Connection Status -->
    {#if connectionStatus !== "idle"}
      <div class="mt-4 relative z-10">
        {#if connectionStatus === "testing"}
          <div class="flex items-center gap-2 text-sm text-[#9B9B9B] bg-[#1F1F1F] rounded-lg p-3 border border-[#333333]">
            <Loader2 size={16} class="animate-spin text-indigo-400" />
            Testing connection...
          </div>
        {:else if connectionStatus === "success"}
          <div class="flex items-center gap-2 text-sm text-green-400 bg-green-500/10 rounded-lg p-3 border border-green-500/20">
            <Check size={16} />
            Connection successful!
          </div>
        {:else if connectionStatus === "error"}
          <div class="text-sm text-red-400 bg-red-500/10 rounded-lg p-3 border border-red-500/20">
            <div class="flex items-center gap-2 mb-1">
              <AlertCircle size={16} />
              Connection failed
            </div>
            {#if errorMessage}
              <div class="text-xs text-red-400/80 ml-6 whitespace-pre-line">{errorMessage}</div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Actions -->
    <div class="mt-6 space-y-3 relative z-10">
      <div class="flex gap-3">
        <button
          class="flex-1 py-2.5 px-4 bg-[#333333] hover:bg-[#444444] text-[#EBEBEB] text-sm font-medium rounded-lg transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
          onclick={testConnection}
          disabled={isTestingConnection || !llmUrl.trim()}
        >
          {#if isTestingConnection}
            <Loader2 size={16} class="animate-spin" />
          {:else}
            <Zap size={16} />
          {/if}
          Test Connection
        </button>

        <button
          class="flex-1 py-2.5 px-4 bg-indigo-500 hover:bg-indigo-400 text-white text-sm font-medium rounded-lg transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
          onclick={saveAndContinue}
          disabled={isSaving || !llmUrl.trim()}
        >
          {#if isSaving}
            <Loader2 size={16} class="animate-spin" />
          {:else}
            <Check size={16} />
          {/if}
          Save & Continue
        </button>
      </div>

      <button
        class="w-full py-2 text-sm text-[#666666] hover:text-[#9B9B9B] transition-colors"
        onclick={skipSetup}
      >
        Skip for now (AI features will be limited)
      </button>
    </div>

    <!-- Help link -->
    <div class="mt-6 text-center relative z-10">
      <a
        href="https://lmstudio.ai/"
        target="_blank"
        class="inline-flex items-center gap-1 text-xs text-[#666666] hover:text-indigo-400 transition-colors"
      >
        <ExternalLink size={12} />
        New to local LLMs? Try LM Studio
      </a>
    </div>
  </div>
</div>
