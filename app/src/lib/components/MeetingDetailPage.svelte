<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";
  import { ArrowLeft, Trash2, Sparkles, CheckCircle2, MessageSquare, Users, Hash, Calendar, Clock, Type, Loader2 } from "lucide-svelte";

  // TypeScript interfaces
  interface Meeting {
    id: { tb: string; id: { String: string } } | null;
    title: string;
    start_time: number;
    end_time: number | null;
    participants: string[];
    summary: string | null;
  }

  interface TranscriptSegment {
    id: { tb: string; id: { String: string } } | null;
    meeting_id: string;
    speaker: string;
    text: string;
    start_ms: number;
    end_ms: number;
    embedding: number[];
  }

  interface ActionItem {
    id: { tb: string; id: { String: string } } | null;
    meeting_id: string;
    text: string;
    assignee: string | null;
    deadline: string | null;
    status: string;
    created_at: number;
  }

  interface Decision {
    id: { tb: string; id: { String: string } } | null;
    meeting_id: string;
    text: string;
    participants: string[];
    created_at: number;
  }

  interface Topic {
    id: { tb: string; id: { String: string } } | null;
    name: string;
    embedding: number[];
    mention_count: number;
    last_mentioned: number;
  }

  interface Person {
    id: { tb: string; id: { String: string } } | null;
    name: string;
    aliases: string[];
    first_seen: number;
    last_seen: number;
  }

  interface MeetingStats {
    segment_count: number;
    action_count: number;
    decision_count: number;
    topic_count: number;
    people_count: number;
    duration_ms: number;
    total_words: number;
  }

  // Props
  let {
    meetingId,
    onBack
  }: {
    meetingId: string;
    onBack: () => void;
  } = $props();

  // State
  let meeting = $state<Meeting | null>(null);
  let segments = $state<TranscriptSegment[]>([]);
  let actionItems = $state<ActionItem[]>([]);
  let decisions = $state<Decision[]>([]);
  let topics = $state<Topic[]>([]);
  let people = $state<Person[]>([]);
  let stats = $state<MeetingStats | null>(null);
  let isLoading = $state(true);
  let activeTab = $state<'transcript' | 'highlights' | 'entities'>('highlights');

  // Ask AI state
  let aiQuestion = $state("");
  let aiResponse = $state("");
  let isAskingAi = $state(false);
  let showAskAi = $state(false);

  // Delete meeting state
  let showDeleteConfirm = $state(false);
  let isDeleting = $state(false);

  // Track if highlights are still being processed
  let processingHighlights = $state(false);

  // Handler for highlights ready event
  function handleHighlightsReady(event: CustomEvent<{ meetingId: string }>) {
    if (event.detail.meetingId === meetingId) {
      console.log("[MeetingDetail] Highlights ready, reloading data...");
      processingHighlights = false;
      loadMeetingData();
    }
  }

  // Load meeting data
  onMount(async () => {
    // Listen for highlights ready event
    window.addEventListener('meeting-highlights-ready', handleHighlightsReady as EventListener);

    await loadMeetingData();

    // If this is a just-ended meeting (no action items/decisions yet), show processing indicator
    if (actionItems.length === 0 && decisions.length === 0 && segments.length > 0) {
      processingHighlights = true;
    }
  });

  onDestroy(() => {
    window.removeEventListener('meeting-highlights-ready', handleHighlightsReady as EventListener);
  });

  async function loadMeetingData() {
    isLoading = true;
    try {
      // Load all data in parallel
      const [meetingData, segmentsData, actionsData, decisionsData, topicsData, peopleData, statsData] = await Promise.all([
        invoke<Meeting | null>("get_meeting", { meetingId }),
        invoke<TranscriptSegment[]>("get_meeting_segments", { meetingId }),
        invoke<ActionItem[]>("get_meeting_action_items", { meetingId }),
        invoke<Decision[]>("get_meeting_decisions", { meetingId }),
        invoke<Topic[]>("get_meeting_topics", { meetingId }),
        invoke<Person[]>("get_meeting_people", { meetingId }),
        invoke<MeetingStats>("get_meeting_stats", { meetingId }),
      ]);

      meeting = meetingData;
      segments = segmentsData;
      actionItems = actionsData;
      decisions = decisionsData;
      topics = topicsData;
      people = peopleData;
      stats = statsData;
    } catch (e) {
      console.error("Failed to load meeting data:", e);
    } finally {
      isLoading = false;
    }
  }

  // Format time
  function formatTime(ms: number): string {
    const date = new Date(ms);
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  function formatDate(ms: number): string {
    const date = new Date(ms);
    return date.toLocaleDateString([], { weekday: 'long', month: 'short', day: 'numeric', year: 'numeric' });
  }

  function formatDuration(ms: number): string {
    const minutes = Math.floor(ms / 60000);
    const hours = Math.floor(minutes / 60);
    const mins = minutes % 60;
    if (hours > 0) {
      return `${hours}h ${mins}m`;
    }
    return `${mins}m`;
  }

  function formatSegmentTime(startMs: number, meetingStartMs: number): string {
    const offsetMs = startMs - meetingStartMs;
    const minutes = Math.floor(offsetMs / 60000);
    const seconds = Math.floor((offsetMs % 60000) / 1000);
    return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  }

  // Get status color
  function getStatusColor(status: string): string {
    switch (status) {
      case 'open': return 'bg-amber-500/10 text-amber-500 border-amber-500/20';
      case 'in_progress': return 'bg-blue-500/10 text-blue-500 border-blue-500/20';
      case 'done': return 'bg-green-500/10 text-green-500 border-green-500/20';
      default: return 'bg-zinc-500/10 text-zinc-500 border-zinc-500/20';
    }
  }

  // Update action item status
  async function updateActionStatus(actionId: string, newStatus: string) {
    try {
      await invoke("update_action_item_status", { actionId, status: newStatus });
      actionItems = actionItems.map(a => {
        const id = a.id ? `${a.id.tb}:${a.id.id.String}` : '';
        if (id === actionId) {
          return { ...a, status: newStatus };
        }
        return a;
      });
    } catch (e) {
      console.error("Failed to update action status:", e);
    }
  }

  // Get action ID string
  function getActionId(action: ActionItem): string {
    return action.id ? `${action.id.tb}:${action.id.id.String}` : '';
  }

  // Ask AI about this meeting
  async function askAboutMeeting() {
    if (!aiQuestion.trim() || !meeting) return;

    isAskingAi = true;
    aiResponse = "";

    try {
      // Format transcript segments as "Speaker: text"
      const transcriptLines = segments.map(s => `${s.speaker}: ${s.text}`);
      const actionTexts = actionItems.map(a => a.text);
      const decisionTexts = decisions.map(d => d.text);

      const response = await invoke<string>("ask_meeting_question", {
        question: aiQuestion,
        meetingTitle: meeting.title,
        transcript: transcriptLines,
        actionItems: actionTexts,
        decisions: decisionTexts,
      });

      aiResponse = response;
    } catch (e) {
      console.error("Failed to ask AI:", e);
      aiResponse = `Error: ${e}`;
    } finally {
      isAskingAi = false;
    }
  }

  // Handle Enter key in question input
  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      askAboutMeeting();
    }
  }

  // Delete meeting
  async function deleteMeeting() {
    if (!meetingId) return;

    isDeleting = true;
    try {
      await invoke("delete_meeting", { meetingId });
      // Navigate back after successful deletion
      onBack();
    } catch (e) {
      console.error("Failed to delete meeting:", e);
      isDeleting = false;
    }
  }
</script>

<div class="h-full flex flex-col bg-[#1F1F1F]">
  <!-- Header -->
  <header class="flex items-center gap-4 p-6 border-b border-[#2C2C2C]">
    <button
      onclick={onBack}
      class="p-2 rounded hover:bg-[#2C2C2C] text-[#9B9B9B] hover:text-[#EBEBEB] transition-colors"
    >
      <ArrowLeft size={20} />
    </button>

    {#if meeting}
      <div class="flex-1">
        <h1 class="text-xl font-semibold text-[#EBEBEB]">{meeting.title}</h1>
        <div class="flex items-center gap-4 mt-1 text-sm text-[#9B9B9B]">
          <span>{formatDate(meeting.start_time)}</span>
          <span>•</span>
          <span>{formatTime(meeting.start_time)}</span>
          {#if stats}
            <span>•</span>
            <span>{formatDuration(stats.duration_ms)}</span>
          {/if}
        </div>
      </div>
    {/if}

    {#if stats}
      <div class="flex gap-6 text-sm">
        <div class="text-center">
          <div class="text-lg font-semibold text-[#EBEBEB]">{stats.segment_count}</div>
          <div class="text-xs text-[#777777]">Segments</div>
        </div>
        <div class="text-center">
          <div class="text-lg font-semibold text-[#EBEBEB]">{stats.total_words}</div>
          <div class="text-xs text-[#777777]">Words</div>
        </div>
        <div class="text-center">
          <div class="text-lg font-semibold text-amber-500">{stats.action_count}</div>
          <div class="text-xs text-[#777777]">Actions</div>
        </div>
        <div class="text-center">
          <div class="text-lg font-semibold text-indigo-500">{stats.decision_count}</div>
          <div class="text-xs text-[#777777]">Decisions</div>
        </div>
      </div>
    {/if}

    <!-- Ask AI Button -->
    <button
      onclick={() => showAskAi = !showAskAi}
      class="flex items-center gap-2 px-4 py-2 rounded transition-colors {showAskAi ? 'bg-[#2C2C2C] text-white' : 'bg-[#252525] border border-transparent hover:border-[#333333] text-[#CCCCCC] hover:bg-[#2C2C2C]'}"
    >
      <Sparkles size={16} />
      <span class="text-sm font-medium">Ask AI</span>
    </button>

    <!-- Delete Button -->
    <button
      onclick={() => showDeleteConfirm = true}
      class="flex items-center gap-2 px-3 py-2 rounded bg-red-500/10 text-red-500 hover:bg-red-500/20 transition-colors"
      title="Delete meeting"
    >
      <Trash2 size={16} />
    </button>
  </header>

  <!-- Delete Confirmation Modal -->
  {#if showDeleteConfirm}
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div class="bg-[#252525] border border-[#333333] rounded-lg p-6 max-w-md mx-4 shadow-2xl">
        <div class="flex items-center gap-3 mb-4">
          <div class="p-2 rounded-full bg-red-500/10">
            <Trash2 class="text-red-500" size={24} />
          </div>
          <h3 class="text-lg font-semibold text-[#EBEBEB]">Delete Meeting</h3>
        </div>

        <p class="text-[#9B9B9B] mb-6 text-sm">
          Are you sure you want to delete "{meeting?.title}"? This will permanently remove the meeting, all transcript segments, action items, and decisions. This action cannot be undone.
        </p>

        <div class="flex gap-3 justify-end">
          <button
            onclick={() => showDeleteConfirm = false}
            class="px-4 py-2 rounded bg-[#333333] text-[#CCCCCC] hover:bg-[#444444] transition-colors text-sm font-medium"
            disabled={isDeleting}
          >
            Cancel
          </button>
          <button
            onclick={deleteMeeting}
            class="px-4 py-2 rounded bg-red-600 text-white hover:bg-red-700 transition-colors flex items-center gap-2 text-sm font-medium"
            disabled={isDeleting}
          >
            {#if isDeleting}
              <div class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></div>
              <span>Deleting...</span>
            {:else}
              <span>Delete Meeting</span>
            {/if}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Ask AI Panel -->
  {#if showAskAi}
    <div class="border-b border-[#2C2C2C] bg-[#1F1F1F] p-4">
      <div class="max-w-4xl mx-auto">
        <div class="flex gap-3">
          <input
            type="text"
            bind:value={aiQuestion}
            onkeydown={handleKeydown}
            placeholder="Ask about this meeting... (e.g., 'What were the main decisions?' or 'What did John say about the deadline?')"
            class="flex-1 px-4 py-2.5 rounded bg-[#252525] border border-transparent text-[#EBEBEB] placeholder:text-[#666666] focus:outline-none focus:ring-1 focus:ring-[#444444]"
            disabled={isAskingAi}
          />
          <button
            onclick={askAboutMeeting}
            disabled={isAskingAi || !aiQuestion.trim()}
            class="px-5 py-2.5 rounded bg-[#333333] text-[#EBEBEB] font-medium hover:bg-[#444444] disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
          >
            {#if isAskingAi}
              <div class="w-4 h-4 border-2 border-[#9B9B9B] border-t-white rounded-full animate-spin"></div>
              <span>Thinking...</span>
            {:else}
              <Sparkles size={16} />
              <span>Ask</span>
            {/if}
          </button>
        </div>

        {#if aiResponse}
          <div class="mt-4 p-4 rounded bg-[#252525] border border-transparent">
            <div class="flex items-center gap-2 mb-2 text-xs text-[#777777] font-medium uppercase tracking-wider">
              AI Response
            </div>
            <div class="text-[#CCCCCC] text-sm leading-relaxed whitespace-pre-wrap">
              {aiResponse}
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Tabs -->
  <div class="flex gap-1 p-2 border-b border-[#2C2C2C] bg-[#1F1F1F]">
    <button
      class="px-4 py-2 text-sm font-medium rounded transition-colors {activeTab === 'highlights' ? 'bg-[#2C2C2C] text-[#EBEBEB]' : 'text-[#9B9B9B] hover:text-[#EBEBEB] hover:bg-[#2C2C2C]/50'}"
      onclick={() => activeTab = 'highlights'}
    >
      Highlights
    </button>
    <button
      class="px-4 py-2 text-sm font-medium rounded transition-colors {activeTab === 'transcript' ? 'bg-[#2C2C2C] text-[#EBEBEB]' : 'text-[#9B9B9B] hover:text-[#EBEBEB] hover:bg-[#2C2C2C]/50'}"
      onclick={() => activeTab = 'transcript'}
    >
      Transcript
    </button>
    <button
      class="px-4 py-2 text-sm font-medium rounded transition-colors {activeTab === 'entities' ? 'bg-[#2C2C2C] text-[#EBEBEB]' : 'text-[#9B9B9B] hover:text-[#EBEBEB] hover:bg-[#2C2C2C]/50'}"
      onclick={() => activeTab = 'entities'}
    >
      Entities
    </button>
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-y-auto p-6 bg-[#1F1F1F]">
    {#if isLoading}
      <div class="flex items-center justify-center h-64">
        <div class="text-[#666666]">Loading meeting data...</div>
      </div>
    {:else if activeTab === 'highlights'}
      <!-- Highlights Tab -->
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 max-w-6xl mx-auto">
        <!-- Processing indicator -->
        {#if processingHighlights}
          <div class="lg:col-span-2 bg-indigo-500/10 border border-indigo-500/20 rounded-lg p-4 flex items-center gap-3">
            <Loader2 size={18} class="text-indigo-400 animate-spin" />
            <div>
              <p class="text-sm text-indigo-400 font-medium">Analyzing meeting content...</p>
              <p class="text-xs text-indigo-400/70">Extracting action items, decisions, and key insights</p>
            </div>
          </div>
        {/if}

        <!-- Summary -->
        {#if meeting?.summary}
          <section class="lg:col-span-2 bg-[#252525] border border-transparent rounded-lg p-5">
            <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-3">Summary</h2>
            <p class="text-[#EBEBEB] leading-relaxed">{meeting.summary}</p>
          </section>
        {/if}

        <!-- Action Items -->
        <section class="bg-[#252525] border border-transparent rounded-lg p-5">
          <div class="flex items-center justify-between mb-4">
            <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider">Action Items</h2>
            <span class="text-xs font-medium px-2 py-1 rounded-full bg-amber-500/10 text-amber-500">
              {actionItems.length}
            </span>
          </div>

          {#if actionItems.length > 0}
            <div class="space-y-3">
              {#each actionItems as action}
                <div class="flex items-start gap-3 p-3 rounded bg-[#1F1F1F] border border-transparent hover:border-[#333333] transition-colors group">
                  <button
                    class="mt-0.5 w-5 h-5 rounded border flex items-center justify-center transition-colors {action.status === 'done' ? 'bg-green-600 border-green-600' : 'border-[#444444] hover:border-[#666666]'}"
                    onclick={() => updateActionStatus(getActionId(action), action.status === 'done' ? 'open' : 'done')}
                  >
                    {#if action.status === 'done'}
                      <CheckCircle2 size={12} color="white" />
                    {/if}
                  </button>
                  <div class="flex-1 min-w-0">
                    <p class="text-sm text-[#EBEBEB] {action.status === 'done' ? 'line-through opacity-60' : ''}">{action.text}</p>
                    <div class="flex items-center gap-3 mt-1 text-xs text-[#777777]">
                      {#if action.assignee}
                        <span>Assigned to {action.assignee}</span>
                      {/if}
                      {#if action.deadline}
                        <span>Due {action.deadline}</span>
                      {/if}
                    </div>
                  </div>
                  <span class="text-[10px] px-1.5 py-0.5 rounded border {getStatusColor(action.status)}">
                    {action.status.replace('_', ' ')}
                  </span>
                </div>
              {/each}
            </div>
          {:else}
            <div class="text-center py-8 text-[#666666]">
              <p class="mt-2 text-sm">No action items found</p>
            </div>
          {/if}
        </section>

        <!-- Decisions -->
        <section class="bg-[#252525] border border-transparent rounded-lg p-5">
          <div class="flex items-center justify-between mb-4">
            <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider">Decisions</h2>
            <span class="text-xs font-medium px-2 py-1 rounded-full bg-indigo-500/10 text-indigo-500">
              {decisions.length}
            </span>
          </div>

          {#if decisions.length > 0}
            <div class="space-y-3">
              {#each decisions as decision}
                <div class="p-3 rounded bg-[#1F1F1F] border border-transparent hover:border-[#333333] transition-colors">
                  <p class="text-sm text-[#EBEBEB]">{decision.text}</p>
                  {#if decision.participants.length > 0}
                    <div class="flex items-center gap-1 mt-2">
                      {#each decision.participants as participant}
                        <span class="text-xs px-1.5 py-0.5 rounded bg-[#2C2C2C] text-[#9B9B9B]">{participant}</span>
                      {/each}
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {:else}
            <div class="text-center py-8 text-[#666666]">
              <p class="mt-2 text-sm">No decisions recorded</p>
            </div>
          {/if}
        </section>

        <!-- Topics -->
        <section class="bg-[#252525] border border-transparent rounded-lg p-5">
          <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4">Topics Discussed</h2>
          {#if topics.length > 0}
            <div class="flex flex-wrap gap-2">
              {#each topics as topic}
                <span class="px-2 py-1 rounded bg-[#1F1F1F] text-[#CCCCCC] text-sm border border-transparent hover:border-[#333333] transition-colors">
                  {topic.name}
                  <span class="ml-1 text-xs text-[#666666]">({topic.mention_count})</span>
                </span>
              {/each}
            </div>
          {:else}
            <div class="text-center py-4 text-[#666666] text-sm">No topics extracted</div>
          {/if}
        </section>

        <!-- People -->
        <section class="bg-[#252525] border border-transparent rounded-lg p-5">
          <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4">People Mentioned</h2>
          {#if people.length > 0}
            <div class="flex flex-wrap gap-2">
              {#each people as person}
                <span class="px-2 py-1 rounded bg-[#1F1F1F] text-[#CCCCCC] text-sm border border-transparent hover:border-[#333333] transition-colors">
                  {person.name}
                </span>
              {/each}
            </div>
          {:else}
            <div class="text-center py-4 text-[#666666] text-sm">No people mentioned</div>
          {/if}
        </section>
      </div>

    {:else if activeTab === 'transcript'}
      <!-- Transcript Tab -->
      <div class="max-w-4xl mx-auto space-y-4">
        {#if segments.length > 0}
          {#each segments as segment}
            <div class="flex gap-4 group">
              <div class="w-16 flex-shrink-0 text-right">
                <span class="text-xs text-[#666666] font-mono">
                  {formatSegmentTime(segment.start_ms, meeting?.start_time || 0)}
                </span>
              </div>
              <div class="flex-1 pb-4 border-b border-[#2C2C2C] last:border-b-0">
                <span class="text-xs font-medium {segment.speaker === 'You' ? 'text-[#9B9B9B]' : 'text-[#777777]'} uppercase tracking-wide">
                  {segment.speaker}
                </span>
                <p class="text-[#EBEBEB] mt-1 leading-relaxed">{segment.text}</p>
              </div>
            </div>
          {/each}
        {:else}
          <div class="text-center py-12 text-[#666666]">
            <p class="mt-2">No transcript segments yet</p>
          </div>
        {/if}
      </div>

    {:else if activeTab === 'entities'}
      <!-- Entities Tab -->
      <div class="grid grid-cols-1 md:grid-cols-2 gap-6 max-w-4xl mx-auto">
        <!-- People -->
        <section class="bg-[#252525] border border-transparent rounded-lg p-5">
          <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4 flex items-center gap-2">
            <Users size={14} /> People ({people.length})
          </h2>
          {#if people.length > 0}
            <div class="space-y-2">
              {#each people as person}
                <div class="flex items-center justify-between p-2 rounded bg-[#1F1F1F] border border-transparent hover:border-[#333333] transition-colors">
                  <span class="text-[#CCCCCC]">{person.name}</span>
                  {#if person.aliases.length > 0}
                    <span class="text-xs text-[#666666]">aka {person.aliases.join(', ')}</span>
                  {/if}
                </div>
              {/each}
            </div>
          {:else}
            <p class="text-[#666666] text-sm">No people extracted</p>
          {/if}
        </section>

        <!-- Topics -->
        <section class="bg-[#252525] border border-transparent rounded-lg p-5">
          <h2 class="text-xs font-semibold text-[#9B9B9B] uppercase tracking-wider mb-4 flex items-center gap-2">
            <Hash size={14} /> Topics ({topics.length})
          </h2>
          {#if topics.length > 0}
            <div class="space-y-2">
              {#each topics as topic}
                <div class="flex items-center justify-between p-2 rounded bg-[#1F1F1F] border border-transparent hover:border-[#333333] transition-colors">
                  <span class="text-[#CCCCCC]">{topic.name}</span>
                  <span class="text-xs text-[#666666]">{topic.mention_count} mentions</span>
                </div>
              {/each}
            </div>
          {:else}
            <p class="text-[#666666] text-sm">No topics extracted</p>
          {/if}
        </section>
      </div>
    {/if}
  </div>
</div>
