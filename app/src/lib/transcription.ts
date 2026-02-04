/**
 * Transcription Service
 *
 * Receives transcriptions from the Rust backend via Tauri Channels (preferred)
 * or events (fallback). ASR processing is done natively using sherpa-rs in Rust.
 */

import { invoke, Channel } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface TranscriptionResult {
  text: string;
  isFinal: boolean;
  source: 'microphone' | 'system';
  timestamp: number;
  language?: string;
  emotion?: string;
  audioEvents?: string[];
  isTurnComplete?: boolean;
  turnConfidence?: number;
}

export type TranscriptionCallback = (result: TranscriptionResult) => void;

// Channel event types from Rust (matching TranscriptionEvent enum)
type TranscriptionChannelEvent =
  | {
      event: 'transcription';
      data: {
        text: string;
        source: string;
        timestampMs: number;
        isFinal: boolean;
        language: string;
        emotion: string;
        audioEvents: string[];
        isTurnComplete: boolean;
        turnConfidence: number;
      };
    }
  | {
      event: 'audioLevel';
      data: {
        source: string;
        rms: number;
        isSpeech: boolean;
      };
    }
  | {
      event: 'suggestion';
      data: {
        insight: string | null;
        question: string | null;
        relatedInfo: string | null;
      };
    }
  | {
      event: 'status';
      data: {
        recording: boolean;
        message: string;
      };
    };

// Event payload from Rust (legacy emit-based)
interface RustTranscriptionEvent {
  text: string;
  source: string;
  timestamp_ms: number;
  is_final: boolean;
  language?: string;
  emotion?: string;
  audio_events?: string[];
  is_turn_complete?: boolean;
  turn_confidence?: number;
}

/**
 * TranscriptionService receives real-time transcriptions from Rust backend.
 * Uses Tauri Channels for efficient streaming (preferred) with emit fallback.
 */
export class TranscriptionService {
  private callback: TranscriptionCallback | null = null;
  private unlistenFn: UnlistenFn | null = null;
  private isListening = false;
  private useChannel = true; // Prefer channel-based streaming
  private channel: Channel<TranscriptionChannelEvent> | null = null;

  /**
   * Initialize the transcription service
   * Sets up Channel-based streaming (preferred) with emit fallback
   */
  async initialize(): Promise<void> {
    if (this.isListening) return;

    try {
      if (this.useChannel) {
        // Create a new Channel for receiving transcription events
        this.channel = new Channel<TranscriptionChannelEvent>();

        // Set up the message handler
        this.channel.onmessage = (message) => {
          if (message.event === 'transcription' && this.callback) {
            const result: TranscriptionResult = {
              text: message.data.text,
              isFinal: message.data.isFinal,
              source: message.data.source as 'microphone' | 'system',
              timestamp: message.data.timestampMs,
              language: message.data.language,
              emotion: message.data.emotion,
              audioEvents: message.data.audioEvents,
              isTurnComplete: message.data.isTurnComplete,
              turnConfidence: message.data.turnConfidence,
            };
            this.callback(result);
          }
          // Handle other event types as needed
          else if (message.event === 'audioLevel') {
            // Could emit audio level updates for visualization
            console.debug('[Channel] Audio level:', message.data);
          }
          else if (message.event === 'suggestion') {
            // Real-time suggestions (handled separately in SecondBrain.svelte)
            console.debug('[Channel] Suggestion:', message.data);
          }
        };

        // Subscribe the channel to the backend
        await invoke('subscribe_transcription', { onEvent: this.channel });
        console.log('TranscriptionService initialized - using Channel streaming');
      }

      // Always set up emit listener (emit is always sent for reliability)
      this.unlistenFn = await listen<RustTranscriptionEvent>('transcription', (event) => {
        if (this.callback) {
          const result: TranscriptionResult = {
            text: event.payload.text,
            isFinal: event.payload.is_final,
            source: event.payload.source as 'microphone' | 'system',
            timestamp: event.payload.timestamp_ms,
            language: event.payload.language,
            emotion: event.payload.emotion,
            audioEvents: event.payload.audio_events,
            isTurnComplete: event.payload.is_turn_complete,
            turnConfidence: event.payload.turn_confidence,
          };
          this.callback(result);
        }
      });

      this.isListening = true;
      console.log('TranscriptionService initialized - listening for events');
    } catch (error) {
      console.error('Failed to initialize TranscriptionService:', error);
      // Fall back to emit-based if channel fails
      this.useChannel = false;
      throw error;
    }
  }

  /**
   * Set callback for transcription results
   */
  onTranscription(callback: TranscriptionCallback): void {
    this.callback = callback;
  }

  /**
   * Process audio samples from microphone
   * Note: Audio is actually processed in Rust. This is kept for API compatibility.
   */
  processMicrophoneAudio(_samples: Float32Array, _sampleRate: number): void {
    // Audio processing is handled by Rust backend via sherpa-rs
    // This method is kept for API compatibility but does nothing
  }

  /**
   * Process audio samples from system audio (guests)
   * Note: Audio is actually processed in Rust. This is kept for API compatibility.
   */
  processSystemAudio(_samples: Float32Array, _sampleRate: number): void {
    // Audio processing is handled by Rust backend via sherpa-rs
    // This method is kept for API compatibility but does nothing
  }

  /**
   * Stop transcription
   */
  stop(): void {
    // Transcription stopping is handled when recording stops in Rust
  }

  /**
   * Cleanup resources
   */
  async destroy(): Promise<void> {
    // Unsubscribe from channel
    if (this.useChannel) {
      try {
        await invoke('unsubscribe_transcription');
      } catch (e) {
        console.warn('Failed to unsubscribe from transcription channel:', e);
      }
    }

    if (this.unlistenFn) {
      this.unlistenFn();
      this.unlistenFn = null;
    }

    this.channel = null;
    this.isListening = false;
    this.callback = null;
    console.log('TranscriptionService destroyed');
  }
}

// Export singleton instance
export const transcriptionService = new TranscriptionService();
