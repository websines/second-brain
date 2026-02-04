<script lang="ts">
  import type { TranscriptSegment } from "$lib/audio-pipeline";
  import { MessageSquare, Mic } from "lucide-svelte";

  interface Props {
    segments: TranscriptSegment[];
    currentMicText: string;
    currentSystemText: string;
  }

  let { segments, currentMicText, currentSystemText }: Props = $props();

  // Auto-scroll to bottom
  let container: HTMLDivElement;

  $effect(() => {
    if (container && (segments.length > 0 || currentMicText || currentSystemText)) {
      container.scrollTop = container.scrollHeight;
    }
  });

  function formatTime(timestamp: number): string {
    const date = new Date(timestamp);
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  function getSpeakerLabel(segment: TranscriptSegment): string {
    if (segment.speaker === 'you') return 'You';
    return segment.guestId ? `Guest ${segment.guestId}` : 'Guest';
  }

  function getSpeakerColorClass(segment: TranscriptSegment): string {
    if (segment.speaker === 'you') return 'text-indigo-400 border-indigo-500';
    // Different colors for different guests
    const colors = [
      'text-green-400 border-green-500', 
      'text-amber-400 border-amber-500', 
      'text-red-400 border-red-500', 
      'text-purple-400 border-purple-500'
    ];
    const idx = (segment.guestId || 1) - 1;
    return colors[idx % colors.length];
  }
</script>

<div class="flex-1 overflow-y-auto p-4 flex flex-col gap-3 scroll-smooth" bind:this={container}>
  {#if segments.length === 0 && !currentMicText && !currentSystemText}
    <div class="flex flex-col items-center justify-center h-full text-[#666666] gap-4">
      <div class="p-4 rounded-full bg-[#252525] border border-[#333333]">
        <Mic size={32} class="opacity-50" />
      </div>
      <p class="text-sm font-medium">Start recording to see live transcription</p>
    </div>
  {:else}
    <!-- Completed segments -->
    {#each segments as segment (segment.id)}
      <div class="bg-[#252525] rounded-xl p-3 pl-4 border-l-[3px] {getSpeakerColorClass(segment).split(' ')[1]} {segment.speaker === 'you' ? 'bg-indigo-500/5' : ''} border border-transparent hover:border-[#333333] transition-colors">
        <div class="flex justify-between items-center mb-1.5">
          <span class="text-xs font-semibold uppercase tracking-wider {getSpeakerColorClass(segment).split(' ')[0]}">
            {getSpeakerLabel(segment)}
          </span>
          <span class="text-[10px] text-[#777777]">{formatTime(segment.startTime)}</span>
        </div>
        <p class="text-sm leading-relaxed text-[#EBEBEB] m-0">{segment.text}</p>
      </div>
    {/each}

    <!-- Current live segments (not yet finalized) -->
    {#if currentMicText}
      <div class="bg-[#252525] rounded-xl p-3 pl-4 border-l-[3px] border-indigo-500 bg-indigo-500/5 animate-pulse">
        <div class="flex justify-between items-center mb-1.5">
          <span class="text-xs font-semibold uppercase tracking-wider text-indigo-400">You</span>
          <span class="flex items-center gap-1.5 text-[10px] text-red-500 font-medium">
            <span class="w-1.5 h-1.5 bg-red-500 rounded-full animate-ping"></span>
            Live
          </span>
        </div>
        <p class="text-sm leading-relaxed text-[#EBEBEB] m-0">{currentMicText}</p>
      </div>
    {/if}

    {#if currentSystemText}
      <div class="bg-[#252525] rounded-xl p-3 pl-4 border-l-[3px] border-green-500 animate-pulse">
        <div class="flex justify-between items-center mb-1.5">
          <span class="text-xs font-semibold uppercase tracking-wider text-green-400">Guest</span>
          <span class="flex items-center gap-1.5 text-[10px] text-red-500 font-medium">
            <span class="w-1.5 h-1.5 bg-red-500 rounded-full animate-ping"></span>
            Live
          </span>
        </div>
        <p class="text-sm leading-relaxed text-[#EBEBEB] m-0">{currentSystemText}</p>
      </div>
    {/if}
  {/if}
</div>
