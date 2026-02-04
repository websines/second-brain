//! Agent Workers - Process jobs from the queue
//!
//! Each worker type handles specific AI tasks:
//! - RealtimeSuggestionsWorker: Generates meeting suggestions
//! - QuestionAnswerWorker: Answers user questions
//! - HighlightsWorker: Extracts meeting highlights
//! - EntityWorker: Extracts entities from text

use std::sync::Arc;
use tokio::sync::RwLock;
use parking_lot::RwLock as SyncRwLock;

use crate::agent_queue::*;
use crate::knowledge_base::KnowledgeBase;
use crate::llm_agent::MeetingAssistant;
use crate::entities::EntityEngine;

/// Process a job from the queue
/// This is the main dispatch function called by workers
pub async fn process_agent_job(
    job: AgentJob,
    queue_stats: Arc<RwLock<QueueStats>>,
    llm: Option<Arc<MeetingAssistant>>,
    kb: Option<Arc<RwLock<Option<KnowledgeBase>>>>,
    entity_engine: Option<Arc<SyncRwLock<Option<EntityEngine>>>>,
) {
    match job {
        AgentJob::RealtimeSuggestions { meeting_id, recent_transcripts, context, response_tx } => {
            let result = process_realtime_suggestions(
                &meeting_id,
                &recent_transcripts,
                context.as_deref(),
                llm.as_ref(),
                kb.as_ref(),
            ).await;

            let _ = response_tx.send(result).await;

            let mut stats = queue_stats.write().await;
            if stats.pending_jobs > 0 { stats.pending_jobs -= 1; }
            stats.processed_jobs += 1;
        }

        AgentJob::AnswerQuestion { question, context, response_tx } => {
            let result = process_answer_question(
                &question,
                context.as_deref(),
                llm.as_ref(),
                kb.as_ref(),
            ).await;

            let _ = response_tx.send(result).await;

            let mut stats = queue_stats.write().await;
            if stats.pending_jobs > 0 { stats.pending_jobs -= 1; }
            stats.processed_jobs += 1;
        }

        AgentJob::PostMeetingHighlights { meeting_id, response_tx } => {
            let result = process_meeting_highlights(
                &meeting_id,
                llm.as_ref(),
                kb.as_ref(),
            ).await;

            let _ = response_tx.send(result).await;

            let mut stats = queue_stats.write().await;
            if stats.pending_jobs > 0 { stats.pending_jobs -= 1; }
            stats.processed_jobs += 1;
        }

        AgentJob::EntityExtraction { text, source, timestamp_ms, response_tx } => {
            let result = process_entity_extraction(
                &text,
                &source,
                timestamp_ms,
                entity_engine.as_ref(),
            ).await;

            let _ = response_tx.send(result).await;

            let mut stats = queue_stats.write().await;
            if stats.pending_jobs > 0 { stats.pending_jobs -= 1; }
            stats.processed_jobs += 1;
        }

        AgentJob::Shutdown => {
            // Handled by worker pool
        }
    }
}

/// Generate real-time suggestions during a meeting
async fn process_realtime_suggestions(
    _meeting_id: &str,
    recent_transcripts: &[String],
    context: Option<&str>,
    llm: Option<&Arc<MeetingAssistant>>,
    kb: Option<&Arc<RwLock<Option<KnowledgeBase>>>>,
) -> RealtimeSuggestionResult {
    let Some(assistant) = llm else {
        return RealtimeSuggestionResult {
            error: Some("LLM not initialized".to_string()),
            ..Default::default()
        };
    };

    if recent_transcripts.is_empty() {
        return RealtimeSuggestionResult::default();
    }

    // Get KB for context lookup
    let kb_arc = match kb {
        Some(k) => k.clone(),
        None => {
            return RealtimeSuggestionResult {
                error: Some("Knowledge base not available".to_string()),
                ..Default::default()
            };
        }
    };

    match assistant.generate_realtime_suggestions(recent_transcripts, context, kb_arc).await {
        Ok(suggestion) => RealtimeSuggestionResult {
            insight: suggestion.insight,
            question: suggestion.question,
            related_info: suggestion.related_info,
            error: None,
        },
        Err(e) => RealtimeSuggestionResult {
            error: Some(e),
            ..Default::default()
        },
    }
}

/// Answer a user question using KB and LLM
async fn process_answer_question(
    question: &str,
    context: Option<&str>,
    llm: Option<&Arc<MeetingAssistant>>,
    kb: Option<&Arc<RwLock<Option<KnowledgeBase>>>>,
) -> AnswerResult {
    let Some(assistant) = llm else {
        return AnswerResult {
            error: Some("LLM not initialized".to_string()),
            ..Default::default()
        };
    };

    let kb_arc = match kb {
        Some(k) => k.clone(),
        None => {
            return AnswerResult {
                error: Some("Knowledge base not available".to_string()),
                ..Default::default()
            };
        }
    };

    // Build context from question + optional context
    let full_context = match context {
        Some(ctx) => format!("Context: {}\n\nQuestion: {}", ctx, question),
        None => question.to_string(),
    };

    match assistant.ask(&full_context, kb_arc).await {
        Ok(answer) => AnswerResult {
            answer,
            sources: vec![], // TODO: Track sources from KB lookups
            error: None,
        },
        Err(e) => AnswerResult {
            error: Some(e),
            ..Default::default()
        },
    }
}

/// Extract highlights from a completed meeting
async fn process_meeting_highlights(
    meeting_id: &str,
    llm: Option<&Arc<MeetingAssistant>>,
    kb: Option<&Arc<RwLock<Option<KnowledgeBase>>>>,
) -> HighlightsResult {
    let Some(assistant) = llm else {
        return HighlightsResult {
            error: Some("LLM not initialized".to_string()),
            ..Default::default()
        };
    };

    let kb_arc = match kb {
        Some(k) => k.clone(),
        None => {
            return HighlightsResult {
                error: Some("Knowledge base not available".to_string()),
                ..Default::default()
            };
        }
    };

    // Get meeting segments from KB
    let kb_guard = kb_arc.read().await;
    let kb = match kb_guard.as_ref() {
        Some(k) => k,
        None => {
            return HighlightsResult {
                error: Some("Knowledge base not initialized".to_string()),
                ..Default::default()
            };
        }
    };

    // Get meeting segments
    let segments = match kb.get_meeting_segments(meeting_id).await {
        Ok(s) => s,
        Err(e) => {
            return HighlightsResult {
                error: Some(format!("Failed to get meeting segments: {}", e)),
                ..Default::default()
            };
        }
    };

    if segments.is_empty() {
        return HighlightsResult {
            error: Some("No segments found for meeting".to_string()),
            ..Default::default()
        };
    }

    // Get meeting title
    let meeting = match kb.get_meeting(meeting_id).await {
        Ok(Some(m)) => m,
        Ok(None) => {
            return HighlightsResult {
                error: Some("Meeting not found".to_string()),
                ..Default::default()
            };
        }
        Err(e) => {
            return HighlightsResult {
                error: Some(format!("Failed to get meeting: {}", e)),
                ..Default::default()
            };
        }
    };

    // Format transcript as strings (TranscriptSegment has .speaker and .text fields)
    let formatted: Vec<String> = segments
        .iter()
        .map(|s| format!("{}: {}", s.speaker, s.text))
        .collect();

    drop(kb_guard); // Release lock before LLM call

    // Process with LLM
    match assistant.process_meeting_end(&formatted, &meeting.title).await {
        Ok(highlights) => HighlightsResult {
            summary: highlights.summary,
            key_topics: highlights.key_topics,
            action_items: highlights.action_items.into_iter().map(|a| ActionItemResult {
                task: a.task,
                assignee: a.assignee,
                deadline: a.deadline,
            }).collect(),
            decisions: highlights.decisions,
            highlights: highlights.highlights,
            follow_ups: highlights.follow_ups,
            error: None,
        },
        Err(e) => HighlightsResult {
            error: Some(e),
            ..Default::default()
        },
    }
}

/// Extract entities from text using GLiNER
async fn process_entity_extraction(
    text: &str,
    _source: &str,
    _timestamp_ms: u64,
    entity_engine: Option<&Arc<SyncRwLock<Option<EntityEngine>>>>,
) -> EntityResult {
    let Some(engine_lock) = entity_engine else {
        return EntityResult {
            error: Some("Entity engine not available".to_string()),
            ..Default::default()
        };
    };

    let guard = engine_lock.read();
    let Some(ref engine) = *guard else {
        return EntityResult {
            error: Some("Entity engine not initialized".to_string()),
            ..Default::default()
        };
    };

    match engine.extract_with_relations(text) {
        Ok((entities, relationships)) => EntityResult {
            entities: entities.into_iter().map(|e| ExtractedEntity {
                text: e.text,
                label: e.label,
                confidence: e.confidence,
            }).collect(),
            relationships: relationships.into_iter().map(|r| ExtractedRelationship {
                source: r.source,
                relation: r.relation,
                target: r.target,
                confidence: r.confidence,
            }).collect(),
            error: None,
        },
        Err(e) => EntityResult {
            error: Some(e),
            ..Default::default()
        },
    }
}

/// Convenience struct to hold all worker dependencies
pub struct WorkerDependencies {
    pub llm: Option<Arc<MeetingAssistant>>,
    pub kb: Option<Arc<RwLock<Option<KnowledgeBase>>>>,
    pub entity_engine: Option<Arc<SyncRwLock<Option<EntityEngine>>>>,
}

impl Clone for WorkerDependencies {
    fn clone(&self) -> Self {
        Self {
            llm: self.llm.clone(),
            kb: self.kb.clone(),
            entity_engine: self.entity_engine.clone(),
        }
    }
}

/// Create a job processor function with dependencies
pub fn create_job_processor(
    deps: WorkerDependencies,
) -> impl Fn(AgentJob, Arc<RwLock<QueueStats>>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Clone + Send + Sync + 'static
{
    move |job, stats| {
        let deps = deps.clone();
        Box::pin(async move {
            process_agent_job(
                job,
                stats,
                deps.llm,
                deps.kb,
                deps.entity_engine,
            ).await;
        })
    }
}
