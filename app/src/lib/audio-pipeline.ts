/**
 * Audio Processing Pipeline
 *
 * Coordinates transcription events from Rust backend (sherpa-rs) and
 * provides turn detection using Smart Turn.
 *
 * ASR processing happens natively in Rust via sherpa-rs.
 * Frontend receives transcription events via Tauri.
 */

import { transcriptionService, type TranscriptionResult } from './transcription';
import { smartTurn, type TurnResult } from './smart-turn';

export interface TranscriptSegment {
  id: string;
  speaker: 'you' | 'guest';
  guestId?: number;
  text: string;
  startTime: number;
  endTime?: number;
  isFinal: boolean;
}

export interface PipelineEvents {
  onTranscript: (segment: TranscriptSegment) => void;
  onTurnEnd: (result: TurnResult) => void;
  onError: (error: Error) => void;
}

/**
 * AudioPipeline coordinates audio processing
 */
export class AudioPipeline {
  private events: Partial<PipelineEvents> = {};
  private isInitialized = false;
  private isRunning = false;

  // Current transcript state
  private currentMicSegment: TranscriptSegment | null = null;
  private currentSystemSegment: TranscriptSegment | null = null;
  private segmentCounter = 0;
  private guestCounter = 0;

  // Full transcript history
  private transcripts: TranscriptSegment[] = [];

  /**
   * Initialize all services
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) return;

    try {
      // Initialize transcription service (listens for Rust events)
      await transcriptionService.initialize();

      // Set up callbacks
      this.setupCallbacks();

      this.isInitialized = true;
      console.log('AudioPipeline initialized');
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      this.events.onError?.(err);
      throw err;
    }
  }

  /**
   * Set up internal callbacks between services
   */
  private setupCallbacks(): void {
    // VAD results feed into turn detection
    // Note: VAD is currently processed per-audio-chunk, so we track source separately
    // The VAD callback receives results but we need to track which source it came from

    // Transcription results update UI and feed into turn detection
    transcriptionService.onTranscription((result) => {
      this.handleTranscription(result);
    });

    // Turn detection triggers segment finalization
    smartTurn.onTurn((result) => {
      this.handleTurnEnd(result);
    });
  }

  /**
   * Handle transcription results
   */
  private handleTranscription(result: TranscriptionResult): void {
    const isYou = result.source === 'microphone';
    const currentSegment = isYou ? this.currentMicSegment : this.currentSystemSegment;

    if (!currentSegment) {
      // Start new segment
      const newSegment: TranscriptSegment = {
        id: `seg-${++this.segmentCounter}`,
        speaker: isYou ? 'you' : 'guest',
        guestId: isYou ? undefined : this.getCurrentGuestId(),
        text: result.text,
        startTime: result.timestamp,
        isFinal: result.isFinal,
      };

      if (isYou) {
        this.currentMicSegment = newSegment;
      } else {
        this.currentSystemSegment = newSegment;
      }

      this.events.onTranscript?.(newSegment);
    } else {
      // Update existing segment
      currentSegment.text = result.text;
      currentSegment.isFinal = result.isFinal;

      if (result.isFinal) {
        currentSegment.endTime = result.timestamp;
        this.transcripts.push({ ...currentSegment });

        // Reset current segment
        if (isYou) {
          this.currentMicSegment = null;
        } else {
          this.currentSystemSegment = null;
        }
      }

      this.events.onTranscript?.(currentSegment);
    }

    // Feed to turn detection
    smartTurn.processTranscription(result);
  }

  /**
   * Handle turn end detection
   */
  private handleTurnEnd(result: TurnResult): void {
    this.events.onTurnEnd?.(result);

    // Could trigger context lookup, LLM processing, etc. here
    console.log(`Turn ended: ${result.source} (${result.reason}, confidence: ${result.confidence})`);
  }

  /**
   * Get current guest ID (for speaker diarization)
   * TODO: Implement actual diarization
   */
  private getCurrentGuestId(): number {
    // For now, just increment. Real implementation would use
    // speaker embeddings to identify unique speakers.
    return 1;
  }

  /**
   * Start the pipeline
   */
  start(): void {
    if (!this.isInitialized) {
      throw new Error('Pipeline not initialized. Call initialize() first.');
    }
    this.isRunning = true;
    console.log('AudioPipeline started');
  }

  /**
   * Stop the pipeline
   */
  stop(): void {
    this.isRunning = false;
    transcriptionService.stop();
    smartTurn.reset();
    console.log('AudioPipeline stopped');
  }

  /**
   * Set event handlers
   */
  on<K extends keyof PipelineEvents>(event: K, handler: PipelineEvents[K]): void {
    this.events[event] = handler;
  }

  /**
   * Get full transcript history
   */
  getTranscripts(): TranscriptSegment[] {
    return [...this.transcripts];
  }

  /**
   * Clear transcript history
   */
  clearTranscripts(): void {
    this.transcripts = [];
    this.currentMicSegment = null;
    this.currentSystemSegment = null;
    this.segmentCounter = 0;
  }

  /**
   * Cleanup all resources
   */
  destroy(): void {
    this.stop();
    transcriptionService.destroy();
    smartTurn.destroy();
    this.events = {};
    this.transcripts = [];
    this.isInitialized = false;
  }
}

// Export singleton instance
export const audioPipeline = new AudioPipeline();
