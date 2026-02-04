<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { onMount, onDestroy } from "svelte";
  import KnowledgeBaseView from "./KnowledgeBaseView.svelte";
  import MeetingDetailPage from "./MeetingDetailPage.svelte";
  import MeetingPrepPage from "./MeetingPrepPage.svelte";
  import {
    LayoutDashboard,
    Calendar,
    FileText,
    Sparkles,
    Library,
    Blocks,
    Search,
    Mic,
    Square,
    Radio,
    Plus,
    Clock,
    CheckCircle2,
    MessageSquare,
    Link as LinkIcon,
    AlertTriangle,
    ChevronRight,
    MoreHorizontal,
    Hash,
    Command,
    Server,
    Shield,
    Eye,
    EyeOff,
    Settings,
    Zap,
    Loader2,
    Camera,
    Image as ImageIcon,
    X
  } from "lucide-svelte";

  // TypeScript interfaces for user store data
  interface Note {
    id: number;
    content: string;
    tags: string[];
    pinned: boolean;
    created_at: string;
    updated_at: string;
  }

  interface Integration {
    id: string;
    name: string;
    status: string;
    access_token: string | null;
    refresh_token: string | null;
    expires_at: string | null;
    metadata: string | null;
    connected_at: string | null;
  }

  interface UserSettings {
    id: number;
    theme: string;
    llm_url: string;
    llm_model: string;
    llm_api_key: string;
    auto_record: boolean;
    notifications_enabled: boolean;
    language: string;
    created_at: string;
    updated_at: string;
  }

  // Meeting interface from KB
  interface Meeting {
    id: { tb: string; id: { String: string } } | null;
    title: string;
    start_time: number;
    end_time: number | null;
    participants: string[];
    summary: string | null;
  }

  // Live transcription segment (from SenseVoice + Smart Turn)
  interface LiveSegment {
    text: string;
    source: string;
    timestamp_ms: number;
    is_final: boolean;
    language?: string;           // Detected language (en/zh/ja/ko/yue)
    emotion?: string;            // Detected emotion (Neutral/Happy/Sad/Angry/etc)
    audio_events?: string[];     // Audio events (Speech/Laughter/Applause/etc)
    is_turn_complete?: boolean;  // Whether speaker finished their turn
    turn_confidence?: number;    // Confidence of turn completion (0-1)
  }

  // Props
  let {
    isRecording = false,
  }: {
    isRecording?: boolean;
  } = $props();

  // Navigation state
  let activeView = $state<'home' | 'meetings' | 'notes' | 'insights' | 'tools' | 'knowledge'>('home');

  // Meeting state
  let currentMeetingId = $state<string | null>(null);
  let meetingTitle = $state("");
  let meetingContext = $state<string>("");  // Text context for real-time suggestions
  let linkedSources = $state<KnowledgeSource[]>([]);  // Knowledge sources linked to meeting
  let availableSources = $state<KnowledgeSource[]>([]);  // All available KB sources

  // Knowledge source interface (SurrealDB Thing format)
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

  // Search/Ask state
  let searchQuery = $state("");
  let askQuery = $state("");
  let isLoading = $state(false);

  // Real data from user store
  let notes = $state<Note[]>([]);
  let integrations = $state<Integration[]>([]);
  let userSettings = $state<UserSettings | null>(null);

  // Meetings from Knowledge Base
  let meetings = $state<Meeting[]>([]);
  let selectedMeetingId = $state<string | null>(null);

  // Live transcription during recording
  let liveTranscript = $state<LiveSegment[]>([]);
  let unlistenTranscription: (() => void) | null = null;

  // AI Assistant state
  let aiResponse = $state<string>("");
  let aiLoading = $state(false);

  // Real-time suggestions during recording
  interface RealtimeSuggestion {
    insight: string | null;
    question: string | null;
    related_info: string | null;
  }
  let realtimeSuggestion = $state<RealtimeSuggestion | null>(null);
  let suggestionPollInterval: ReturnType<typeof setInterval> | null = null;
  let lastSuggestionFetch = $state<number>(0);

  // Real data from database
  interface ActionItem {
    id: string;
    text: string;
    assignee: string | null;
    deadline: string | null;
    status: string;
    meeting_title: string;
    meeting_id: string;
    created_at: number;
  }

  interface Decision {
    id: string;
    text: string;
    meeting_title: string;
    meeting_id: string;
    created_at: number;
  }

  interface EntityCount {
    label: string;
    count: number;
  }

  let actionItems = $state<ActionItem[]>([]);
  let decisions = $state<Decision[]>([]);
  let entityCounts = $state<EntityCount[]>([]);
  let totalSegments = $state(0);

  // Computed upcoming items from action items with deadlines
  let upcomingItems = $derived(() => {
    const now = new Date();
    const items: Array<{type: string; title: string; time: string; priority?: string}> = [];

    // Add action items with deadlines
    for (const action of actionItems.filter(a => a.status !== 'completed' && a.deadline)) {
      const deadline = new Date(action.deadline!);
      const diffDays = Math.ceil((deadline.getTime() - now.getTime()) / (1000 * 60 * 60 * 24));
      let time = '';
      if (diffDays < 0) time = 'Overdue';
      else if (diffDays === 0) time = 'Due today';
      else if (diffDays === 1) time = 'Due tomorrow';
      else time = `Due in ${diffDays} days`;

      items.push({
        type: 'action',
        title: action.text,
        time,
        priority: diffDays <= 1 ? 'high' : diffDays <= 3 ? 'medium' : 'low'
      });
    }

    // Add items without deadlines
    for (const action of actionItems.filter(a => a.status !== 'completed' && !a.deadline).slice(0, 3)) {
      items.push({
        type: 'action',
        title: action.text,
        time: 'No deadline',
        priority: 'low'
      });
    }

    return items.slice(0, 5);
  });

  // Computed insights from real data
  let insights = $derived(() => {
    const items: Array<{type: string; title: string; description: string}> = [];

    // Open action items insight
    const openActions = actionItems.filter(a => a.status !== 'completed').length;
    if (openActions > 0) {
      items.push({
        type: 'commitment',
        title: `${openActions} Open Action Items`,
        description: `You have ${openActions} action item${openActions > 1 ? 's' : ''} that need attention.`
      });
    }

    // Recent decisions insight
    if (decisions.length > 0) {
      items.push({
        type: 'pattern',
        title: `${decisions.length} Decisions Made`,
        description: `Recent: "${decisions[0]?.text?.slice(0, 50)}${(decisions[0]?.text?.length || 0) > 50 ? '...' : ''}"`
      });
    }

    // Entity patterns
    const topEntities = entityCounts.slice(0, 2);
    if (topEntities.length > 0) {
      items.push({
        type: 'connection',
        title: 'Key Topics',
        description: topEntities.map(e => `${e.label}: ${e.count}`).join(', ')
      });
    }

    // Meeting stats
    if (meetings.length > 0) {
      const recentMeetings = meetings.filter(m => {
        const age = Date.now() - m.start_time;
        return age < 7 * 24 * 60 * 60 * 1000; // Last 7 days
      }).length;
      if (recentMeetings > 0) {
        items.push({
          type: 'pattern',
          title: `${recentMeetings} Meetings This Week`,
          description: `${totalSegments} transcript segments captured across all meetings.`
        });
      }
    }

    // Empty state
    if (items.length === 0) {
      items.push({
        type: 'pattern',
        title: 'Getting Started',
        description: 'Record your first meeting to see insights and patterns emerge.'
      });
    }

    return items;
  });

  // Default integrations that can be connected
  const availableIntegrations = [
    { id: 'calendar', name: 'Google Calendar', icon: '📅' },
    { id: 'slack', name: 'Slack', icon: '💬' },
    { id: 'notion', name: 'Notion', icon: '📝' },
    { id: 'linear', name: 'Linear', icon: '📋' },
    { id: 'github', name: 'GitHub', icon: '🐙' },
  ];

  // Computed integrations list with status
  let connectedTools = $derived(
    availableIntegrations.map(ai => {
      const saved = integrations.find(i => i.id === ai.id);
      return {
        ...ai,
        status: saved?.status === 'connected' ? 'connected' : 'available'
      };
    })
  );

  let newNoteContent = $state("");
  let newNoteTags = $state("");

  // Settings state
  let stealthModeEnabled = $state(true);  // Default to enabled for privacy
  let llmUrl = $state("");
  let llmModel = $state("");
  let llmApiKey = $state("");
  let isTestingLLM = $state(false);
  let llmTestStatus = $state<"idle" | "success" | "error">("idle");
  let llmTestError = $state("");

  // Screenshot analysis state
  let isCapturingScreenshot = $state(false);
  let screenshotAnalysis = $state<string | null>(null);
  let screenshotError = $state<string | null>(null);
  let showScreenshotResult = $state(false);

  // Load data on mount
  onMount(async () => {
    await loadNotes();
    await loadIntegrations();
    await loadSettings();
    await loadMeetings();
    await loadActionItems();
    await loadDecisions();
    await loadStats();
    await setupTranscriptionListener();
    await loadAppSettings();
    await setupHotkeyListeners();
    await setupWindowCloseHandler();
  });

  // Handle window close - end any active meeting
  async function setupWindowCloseHandler() {
    try {
      const currentWindow = getCurrentWebviewWindow();
      await currentWindow.onCloseRequested(async (event) => {
        // If there's an active meeting, end it before closing
        if (currentMeetingId) {
          console.log("[Window Close] Ending active meeting before close:", currentMeetingId);
          try {
            // Stop recording if active
            if (isRecording) {
              await invoke("stop_recording");
            }
            // End the meeting
            await invoke("end_meeting", { summary: "(Ended - window closed)" });
            console.log("[Window Close] Meeting ended successfully");
          } catch (e) {
            console.error("[Window Close] Failed to end meeting:", e);
          }
        }
        // Allow the window to close
      });
    } catch (e) {
      console.error("Failed to setup window close handler:", e);
    }
  }

  async function loadAppSettings() {
    try {
      // Load stealth mode setting
      const stealthState = await invoke<string | null>("get_app_state", { key: "stealth_mode" });
      stealthModeEnabled = stealthState !== "false";  // Default to enabled

      // Apply stealth mode
      await invoke("set_screen_share_protection", { enabled: stealthModeEnabled });

      // Load LLM settings
      if (userSettings) {
        llmUrl = userSettings.llm_url || "";
        llmModel = userSettings.llm_model || "";
        llmApiKey = userSettings.llm_api_key || "";
      }
    } catch (e) {
      console.error("Failed to load app settings:", e);
    }
  }

  async function toggleStealthMode() {
    try {
      stealthModeEnabled = !stealthModeEnabled;
      await invoke("set_screen_share_protection", { enabled: stealthModeEnabled });
      await invoke("set_app_state", { key: "stealth_mode", value: stealthModeEnabled ? "true" : "false" });
    } catch (e) {
      console.error("Failed to toggle stealth mode:", e);
      // Revert on failure
      stealthModeEnabled = !stealthModeEnabled;
    }
  }

  async function saveLLMSettings() {
    isTestingLLM = true;
    llmTestStatus = "idle";
    llmTestError = "";

    try {
      // Save settings
      await invoke("set_user_setting", { key: "llm_url", value: llmUrl.trim() });
      await invoke("set_user_setting", { key: "llm_model", value: llmModel.trim() || "default" });
      await invoke("set_user_setting", { key: "llm_api_key", value: llmApiKey.trim() });

      // Re-initialize LLM with new settings
      await invoke("initialize_llm", {
        apiUrl: llmUrl.trim(),
        model: llmModel.trim() || null,
        apiKey: llmApiKey.trim() || null
      });

      // Test connection
      const response = await invoke<string>("ask_assistant", {
        question: "Say 'OK' if you can hear me."
      });

      if (response && response.length > 0) {
        llmTestStatus = "success";
      } else {
        llmTestStatus = "error";
        llmTestError = "No response from LLM";
      }
    } catch (e) {
      llmTestStatus = "error";
      llmTestError = String(e);
      console.error("Failed to save LLM settings:", e);
    } finally {
      isTestingLLM = false;
    }
  }

  // Screenshot capture and analysis
  async function captureAndAnalyzeScreenshot(customQuestion?: string) {
    if (isCapturingScreenshot) return;

    isCapturingScreenshot = true;
    screenshotError = null;
    screenshotAnalysis = null;
    showScreenshotResult = true;

    try {
      const response = await invoke<string>("analyze_screenshot", {
        question: customQuestion || null
      });
      screenshotAnalysis = response;
    } catch (e) {
      screenshotError = String(e);
      console.error("Screenshot analysis failed:", e);
    } finally {
      isCapturingScreenshot = false;
    }
  }

  function dismissScreenshotResult() {
    showScreenshotResult = false;
    screenshotAnalysis = null;
    screenshotError = null;
  }

  // Hotkey event listeners
  let unlistenScreenshot: (() => void) | null = null;
  let unlistenToggleRecording: (() => void) | null = null;

  async function setupHotkeyListeners() {
    try {
      unlistenScreenshot = await listen("hotkey-screenshot", () => {
        console.log("[Hotkey] Screenshot triggered");
        if (isRecording) {
          captureAndAnalyzeScreenshot();
        }
      });

      unlistenToggleRecording = await listen("hotkey-toggle-recording", async () => {
        console.log("[Hotkey] Toggle recording triggered");
        if (isRecording) {
          await endMeeting();
        } else if (!currentMeetingId) {
          await startMeeting();
        }
      });

      console.log("Hotkey listeners registered");
    } catch (e) {
      console.error("Failed to setup hotkey listeners:", e);
    }
  }

  // Cleanup on destroy
  onDestroy(() => {
    if (unlistenTranscription) {
      unlistenTranscription();
    }
    if (unlistenScreenshot) {
      unlistenScreenshot();
    }
    if (unlistenToggleRecording) {
      unlistenToggleRecording();
    }
  });

  async function loadNotes() {
    try {
      notes = await invoke<Note[]>("get_notes", { limit: 50 });
    } catch (e) {
      console.error("Failed to load notes:", e);
    }
  }

  async function loadIntegrations() {
    try {
      integrations = await invoke<Integration[]>("get_integrations");
    } catch (e) {
      console.error("Failed to load integrations:", e);
    }
  }

  async function loadSettings() {
    try {
      userSettings = await invoke<UserSettings>("get_user_settings");
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  }

  async function loadMeetings() {
    try {
      meetings = await invoke<Meeting[]>("get_meetings", { limit: 50 });
    } catch (e) {
      console.error("Failed to load meetings:", e);
    }
  }

  async function loadActionItems() {
    try {
      actionItems = await invoke<ActionItem[]>("get_all_action_items", { limit: 50 });
    } catch (e) {
      console.error("Failed to load action items:", e);
      actionItems = [];
    }
  }

  async function loadDecisions() {
    try {
      decisions = await invoke<Decision[]>("get_all_decisions", { limit: 20 });
    } catch (e) {
      console.error("Failed to load decisions:", e);
      decisions = [];
    }
  }

  async function loadStats() {
    try {
      const stats = await invoke<{entity_counts: EntityCount[]; total_segments: number}>("get_knowledge_stats");
      entityCounts = stats.entity_counts || [];
      totalSegments = stats.total_segments || 0;
    } catch (e) {
      console.error("Failed to load stats:", e);
      entityCounts = [];
      totalSegments = 0;
    }
  }

  async function setupTranscriptionListener() {
    try {
      unlistenTranscription = await listen<LiveSegment>("transcription", (event) => {
        const segment = event.payload;
        if (segment.is_final) {
          // Add to live transcript
          liveTranscript = [...liveTranscript, segment];
          // Keep only last 50 segments to avoid memory issues
          if (liveTranscript.length > 50) {
            liveTranscript = liveTranscript.slice(-50);
          }
        }
      });
    } catch (e) {
      console.error("Failed to setup transcription listener:", e);
    }
  }

  // Get meeting ID as string
  function getMeetingIdString(meeting: Meeting): string {
    if (!meeting.id) return '';
    return `${meeting.id.tb}:${meeting.id.id.String}`;
  }

  // Format meeting date
  function formatMeetingDate(timestamp: number): string {
    const date = new Date(timestamp);
    const now = new Date();
    const diffDays = Math.floor((now.getTime() - date.getTime()) / (1000 * 60 * 60 * 24));

    if (diffDays === 0) {
      return `Today, ${date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
    } else if (diffDays === 1) {
      return 'Yesterday';
    } else if (diffDays < 7) {
      return date.toLocaleDateString([], { weekday: 'long' });
    } else {
      return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
    }
  }

  // Format meeting duration
  function formatMeetingDuration(startMs: number, endMs: number | null): string {
    if (!endMs) return 'In progress';
    const durationMs = endMs - startMs;
    const minutes = Math.floor(durationMs / 60000);
    if (minutes < 60) return `${minutes} min`;
    const hours = Math.floor(minutes / 60);
    const mins = minutes % 60;
    return `${hours}h ${mins}m`;
  }

  // Open meeting detail page
  function openMeeting(meeting: Meeting) {
    selectedMeetingId = getMeetingIdString(meeting);
  }

  // Close meeting detail page
  async function closeMeetingDetail() {
    selectedMeetingId = null;
    // Reload meetings to reflect any changes (deletions, updates, etc.)
    await loadMeetings();
  }

  // Cancel meeting prep (delete the meeting and reset state)
  function cancelMeetingPrep() {
    currentMeetingId = null;
    meetingTitle = "";
    meetingContext = "";
    linkedSources = [];
    availableSources = [];
  }

  // Start recording from prep page
  function startRecordingFromPrep() {
    // Recording has started, isRecording will become true via event listener
    // Just need to refresh state
  }

  // Format relative time
  function formatRelativeTime(isoDate: string): string {
    const date = new Date(isoDate);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins} min ago`;
    if (diffHours < 24) return `${diffHours} hours ago`;
    if (diffDays < 7) return `${diffDays} days ago`;
    return date.toLocaleDateString();
  }

  // Create a meeting for prep (without starting recording)
  async function createMeeting() {
    if (!meetingTitle.trim()) return;
    try {
      isLoading = true;
      // Clear live transcript for new meeting
      liveTranscript = [];

      // Create meeting in database (no recording yet)
      currentMeetingId = await invoke<string>("start_meeting", {
        title: meetingTitle,
        participants: []
      });

      console.log("Meeting created for prep:", currentMeetingId);
      // Load available knowledge sources for linking
      await loadAvailableSources();
    } catch (e) {
      console.error("Failed to create meeting:", e);
      currentMeetingId = null;
    } finally {
      isLoading = false;
    }
  }

  // Load available knowledge sources from KB
  async function loadAvailableSources() {
    try {
      availableSources = await invoke<KnowledgeSource[]>("get_knowledge_sources", { limit: 50 });
    } catch (e) {
      console.error("Failed to load knowledge sources:", e);
      availableSources = [];
    }
  }

  // Link a knowledge source to the current meeting
  async function linkSourceToMeeting(source: KnowledgeSource) {
    if (!currentMeetingId) return;
    const sourceId = getSourceId(source);
    if (!sourceId) return;

    try {
      await invoke("link_knowledge_to_meeting", {
        meetingId: currentMeetingId,
        sourceId: sourceId
      });
      // Add to linked if not already there
      if (!linkedSources.find(s => getSourceId(s) === sourceId)) {
        linkedSources = [...linkedSources, source];
      }
      console.log("Linked source to meeting:", sourceId);
    } catch (e) {
      console.error("Failed to link source:", e);
    }
  }

  // Remove a linked source (UI only for now)
  function unlinkSource(source: KnowledgeSource) {
    const sourceId = getSourceId(source);
    linkedSources = linkedSources.filter(s => getSourceId(s) !== sourceId);
  }

  // Fetch real-time suggestions from LLM
  async function fetchRealtimeSuggestions() {
    try {
      const suggestion = await invoke<RealtimeSuggestion>("get_realtime_suggestions", {
        meetingContext: meetingContext || null
      });
      // Only update if there's actual content
      if (suggestion.insight || suggestion.question || suggestion.related_info) {
        realtimeSuggestion = suggestion;
        lastSuggestionFetch = Date.now();
      }
    } catch (e) {
      console.error("Failed to fetch suggestions:", e);
    }
  }

  // Open the floating suggestions overlay window
  async function openSuggestionsOverlay() {
    try {
      console.log("Opening suggestions overlay...");

      // Check if overlay already exists
      const existing = await WebviewWindow.getByLabel('suggestions-overlay');
      if (existing) {
        console.log("Overlay already exists, focusing...");
        await existing.setFocus();
        return;
      }

      // Create new overlay window
      console.log("Creating new overlay window...");
      const overlay = new WebviewWindow('suggestions-overlay', {
        url: '/overlay',
        title: 'AI Assistant',
        width: 320,
        height: 400,
        minWidth: 280,
        minHeight: 200,
        resizable: true,
        decorations: true,
        titleBarStyle: 'overlay', // macOS overlay title bar (traffic lights only)
        hiddenTitle: true,
        alwaysOnTop: true,
        x: 50,
        y: 100,
        focus: false,
      });

      // Listen for window creation events
      overlay.once('tauri://created', () => {
        console.log("Suggestions overlay created successfully");
      });

      overlay.once('tauri://error', (e) => {
        console.error("Overlay window error:", e);
      });

    } catch (e) {
      console.error("Failed to open suggestions overlay:", e);
    }
  }

  // Close the floating suggestions overlay window
  async function closeSuggestionsOverlay() {
    try {
      const overlay = await WebviewWindow.getByLabel('suggestions-overlay');
      if (overlay) {
        await overlay.close();
        console.log("Suggestions overlay closed");
      }
    } catch (e) {
      console.error("Failed to close suggestions overlay:", e);
    }
  }

  // Start polling for suggestions (deprecated - using overlay now)
  function startSuggestionPolling() {
    // Now handled by overlay window
  }

  // Stop polling for suggestions (deprecated - using overlay now)
  function stopSuggestionPolling() {
    realtimeSuggestion = null;
  }

  // Start recording for current meeting (or create and start)
  async function startMeeting() {
    if (!meetingTitle.trim()) return;
    try {
      isLoading = true;

      // If no meeting exists yet, create one first
      if (!currentMeetingId) {
        // Clear live transcript for new meeting
        liveTranscript = [];
        realtimeSuggestion = null;

        currentMeetingId = await invoke<string>("start_meeting", {
          title: meetingTitle,
          participants: []
        });
      }

      // Start audio recording
      await invoke("start_recording");
      console.log("Recording started for meeting:", currentMeetingId);

      // Open floating suggestions overlay
      await openSuggestionsOverlay();
    } catch (e) {
      console.error("Failed to start recording:", e);
      // If recording failed but meeting was just created, clean up
      if (currentMeetingId && liveTranscript.length === 0) {
        try {
          await invoke("end_meeting", { summary: null });
        } catch (_) {}
        currentMeetingId = null;
      }
    } finally {
      isLoading = false;
    }
  }

  async function endMeeting() {
    if (!currentMeetingId) return;
    const meetingIdToProcess = currentMeetingId;
    try {
      isLoading = true;

      // 1. Close suggestions overlay (it also auto-closes on recording-stopped)
      stopSuggestionPolling();
      await closeSuggestionsOverlay();

      // 2. Stop audio recording
      await invoke("stop_recording");

      // 3. End meeting in database (triggers diarization)
      await invoke("end_meeting", { summary: null });

      // 4. Clear recording transcripts and meeting context
      await invoke("clear_recent_transcripts");
      await invoke("set_meeting_context", { context: null });

      // Store meeting ID before clearing
      const finishedMeetingId = currentMeetingId;

      // Clear current meeting state
      currentMeetingId = null;
      meetingTitle = "";
      liveTranscript = [];
      meetingContext = "";
      linkedSources = [];
      availableSources = [];
      realtimeSuggestion = null;

      // Reload meetings to show the completed meeting
      await loadMeetings();
      console.log("Meeting ended and recording stopped");

      // 5. Navigate to the finished meeting detail page
      selectedMeetingId = finishedMeetingId;

      // 6. Trigger post-meeting highlights extraction (async, in background)
      processPostMeetingHighlights(meetingIdToProcess);

    } catch (e) {
      console.error("Failed to end meeting:", e);
    } finally {
      isLoading = false;
    }
  }

  // Process meeting highlights after meeting ends (runs in background)
  async function processPostMeetingHighlights(meetingId: string) {
    try {
      console.log("Processing post-meeting highlights for:", meetingId);
      const highlights = await invoke<{
        summary: string | null;
        key_topics: string[];
        action_items: Array<{ task: string; assignee: string | null; deadline: string | null }>;
        decisions: string[];
        highlights: string[];
        follow_ups: string[];
      }>("process_meeting_highlights", { meetingId });

      console.log("Meeting highlights processed:", highlights);

      // Reload data to reflect new insights
      await loadActionItems();
      await loadDecisions();
      await loadMeetings();

      // Emit event so MeetingDetailPage can refresh
      window.dispatchEvent(new CustomEvent('meeting-highlights-ready', { detail: { meetingId } }));
    } catch (e) {
      console.error("Failed to process meeting highlights:", e);
    }
  }

  function getPriorityClass(priority: string) {
    return priority === 'high' ? 'bg-red-500/10 text-red-500 border-red-500/20' : 
           priority === 'medium' ? 'bg-amber-500/10 text-amber-500 border-amber-500/20' : 
           'bg-green-500/10 text-green-500 border-green-500/20';
  }

  // Create a new note
  async function createNote() {
    if (!newNoteContent.trim()) return;
    try {
      const tags = newNoteTags.trim()
        ? newNoteTags.split(',').map(t => t.trim()).filter(t => t)
        : [];
      const note = await invoke<Note>("create_note", {
        content: newNoteContent.trim(),
        tags
      });
      notes = [note, ...notes];
      newNoteContent = "";
      newNoteTags = "";
    } catch (e) {
      console.error("Failed to create note:", e);
    }
  }

  // Toggle note pin status
  async function togglePin(noteId: number) {
    try {
      const updatedNote = await invoke<Note>("toggle_note_pin", { id: noteId });
      notes = notes.map(n => n.id === noteId ? updatedNote : n);
    } catch (e) {
      console.error("Failed to toggle pin:", e);
    }
  }

  // Delete a note
  async function deleteNote(noteId: number) {
    try {
      await invoke("delete_note", { id: noteId });
      notes = notes.filter(n => n.id !== noteId);
    } catch (e) {
      console.error("Failed to delete note:", e);
    }
  }

  // Ask AI about meetings
  async function askAI() {
    if (!askQuery.trim() || aiLoading) return;
    try {
      aiLoading = true;
      aiResponse = "";
      const response = await invoke<string>("ask_assistant", { question: askQuery });
      aiResponse = response;
    } catch (e) {
      console.error("Failed to ask AI:", e);
      aiResponse = `Error: ${e}`;
    } finally {
      aiLoading = false;
    }
  }

  // Quick question shortcuts
  async function askQuickQuestion(question: string) {
    askQuery = question;
    await askAI();
  }

  // Connect/disconnect integration (placeholder for OAuth flows)
  async function toggleIntegration(toolId: string, toolName: string) {
    const existing = integrations.find(i => i.id === toolId);
    if (existing?.status === 'connected') {
      // Disconnect
      try {
        await invoke("disconnect_integration", { id: toolId });
        integrations = integrations.filter(i => i.id !== toolId);
      } catch (e) {
        console.error("Failed to disconnect:", e);
      }
    } else {
      // Connect (placeholder - in real app this would trigger OAuth)
      try {
        const integration: Integration = {
          id: toolId,
          name: toolName,
          status: 'connected',
          access_token: null,
          refresh_token: null,
          expires_at: null,
          metadata: null,
          connected_at: new Date().toISOString()
        };
        await invoke("upsert_integration", { integration });
        integrations = [...integrations.filter(i => i.id !== toolId), integration];
      } catch (e) {
        console.error("Failed to connect:", e);
      }
    }
  }
</script>

<div class="flex h-screen bg-[#1F1F1F] text-[#EBEBEB] font-sans overflow-hidden">
  <!-- Sidebar Navigation -->
  <nav class="w-64 bg-[#191919] border-r border-[#2C2C2C] flex flex-col font-medium">
    <div class="px-4 py-5 mb-2">
      <div class="flex items-center gap-2.5 text-zinc-100 hover:bg-[#2C2C2C] p-1.5 -ml-1.5 rounded transition-colors cursor-pointer">
        <div class="w-5 h-5 bg-indigo-500 rounded flex items-center justify-center text-[10px] font-bold text-white">S</div>
        <span class="font-medium text-sm tracking-tight text-[#EBEBEB]">Second Brain</span>
        <div class="ml-auto opacity-0 group-hover:opacity-100">
          <ChevronRight size={14} class="text-zinc-500" />
        </div>
      </div>
    </div>

    <div class="flex flex-col gap-0.5 px-3 flex-1 overflow-y-auto">
      <button 
        class="flex items-center gap-2.5 px-3 py-1.5 rounded text-[14px] transition-colors {activeView === 'home' ? 'bg-[#2C2C2C] text-[#EBEBEB]' : 'text-[#9B9B9B] hover:bg-[#2C2C2C] hover:text-[#EBEBEB]'}"
        onclick={() => activeView = 'home'}
      >
        <LayoutDashboard size={16} />
        <span>Home</span>
      </button>
      
      <button 
        class="flex items-center gap-2.5 px-3 py-1.5 rounded text-[14px] transition-colors group {activeView === 'meetings' ? 'bg-[#2C2C2C] text-[#EBEBEB]' : 'text-[#9B9B9B] hover:bg-[#2C2C2C] hover:text-[#EBEBEB]'}"
        onclick={() => activeView = 'meetings'}
      >
        <Calendar size={16} />
        <span class="flex-1 text-left">Meetings</span>
        {#if isRecording}
          <div class="w-2 h-2 rounded-full bg-red-500 animate-pulse"></div>
        {:else if currentMeetingId}
          <div class="w-2 h-2 rounded-full bg-amber-500"></div>
        {/if}
      </button>

      <button 
        class="flex items-center gap-2.5 px-3 py-1.5 rounded text-[14px] transition-colors {activeView === 'notes' ? 'bg-[#2C2C2C] text-[#EBEBEB]' : 'text-[#9B9B9B] hover:bg-[#2C2C2C] hover:text-[#EBEBEB]'}"
        onclick={() => activeView = 'notes'}
      >
        <FileText size={16} />
        <span>Notes</span>
      </button>

      <button 
        class="flex items-center gap-2.5 px-3 py-1.5 rounded text-[14px] transition-colors {activeView === 'insights' ? 'bg-[#2C2C2C] text-[#EBEBEB]' : 'text-[#9B9B9B] hover:bg-[#2C2C2C] hover:text-[#EBEBEB]'}"
        onclick={() => activeView = 'insights'}
      >
        <Sparkles size={16} />
        <span class="flex-1 text-left">Insights</span>
      </button>

      <button
        class="flex items-center gap-2.5 px-3 py-1.5 rounded text-[14px] transition-colors {activeView === 'knowledge' ? 'bg-[#2C2C2C] text-[#EBEBEB]' : 'text-[#9B9B9B] hover:bg-[#2C2C2C] hover:text-[#EBEBEB]'}"
        onclick={() => activeView = 'knowledge'}
      >
        <Library size={16} />
        <span>Knowledge Base</span>
      </button>

      <button
        class="flex items-center gap-2.5 px-3 py-1.5 rounded text-[14px] transition-colors {activeView === 'tools' ? 'bg-[#2C2C2C] text-[#EBEBEB]' : 'text-[#9B9B9B] hover:bg-[#2C2C2C] hover:text-[#EBEBEB]'}"
        onclick={() => activeView = 'tools'}
      >
        <Blocks size={16} />
        <span>Integrations</span>
      </button>
    </div>

    <div class="p-3">
      <div class="group flex items-center gap-2 px-3 py-2 rounded transition-colors text-[#9B9B9B] hover:text-[#EBEBEB] cursor-text" onclick={() => document.getElementById('quick-note-input')?.focus()}>
        <Plus size={16} />
        <input
          id="quick-note-input"
          type="text"
          placeholder="New Note..."
          class="bg-transparent border-none outline-none text-[14px] w-full placeholder:text-[#9B9B9B] group-hover:placeholder:text-[#EBEBEB]"
          bind:value={newNoteContent}
          onkeydown={(e) => e.key === 'Enter' && createNote()}
        />
      </div>
    </div>
  </nav>

  <!-- Main Content -->
  <main class="flex-1 bg-zinc-950 overflow-hidden flex flex-col relative">
    <div class="absolute inset-0 overflow-y-auto">
    <!-- Meeting Detail Page (overlays everything when a meeting is selected) -->
    {#if selectedMeetingId}
      <MeetingDetailPage meetingId={selectedMeetingId} onBack={closeMeetingDetail} />
    {:else if currentMeetingId && !isRecording}
      <!-- Meeting Prep Page (full page prep flow) -->
      <MeetingPrepPage
        meetingId={currentMeetingId}
        initialTitle={meetingTitle}
        onCancel={cancelMeetingPrep}
        onStartRecording={startRecordingFromPrep}
      />
    {:else if activeView === 'home'}
      <!-- HOME VIEW -->
      <div class="max-w-5xl mx-auto p-8 space-y-8">
        <header class="pb-6 border-b border-[#2C2C2C]">
          <div class="flex items-end justify-between mb-4">
            <div>
              <h1 class="text-2xl font-semibold tracking-tight text-[#EBEBEB] mb-1">Dashboard</h1>
              <p class="text-[#9B9B9B] text-sm">Overview of your day</p>
            </div>
          </div>
          <!-- Ask AI - Primary action at top -->
          <div class="bg-[#252525] rounded-lg p-1.5 pl-4 flex items-center gap-3 ring-1 ring-[#333333] focus-within:ring-indigo-500/50 transition-all">
            <Sparkles size={16} class="text-indigo-400" />
            <input
              type="text"
              placeholder="Ask anything about your meetings, notes, or knowledge base..."
              bind:value={askQuery}
              onkeydown={(e) => e.key === 'Enter' && askAI()}
              class="bg-transparent border-none outline-none flex-1 text-sm text-[#EBEBEB] placeholder:text-[#666666] h-9"
            />
            <button
              class="px-4 py-1.5 bg-indigo-500 hover:bg-indigo-400 text-white text-xs font-medium rounded transition-colors disabled:opacity-50"
              onclick={askAI}
              disabled={aiLoading || !askQuery.trim()}
            >
              {aiLoading ? 'Thinking...' : 'Ask AI'}
            </button>
          </div>
          <!-- AI Response -->
          {#if aiLoading}
            <div class="mt-4 px-4 py-3 bg-[#252525] rounded-lg border border-[#333333] animate-pulse">
              <p class="text-sm text-[#9B9B9B]">Thinking...</p>
            </div>
          {:else if aiResponse}
            <div class="mt-4 px-4 py-3 bg-[#252525] rounded-lg border border-indigo-500/20">
              <p class="text-sm text-[#CCCCCC] whitespace-pre-wrap leading-relaxed">{aiResponse}</p>
            </div>
          {/if}
        </header>

        <!-- Active Recording Banner with Live Transcript -->
        {#if isRecording}
          <div class="bg-[#252525] border border-red-900/20 rounded-lg overflow-hidden">
            <div class="p-4 flex items-center justify-between border-b border-[#333333]">
              <div class="flex items-center gap-3">
                <div class="w-2 h-2 rounded-full bg-red-500 animate-pulse"></div>
                <div>
                  <span class="block text-[10px] font-bold text-red-500 uppercase tracking-wider">Recording</span>
                  <span class="text-sm font-medium text-[#EBEBEB]">{meetingTitle || 'Unnamed Meeting'}</span>
                </div>
              </div>
              <div class="flex items-center gap-2">
                <button
                  class="px-3 py-1.5 bg-indigo-500/10 hover:bg-indigo-500/20 text-indigo-400 border-indigo-500/20 text-xs font-medium rounded border transition-colors flex items-center gap-2 disabled:opacity-50"
                  onclick={() => captureAndAnalyzeScreenshot()}
                  disabled={isCapturingScreenshot}
                  title="Capture screenshot and analyze with AI (Cmd/Ctrl+Shift+S)"
                >
                  {#if isCapturingScreenshot}
                    <Loader2 size={12} class="animate-spin" />
                  {:else}
                    <Camera size={12} />
                  {/if}
                  Screenshot
                </button>
                <button
                  class="px-3 py-1.5 bg-red-500/10 hover:bg-red-500/20 text-red-500 border-red-500/10 text-xs font-medium rounded border transition-colors flex items-center gap-2"
                  onclick={endMeeting}
                >
                  <Square size={12} fill="currentColor" />
                  End Meeting
                </button>
              </div>
            </div>
            <!-- Live Transcript -->
            <div class="p-4 max-h-48 overflow-y-auto bg-[#1F1F1F]">
              {#if liveTranscript.length > 0}
                <div class="space-y-3">
                  {#each liveTranscript.slice(-10) as segment}
                    <div class="flex gap-4 text-sm items-start group">
                      <span class="font-medium {segment.source === 'microphone' ? 'text-[#9B9B9B]' : 'text-[#777777]'} text-xs uppercase tracking-wide flex-shrink-0 w-12 text-right mt-0.5">
                        {segment.source === 'microphone' ? 'You' : 'Guest'}
                      </span>
                      <div class="flex-1">
                        <span class="text-[#CCCCCC] leading-relaxed">{segment.text}</span>
                        <!-- Emotion and event indicators -->
                        <div class="flex gap-1.5 mt-1.5 opacity-60 group-hover:opacity-100 transition-opacity">
                          {#if segment.emotion && segment.emotion !== 'Neutral'}
                            <span class="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/10 flex items-center gap-1">
                              {segment.emotion}
                            </span>
                          {/if}
                          {#if segment.audio_events?.includes('Laughter')}
                            <span class="text-[10px] px-1.5 py-0.5 rounded bg-green-500/10 text-green-400 border border-green-500/10">Laughter</span>
                          {/if}
                          {#if segment.audio_events?.includes('Applause')}
                            <span class="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/10">Applause</span>
                          {/if}
                        </div>
                      </div>
                    </div>
                  {/each}
                </div>
              {:else}
                <div class="text-center text-[#666666] text-sm py-8 flex flex-col items-center gap-2">
                  <div class="w-2 h-2 rounded-full bg-red-500/50 animate-ping"></div>
                  Listening for speech...
                </div>
              {/if}
            </div>
            <!-- Real-time AI Suggestions -->
            {#if realtimeSuggestion && (realtimeSuggestion.insight || realtimeSuggestion.question || realtimeSuggestion.related_info)}
              <div class="p-4 border-t border-[#333333] bg-gradient-to-r from-indigo-950/20 to-purple-950/20">
                <div class="flex items-center gap-2 mb-3">
                  <Sparkles size={14} class="text-indigo-400" />
                  <span class="text-xs font-medium text-indigo-400 uppercase tracking-wider">AI Suggestions</span>
                </div>
                <div class="space-y-3">
                  {#if realtimeSuggestion.insight}
                    <div class="flex gap-3">
                      <span class="text-[10px] font-medium text-amber-400 uppercase tracking-wide flex-shrink-0 mt-0.5">Insight</span>
                      <p class="text-sm text-[#CCCCCC] leading-relaxed">{realtimeSuggestion.insight}</p>
                    </div>
                  {/if}
                  {#if realtimeSuggestion.question}
                    <div class="flex gap-3">
                      <span class="text-[10px] font-medium text-emerald-400 uppercase tracking-wide flex-shrink-0 mt-0.5">Ask</span>
                      <p class="text-sm text-[#CCCCCC] leading-relaxed italic">"{realtimeSuggestion.question}"</p>
                    </div>
                  {/if}
                  {#if realtimeSuggestion.related_info}
                    <div class="flex gap-3">
                      <span class="text-[10px] font-medium text-blue-400 uppercase tracking-wide flex-shrink-0 mt-0.5">Related</span>
                      <p class="text-sm text-[#AAAAAA] leading-relaxed">{realtimeSuggestion.related_info}</p>
                    </div>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        {:else}
          <div class="bg-gradient-to-r from-[#252525] to-[#1F1F1F] border border-[#2C2C2C] rounded-lg p-6 flex items-center justify-between">
            <div class="flex items-center gap-5">
              <div class="w-12 h-12 rounded-full bg-[#333333] flex items-center justify-center text-[#EBEBEB]">
                <Mic size={20} />
              </div>
              <div>
                <h3 class="text-base font-medium text-[#EBEBEB]">Start a new meeting</h3>
                <p class="text-sm text-[#9B9B9B]">Record and transcribe instantly</p>
              </div>
            </div>
            <div class="flex items-center gap-3">
              <input
                type="text"
                placeholder="Meeting title..."
                class="bg-[#191919] border border-[#333333] rounded-md px-3 py-2 text-sm w-64 focus:outline-none focus:border-[#555555] transition-colors text-[#EBEBEB] placeholder:text-[#555555]"
                bind:value={meetingTitle}
              />
              <button
                class="px-4 py-2 bg-[#333333] text-[#EBEBEB] hover:bg-[#444444] text-sm font-medium rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2"
                onclick={createMeeting}
                disabled={!meetingTitle.trim()}
                title="Create meeting to add context/docs before recording"
              >
                <FileText size={14} />
                Prepare
              </button>
              <button
                class="px-4 py-2 bg-[#EBEBEB] text-black hover:bg-white text-sm font-medium rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2"
                onclick={startMeeting}
                disabled={!meetingTitle.trim()}
              >
                <div class="w-2 h-2 rounded-full bg-red-500"></div>
                Start Recording
              </button>
            </div>
          </div>
        {/if}

        <div class="grid grid-cols-12 gap-8">
          <!-- Upcoming / Today -->
          <section class="col-span-4 flex flex-col h-full">
            <div class="flex items-center justify-between mb-4">
              <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider">Action Items</h2>
              {#if actionItems.length > 0}
                <button class="text-xs text-[#9B9B9B] hover:text-[#EBEBEB]" onclick={() => activeView = 'insights'}>View all</button>
              {/if}
            </div>
            <div class="space-y-1">
              {#if upcomingItems().length > 0}
                {#each upcomingItems() as item}
                  <div class="flex items-start gap-3 p-2.5 rounded-md hover:bg-[#2C2C2C] transition-colors cursor-pointer group">
                    <span class="text-[#9B9B9B] mt-0.5 group-hover:text-[#EBEBEB]">
                      {#if item.type === 'meeting'}
                        <Calendar size={15} />
                      {:else if item.type === 'action'}
                        <CheckCircle2 size={15} />
                      {:else if item.type === 'followup'}
                        <MessageSquare size={15} />
                      {/if}
                    </span>
                    <div class="flex-1 min-w-0">
                      <p class="text-sm font-medium text-[#EBEBEB] truncate">{item.title}</p>
                      <p class="text-xs text-[#9B9B9B]">{item.time}</p>
                    </div>
                    {#if item.priority}
                      <div class="w-1.5 h-1.5 rounded-full mt-2 {item.priority === 'high' ? 'bg-red-500' : item.priority === 'medium' ? 'bg-amber-500' : 'bg-green-500'}"></div>
                    {/if}
                  </div>
                {/each}
              {:else}
                <div class="text-center py-8 text-[#666666]">
                  <CheckCircle2 size={24} class="mx-auto mb-2 opacity-50" />
                  <p class="text-sm">No action items</p>
                  <p class="text-xs mt-1">Items from meetings will appear here</p>
                </div>
              {/if}
            </div>
          </section>

          <!-- AI Insights -->
          <section class="col-span-4 flex flex-col h-full">
            <div class="flex items-center justify-between mb-4">
              <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider">Insights</h2>
            </div>
            <div class="space-y-3">
              {#each insights() as insight}
                <div class="p-3.5 rounded-md bg-[#252525] hover:bg-[#2C2C2C] transition-colors cursor-pointer border border-transparent hover:border-[#333333]">
                  <div class="flex items-center gap-2 mb-1.5">
                    <span class="text-[#9B9B9B]">
                      {#if insight.type === 'pattern'}<Hash size={12} />
                      {:else if insight.type === 'commitment'}<AlertTriangle size={12} />
                      {:else}<LinkIcon size={12} />
                      {/if}
                    </span>
                    <span class="text-[10px] font-medium text-[#777777] uppercase">{insight.type}</span>
                  </div>
                  <h3 class="text-sm font-medium text-[#EBEBEB] mb-1">{insight.title}</h3>
                  <p class="text-xs text-[#9B9B9B] leading-relaxed">{insight.description}</p>
                </div>
              {/each}
            </div>
          </section>

          <!-- Recent Meetings -->
          <section class="col-span-4 flex flex-col h-full">
            <div class="flex items-center justify-between mb-4">
              <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider">Recent Meetings</h2>
              <button class="text-xs text-[#9B9B9B] hover:text-[#EBEBEB] transition-colors" onclick={() => activeView = 'meetings'}>View all</button>
            </div>
            <div class="space-y-2">
              {#if meetings.length > 0}
                {#each meetings.slice(0, 3) as meeting}
                  <button
                    class="w-full text-left p-2.5 rounded-md hover:bg-[#2C2C2C] transition-colors cursor-pointer group"
                    onclick={() => openMeeting(meeting)}
                  >
                    <div class="flex justify-between items-start mb-1">
                      <h3 class="text-sm font-medium text-[#EBEBEB] truncate pr-2 group-hover:text-white">{meeting.title}</h3>
                      <span class="text-xs text-[#777777] whitespace-nowrap">{formatMeetingDate(meeting.start_time)}</span>
                    </div>
                    <div class="flex items-center gap-3 text-xs text-[#9B9B9B]">
                      <span class="flex items-center gap-1.5">
                        <Clock size={12} /> {formatMeetingDuration(meeting.start_time, meeting.end_time)}
                      </span>
                    </div>
                  </button>
                {/each}
              {:else}
                <div class="text-center py-6 text-[#666666]">
                  <p class="text-sm">No meetings yet</p>
                </div>
              {/if}
            </div>
          </section>

        </div>
      </div>

    {:else if activeView === 'meetings'}
      <!-- MEETINGS VIEW -->
      <div class="max-w-5xl mx-auto p-8">
        <header class="flex items-center justify-between mb-8 border-b border-[#2C2C2C] pb-6">
          <div>
            <h1 class="text-2xl font-semibold text-[#EBEBEB]">Meetings</h1>
            <p class="text-sm text-[#9B9B9B] mt-1">{meetings.length} meetings recorded</p>
          </div>
          <div class="relative w-64">
            <span class="absolute left-3 top-1/2 -translate-y-1/2 text-[#9B9B9B]">
              <Search size={14} />
            </span>
            <input type="text" placeholder="Search meetings..." class="w-full bg-[#252525] rounded-sm py-2 pl-9 pr-4 text-sm text-[#EBEBEB] focus:outline-none focus:ring-1 focus:ring-[#444444] transition-colors" bind:value={searchQuery} />
          </div>
        </header>

        <div class="space-y-1">
          <div class="grid grid-cols-12 px-4 py-2 text-xs font-medium text-[#777777] uppercase tracking-wider border-b border-[#2C2C2C] mb-2">
            <div class="col-span-6">Title</div>
            <div class="col-span-3">Date</div>
            <div class="col-span-3">Duration</div>
          </div>
          {#if meetings.length > 0}
            {#each meetings.filter(m => !searchQuery || m.title.toLowerCase().includes(searchQuery.toLowerCase())) as meeting}
              <button
                class="w-full text-left grid grid-cols-12 items-center px-4 py-3 rounded-md hover:bg-[#2C2C2C] transition-all group border border-transparent"
                onclick={() => openMeeting(meeting)}
              >
                <div class="col-span-6 flex items-center gap-3">
                  {#if !meeting.end_time}
                    <div class="w-2 h-2 rounded-full bg-red-500 animate-pulse"></div>
                  {:else}
                    <div class="w-2 h-2 rounded-full bg-[#333333]"></div>
                  {/if}
                  <span class="font-medium text-[#EBEBEB] group-hover:text-white truncate">{meeting.title}</span>
                </div>
                <div class="col-span-3 text-sm text-[#9B9B9B]">
                  {formatMeetingDate(meeting.start_time)}
                </div>
                <div class="col-span-3 text-sm text-[#9B9B9B] flex items-center gap-2">
                  <Clock size={12} />
                  {formatMeetingDuration(meeting.start_time, meeting.end_time)}
                </div>
              </button>
            {/each}
          {:else}
            <div class="text-center py-16 border-2 border-dashed border-[#2C2C2C] rounded-lg">
              <p class="text-[#666666]">No meetings recorded yet</p>
            </div>
          {/if}
        </div>
      </div>

    {:else if activeView === 'notes'}
      <!-- NOTES VIEW -->
      <div class="max-w-5xl mx-auto p-8 h-full flex flex-col">
        <header class="flex items-center justify-between mb-6 border-b border-[#2C2C2C] pb-6">
          <div>
            <h1 class="text-2xl font-semibold text-[#EBEBEB]">Notes</h1>
            <p class="text-sm text-[#9B9B9B] mt-1">{notes.length} notes</p>
          </div>
        </header>

        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 pb-8">
          <!-- Add Note Card -->
          <div class="bg-[#252525] rounded-lg p-4 flex flex-col gap-3 hover:bg-[#2A2A2A] transition-colors group">
            <textarea
              placeholder="Write a note..."
              class="bg-transparent border-none outline-none resize-none text-sm text-[#EBEBEB] placeholder:text-[#666666] h-24"
              bind:value={newNoteContent}
            ></textarea>
            <div class="flex items-center gap-2">
              <input
                type="text"
                placeholder="Tags..."
                class="bg-[#1F1F1F] rounded px-2 py-1 text-xs text-[#EBEBEB] w-full border-none focus:ring-1 focus:ring-[#444444] outline-none"
                bind:value={newNoteTags}
              />
              <button
                class="px-3 py-1 bg-[#EBEBEB] hover:bg-white text-black text-xs font-medium rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                onclick={createNote}
                disabled={!newNoteContent.trim()}
              >Save</button>
            </div>
          </div>

          <!-- Notes List -->
          {#each notes as note (note.id)}
            <div class="group relative bg-[#1F1F1F] border border-[#2C2C2C] rounded-lg p-4 hover:border-[#444444] transition-all flex flex-col h-48">
              <div class="flex justify-between items-start mb-2">
                  <div class="flex gap-1">
                  {#if note.pinned}
                    <div class="text-[#EBEBEB] transform -rotate-45">
                      <LinkIcon size={12} />
                    </div>
                  {/if}
                </div>
                <div class="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button 
                    class="p-1 hover:bg-[#333333] rounded text-[#9B9B9B] hover:text-[#EBEBEB]" 
                    onclick={() => togglePin(note.id)} 
                    title={note.pinned ? "Unpin" : "Pin"}
                  >
                     <LinkIcon size={14} />
                  </button>
                  <button 
                    class="p-1 hover:bg-red-500/10 rounded text-[#9B9B9B] hover:text-red-400" 
                    onclick={() => deleteNote(note.id)} 
                    title="Delete"
                  >
                    <div class="w-3.5 h-3.5"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></div>
                  </button>
                </div>
              </div>
              <p class="text-sm text-[#CCCCCC] whitespace-pre-wrap line-clamp-4 flex-1 font-normal leading-relaxed">{note.content}</p>
              <div class="mt-3 flex items-center justify-between">
                <span class="text-[10px] text-[#666666]">{formatRelativeTime(note.created_at)}</span>
                <div class="flex gap-1">
                  {#each note.tags as tag}
                    <span class="text-[10px] px-1.5 py-0.5 rounded bg-[#2C2C2C] text-[#9B9B9B]">#{tag}</span>
                  {/each}
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>

    {:else if activeView === 'insights'}
      <!-- INSIGHTS VIEW -->
      <div class="max-w-5xl mx-auto p-8">
        <header class="mb-8 border-b border-[#2C2C2C] pb-6">
          <h1 class="text-2xl font-semibold text-[#EBEBEB]">Insights</h1>
          <p class="text-[#9B9B9B] mt-1">
            {#if meetings.length > 0}
              Patterns and action items from {meetings.length} meeting{meetings.length > 1 ? 's' : ''}
            {:else}
              Record meetings to discover patterns and track action items
            {/if}
          </p>
        </header>

        <div class="grid gap-8">
          <!-- Summary Stats -->
          <section class="grid grid-cols-4 gap-4">
            <div class="bg-[#252525] rounded-lg p-4 text-center">
              <p class="text-3xl font-bold text-[#EBEBEB]">{meetings.length}</p>
              <p class="text-xs text-[#9B9B9B] mt-1">Meetings</p>
            </div>
            <div class="bg-[#252525] rounded-lg p-4 text-center">
              <p class="text-3xl font-bold text-[#EBEBEB]">{actionItems.length}</p>
              <p class="text-xs text-[#9B9B9B] mt-1">Action Items</p>
            </div>
            <div class="bg-[#252525] rounded-lg p-4 text-center">
              <p class="text-3xl font-bold text-[#EBEBEB]">{decisions.length}</p>
              <p class="text-xs text-[#9B9B9B] mt-1">Decisions</p>
            </div>
            <div class="bg-[#252525] rounded-lg p-4 text-center">
              <p class="text-3xl font-bold text-[#EBEBEB]">{totalSegments}</p>
              <p class="text-xs text-[#9B9B9B] mt-1">Transcript Segments</p>
            </div>
          </section>

          <!-- Action Items -->
          <section>
            <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4">Action Items</h2>
            {#if actionItems.length > 0}
              <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                {#each actionItems as action}
                  <div class="bg-[#252525] border border-transparent rounded-lg p-5 hover:bg-[#2C2C2C] transition-all group">
                    <div class="flex justify-between items-start mb-3">
                      <span class="text-[10px] font-bold px-2 py-1 rounded uppercase tracking-wide {action.status === 'completed' ? 'bg-green-500/10 text-green-500' : 'bg-amber-500/10 text-amber-500'}">
                        {action.status}
                      </span>
                    </div>
                    <p class="text-[#EBEBEB] font-medium mb-4">{action.text}</p>
                    <div class="flex items-center justify-between text-xs text-[#9B9B9B] border-t border-[#333333] pt-3">
                      <div class="flex items-center gap-3">
                        {#if action.assignee}
                          <span class="flex items-center gap-1"><div class="w-1.5 h-1.5 rounded-full bg-[#666666]"></div> {action.assignee}</span>
                        {/if}
                        {#if action.deadline}
                          <span class="flex items-center gap-1"><Calendar size={12} /> {action.deadline}</span>
                        {/if}
                      </div>
                      <span class="text-[#666666]">{action.meeting_title}</span>
                    </div>
                  </div>
                {/each}
              </div>
            {:else}
              <div class="bg-[#252525] rounded-lg p-8 text-center">
                <CheckCircle2 size={32} class="mx-auto mb-3 text-[#444444]" />
                <p class="text-[#9B9B9B]">No action items yet</p>
                <p class="text-xs text-[#666666] mt-1">Action items from your meetings will appear here</p>
              </div>
            {/if}
          </section>

          <!-- Decisions -->
          <section>
            <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4">Recent Decisions</h2>
            {#if decisions.length > 0}
              <div class="space-y-3">
                {#each decisions as decision}
                  <div class="bg-[#252525] rounded-lg p-4 hover:bg-[#2C2C2C] transition-all">
                    <p class="text-[#EBEBEB]">{decision.text}</p>
                    <p class="text-xs text-[#666666] mt-2">From: {decision.meeting_title}</p>
                  </div>
                {/each}
              </div>
            {:else}
              <div class="bg-[#252525] rounded-lg p-8 text-center">
                <Hash size={32} class="mx-auto mb-3 text-[#444444]" />
                <p class="text-[#9B9B9B]">No decisions recorded</p>
                <p class="text-xs text-[#666666] mt-1">Key decisions from your meetings will appear here</p>
              </div>
            {/if}
          </section>

          <!-- Patterns & Insights -->
          <section>
            <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4">Patterns & Insights</h2>
            <div class="space-y-4">
              {#each insights() as insight}
                <div class="bg-[#252525] border border-transparent rounded-lg p-5 flex items-start gap-4 hover:bg-[#2C2C2C] transition-all group">
                  <div class="w-10 h-10 rounded bg-[#333333] flex items-center justify-center text-[#EBEBEB]">
                     {#if insight.type === 'pattern'}<Hash size={20} />
                     {:else if insight.type === 'commitment'}<AlertTriangle size={20} />
                     {:else}<LinkIcon size={20} />
                     {/if}
                  </div>
                  <div class="flex-1">
                    <h3 class="text-base font-medium text-[#EBEBEB] mb-1">{insight.title}</h3>
                    <p class="text-sm text-[#9B9B9B] leading-relaxed">{insight.description}</p>
                  </div>
                </div>
              {/each}
            </div>
          </section>
        </div>
      </div>

    {:else if activeView === 'tools'}
      <!-- INTEGRATIONS & SETTINGS VIEW -->
      <div class="max-w-5xl mx-auto p-8">
        <header class="mb-8 border-b border-[#2C2C2C] pb-6">
          <h1 class="text-2xl font-semibold text-[#EBEBEB]">Settings & Integrations</h1>
          <p class="text-[#9B9B9B] mt-1">Configure your Second Brain and connect your tools</p>
        </header>

        <!-- Settings Section -->
        <section class="mb-12">
          <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4">Settings</h2>
          <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
            <!-- Stealth Mode -->
            <div class="bg-[#252525] border border-[#333333] rounded-lg p-6">
              <div class="flex items-start justify-between">
                <div class="flex items-start gap-4">
                  <div class="p-2.5 bg-[#1F1F1F] rounded-lg">
                    <Shield size={20} class="text-indigo-400" />
                  </div>
                  <div>
                    <h3 class="text-base font-medium text-[#EBEBEB] mb-1">Stealth Mode</h3>
                    <p class="text-sm text-[#9B9B9B] leading-relaxed">
                      Hide this window from screen recordings and screen shares. Keeps your meetings private.
                    </p>
                  </div>
                </div>
                <button
                  class="relative w-12 h-6 rounded-full transition-colors {stealthModeEnabled ? 'bg-indigo-500' : 'bg-[#444444]'}"
                  onclick={toggleStealthMode}
                >
                  <div class="absolute top-1 w-4 h-4 rounded-full bg-white transition-all {stealthModeEnabled ? 'left-7' : 'left-1'}"></div>
                </button>
              </div>
              <div class="mt-4 pt-4 border-t border-[#333333] flex items-center gap-2 text-xs text-[#666666]">
                {#if stealthModeEnabled}
                  <EyeOff size={14} class="text-indigo-400" />
                  <span>Window is hidden from screen recordings</span>
                {:else}
                  <Eye size={14} />
                  <span>Window is visible in screen recordings</span>
                {/if}
              </div>
            </div>

            <!-- LLM Configuration -->
            <div class="bg-[#252525] border border-[#333333] rounded-lg p-6">
              <div class="flex items-start gap-4 mb-4">
                <div class="p-2.5 bg-[#1F1F1F] rounded-lg">
                  <Server size={20} class="text-indigo-400" />
                </div>
                <div class="flex-1">
                  <h3 class="text-base font-medium text-[#EBEBEB] mb-1">LLM Connection</h3>
                  <p class="text-sm text-[#9B9B9B]">
                    OpenAI-compatible API endpoint for AI features
                  </p>
                </div>
              </div>

              <div class="space-y-3">
                <div>
                  <label class="block text-xs text-[#666666] mb-1.5">API Endpoint URL</label>
                  <input
                    type="text"
                    placeholder="http://localhost:1234/v1"
                    bind:value={llmUrl}
                    class="w-full bg-[#1F1F1F] border border-[#333333] rounded-lg px-3 py-2 text-sm text-[#EBEBEB] placeholder:text-[#555555] focus:outline-none focus:border-indigo-500/50"
                  />
                </div>
                <div>
                  <label class="block text-xs text-[#666666] mb-1.5">API Key <span class="text-[#555555]">(optional for local servers)</span></label>
                  <input
                    type="password"
                    placeholder="sk-... or leave empty for local"
                    bind:value={llmApiKey}
                    class="w-full bg-[#1F1F1F] border border-[#333333] rounded-lg px-3 py-2 text-sm text-[#EBEBEB] placeholder:text-[#555555] focus:outline-none focus:border-indigo-500/50"
                  />
                </div>
                <div>
                  <label class="block text-xs text-[#666666] mb-1.5">Model Name</label>
                  <input
                    type="text"
                    placeholder="gpt-4o-mini, llama3.2, etc."
                    bind:value={llmModel}
                    class="w-full bg-[#1F1F1F] border border-[#333333] rounded-lg px-3 py-2 text-sm text-[#EBEBEB] placeholder:text-[#555555] focus:outline-none focus:border-indigo-500/50"
                  />
                </div>
              </div>

              {#if llmTestStatus !== "idle"}
                <div class="mt-3">
                  {#if llmTestStatus === "success"}
                    <div class="flex items-center gap-2 text-xs text-green-400 bg-green-500/10 rounded-lg p-2 border border-green-500/20">
                      <CheckCircle2 size={14} />
                      Connection successful
                    </div>
                  {:else if llmTestStatus === "error"}
                    <div class="text-xs text-red-400 bg-red-500/10 rounded-lg p-2 border border-red-500/20">
                      <div class="flex items-center gap-2">
                        <AlertTriangle size={14} />
                        Connection failed
                      </div>
                      {#if llmTestError}
                        <p class="mt-1 text-red-400/80 ml-5 truncate">{llmTestError}</p>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}

              <button
                class="mt-4 w-full py-2 px-4 bg-indigo-500 hover:bg-indigo-400 text-white text-sm font-medium rounded-lg transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
                onclick={saveLLMSettings}
                disabled={isTestingLLM || !llmUrl.trim()}
              >
                {#if isTestingLLM}
                  <Loader2 size={14} class="animate-spin" />
                  Testing...
                {:else}
                  <Zap size={14} />
                  Save & Test Connection
                {/if}
              </button>
            </div>
          </div>
        </section>

        <!-- Integrations Section -->
        <section>
          <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4">Integrations</h2>
          <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {#each connectedTools as tool}
              <div class="bg-[#252525] border border-transparent hover:border-[#333333] rounded-lg p-6 flex flex-col gap-4 transition-all">
                <div class="flex justify-between items-start">
                  <span class="text-[#EBEBEB]">
                      {#if tool.id === 'calendar'}<Calendar size={24} />
                      {:else if tool.id === 'slack'}<MessageSquare size={24} />
                      {:else if tool.id === 'notion'}<FileText size={24} />
                      {:else if tool.id === 'linear'}<CheckCircle2 size={24} />
                      {:else if tool.id === 'github'}<Command size={24} />
                      {/if}
                  </span>
                  <span class="text-[10px] font-medium px-2 py-1 rounded-full {tool.status === 'connected' ? 'bg-green-500/10 text-green-500' : 'bg-[#333333] text-[#777777]'}">
                    {tool.status === 'connected' ? 'Connected' : 'Available'}
                  </span>
                </div>
                <div>
                  <h3 class="text-lg font-medium text-[#EBEBEB]">{tool.name}</h3>
                  <p class="text-sm text-[#9B9B9B] mt-1">Sync meetings and docs</p>
                </div>
                <button
                  class="mt-auto w-full py-2 rounded text-sm font-medium transition-colors {tool.status === 'connected' ? 'bg-[#333333] text-[#EBEBEB] hover:bg-[#444444]' : 'bg-[#EBEBEB] text-black hover:bg-white'}"
                  onclick={() => toggleIntegration(tool.id, tool.name)}
                >
                  {tool.status === 'connected' ? 'Disconnect' : 'Connect'}
                </button>
              </div>
            {/each}
          </div>
        </section>
      </div>

    {:else if activeView === 'knowledge'}
      <!-- KNOWLEDGE BASE VIEW -->
      <KnowledgeBaseView />
    {/if}
    </div>

    <!-- Screenshot Analysis Result Overlay -->
    {#if showScreenshotResult}
      <div class="absolute bottom-4 right-4 w-96 bg-[#252525] border border-[#333333] rounded-lg shadow-xl overflow-hidden z-50">
        <div class="flex items-center justify-between px-4 py-3 bg-[#1F1F1F] border-b border-[#333333]">
          <div class="flex items-center gap-2">
            <ImageIcon size={14} class="text-indigo-400" />
            <span class="text-sm font-medium text-[#EBEBEB]">Screenshot Analysis</span>
          </div>
          <button
            class="p-1 hover:bg-[#333333] rounded text-[#9B9B9B] hover:text-[#EBEBEB] transition-colors"
            onclick={dismissScreenshotResult}
          >
            <X size={14} />
          </button>
        </div>
        <div class="p-4 max-h-64 overflow-y-auto">
          {#if isCapturingScreenshot}
            <div class="flex items-center gap-3 text-[#9B9B9B]">
              <Loader2 size={16} class="animate-spin text-indigo-400" />
              <span class="text-sm">Analyzing screenshot...</span>
            </div>
          {:else if screenshotError}
            <div class="text-sm text-red-400 bg-red-500/10 rounded-lg p-3 border border-red-500/20">
              <div class="flex items-center gap-2 mb-1">
                <AlertTriangle size={14} />
                <span class="font-medium">Analysis failed</span>
              </div>
              <p class="text-red-400/80 text-xs">{screenshotError}</p>
            </div>
          {:else if screenshotAnalysis}
            <p class="text-sm text-[#CCCCCC] whitespace-pre-wrap leading-relaxed">{screenshotAnalysis}</p>
          {/if}
        </div>
      </div>
    {/if}
  </main>
</div>
