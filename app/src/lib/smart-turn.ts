/**
 * Smart Turn Detection Service
 *
 * Inspired by Pipecat's Smart Turn v3, this service detects when
 * a speaker has finished their turn using multiple signals:
 *
 * 1. Silence duration (time since last transcription)
 * 2. Sentence completion (grammar analysis)
 * 3. Turn-taking phrases ("what do you think?", "over to you")
 *
 * This allows for more natural conversation flow without
 * cutting off speakers mid-sentence.
 *
 * Note: VAD is now handled in Rust backend. Turn detection
 * here is based on transcription timing and content analysis.
 */

import type { TranscriptionResult } from './transcription';

export interface TurnResult {
  speakerDone: boolean;
  confidence: number;
  reason: TurnEndReason;
  source: 'microphone' | 'system';
  timestamp: number;
}

export type TurnEndReason =
  | 'silence' // Long silence detected
  | 'sentence_complete' // Complete sentence with ending punctuation
  | 'turn_phrase' // Explicit turn-taking phrase
  | 'question' // Question detected (may expect response)
  | 'continued'; // Speaker still talking

export type TurnCallback = (result: TurnResult) => void;

// Patterns that indicate turn completion
const TURN_PHRASES = [
  /what do you think\??$/i,
  /over to you\.?$/i,
  /your thoughts\??$/i,
  /go ahead\.?$/i,
  /please respond\.?$/i,
  /any questions\??$/i,
  /does that make sense\??$/i,
  /you know\?$/i,
  /right\?$/i,
];

// Sentence ending patterns
const SENTENCE_ENDINGS = /[.!?]$/;
const QUESTION_ENDINGS = /\?$/;

/**
 * SmartTurn Service
 * Detects speaker turn boundaries for natural conversation flow
 */
export class SmartTurnService {
  private callback: TurnCallback | null = null;

  // State per source
  private micState: TurnState;
  private systemState: TurnState;

  // Configuration
  private readonly config: SmartTurnConfig;

  constructor(config: Partial<SmartTurnConfig> = {}) {
    this.config = {
      // Silence thresholds (in ms)
      minSilenceForTurn: 700, // Min silence to consider turn end
      maxSilenceBeforeCutoff: 2000, // Force turn end after this silence

      // Confidence thresholds
      sentenceCompleteBonus: 0.3,
      turnPhraseBonus: 0.4,
      questionPenalty: 0.2, // Questions might expect response

      // Base confidence for silence-only detection
      baseSilenceConfidence: 0.5,

      ...config,
    };

    this.micState = this.createInitialState();
    this.systemState = this.createInitialState();
  }

  /**
   * Set callback for turn detection results
   */
  onTurn(callback: TurnCallback): void {
    this.callback = callback;
  }

  /**
   * Process transcription to analyze content and detect turn boundaries
   */
  processTranscription(result: TranscriptionResult): void {
    const state = result.source === 'microphone' ? this.micState : this.systemState;

    // Track speech timing from transcription
    state.lastSpeechTime = result.timestamp;
    state.isSpeaking = true;

    // Update current text
    state.currentText = result.text;
    state.isFinal = result.isFinal;

    if (result.isFinal) {
      // Analyze the completed utterance
      state.hasSentenceEnding = SENTENCE_ENDINGS.test(result.text.trim());
      state.hasQuestion = QUESTION_ENDINGS.test(result.text.trim());
      state.hasTurnPhrase = TURN_PHRASES.some(pattern => pattern.test(result.text.trim()));

      // Calculate silence since this is a final segment (speech ended)
      state.silenceDuration = this.config.minSilenceForTurn; // Final transcription means segment ended

      // Check for turn end with content analysis
      this.checkTurnEnd(result.source);

      // Reset for next utterance
      state.currentText = '';
    }
  }

  /**
   * Check if the current speaker has finished their turn
   */
  private checkTurnEnd(source: 'microphone' | 'system'): void {
    const state = source === 'microphone' ? this.micState : this.systemState;

    if (!state.isSpeaking) return;

    let confidence = 0;
    let reason: TurnEndReason = 'continued';

    // Check silence duration
    if (state.silenceDuration >= this.config.maxSilenceBeforeCutoff) {
      // Long silence - definitely done
      confidence = 1.0;
      reason = 'silence';
    } else if (state.silenceDuration >= this.config.minSilenceForTurn) {
      // Moderate silence - check other signals
      confidence = this.config.baseSilenceConfidence;

      // Boost confidence based on content
      if (state.hasTurnPhrase) {
        confidence += this.config.turnPhraseBonus;
        reason = 'turn_phrase';
      } else if (state.hasSentenceEnding) {
        confidence += this.config.sentenceCompleteBonus;
        reason = 'sentence_complete';
      }

      // Questions might expect response, reduce confidence
      if (state.hasQuestion) {
        confidence -= this.config.questionPenalty;
        reason = 'question';
      }
    }

    // Emit result if we have enough confidence
    const speakerDone = confidence >= 0.6;

    if (speakerDone && this.callback) {
      this.callback({
        speakerDone,
        confidence: Math.min(confidence, 1.0),
        reason,
        source,
        timestamp: Date.now(),
      });

      // Reset state
      this.resetState(source);
    }
  }

  /**
   * Reset state for a source
   */
  private resetState(source: 'microphone' | 'system'): void {
    if (source === 'microphone') {
      this.micState = this.createInitialState();
    } else {
      this.systemState = this.createInitialState();
    }
  }

  /**
   * Create initial turn state
   */
  private createInitialState(): TurnState {
    return {
      isSpeaking: false,
      lastSpeechTime: 0,
      silenceDuration: 0,
      currentText: '',
      isFinal: false,
      hasSentenceEnding: false,
      hasQuestion: false,
      hasTurnPhrase: false,
    };
  }

  /**
   * Reset all state
   */
  reset(): void {
    this.micState = this.createInitialState();
    this.systemState = this.createInitialState();
  }

  /**
   * Cleanup
   */
  destroy(): void {
    this.callback = null;
    this.reset();
  }
}

interface TurnState {
  isSpeaking: boolean;
  lastSpeechTime: number;
  silenceDuration: number;
  currentText: string;
  isFinal: boolean;
  hasSentenceEnding: boolean;
  hasQuestion: boolean;
  hasTurnPhrase: boolean;
}

interface SmartTurnConfig {
  minSilenceForTurn: number;
  maxSilenceBeforeCutoff: number;
  sentenceCompleteBonus: number;
  turnPhraseBonus: number;
  questionPenalty: number;
  baseSilenceConfidence: number;
}

// Export singleton instance
export const smartTurn = new SmartTurnService();
