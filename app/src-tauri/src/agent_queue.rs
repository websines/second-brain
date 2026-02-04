//! Agent Queue System
//!
//! In-memory job queue using tokio mpsc channels for running AI agents:
//! - RealtimeSuggestions: During meetings, generates contextual suggestions
//! - AnswerQuestion: Answers user questions using KB + LLM
//! - PostMeetingHighlights: Extracts highlights after meeting ends
//! - EntityExtraction: Background NER on text segments

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use serde::{Deserialize, Serialize};

/// Job types that agents can process
#[derive(Debug, Clone)]
pub enum AgentJob {
    /// Generate real-time suggestions during meeting
    RealtimeSuggestions {
        meeting_id: String,
        recent_transcripts: Vec<String>,
        context: Option<String>,
        response_tx: mpsc::Sender<RealtimeSuggestionResult>,
    },

    /// Answer a user question
    AnswerQuestion {
        question: String,
        context: Option<String>,
        response_tx: mpsc::Sender<AnswerResult>,
    },

    /// Extract highlights from completed meeting
    PostMeetingHighlights {
        meeting_id: String,
        response_tx: mpsc::Sender<HighlightsResult>,
    },

    /// Extract entities from text
    EntityExtraction {
        text: String,
        source: String,
        timestamp_ms: u64,
        response_tx: mpsc::Sender<EntityResult>,
    },

    /// Shutdown signal
    Shutdown,
}

/// Result types for each agent
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RealtimeSuggestionResult {
    pub insight: Option<String>,
    pub question: Option<String>,
    pub related_info: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnswerResult {
    pub answer: String,
    pub sources: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HighlightsResult {
    pub summary: Option<String>,
    pub key_topics: Vec<String>,
    pub action_items: Vec<ActionItemResult>,
    pub decisions: Vec<String>,
    pub highlights: Vec<String>,
    pub follow_ups: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionItemResult {
    pub task: String,
    pub assignee: Option<String>,
    pub deadline: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityResult {
    pub entities: Vec<ExtractedEntity>,
    pub relationships: Vec<ExtractedRelationship>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntity {
    pub text: String,
    pub label: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedRelationship {
    pub source: String,
    pub relation: String,
    pub target: String,
    pub confidence: f32,
}

/// Queue statistics
#[derive(Debug, Clone, Default, Serialize)]
pub struct QueueStats {
    pub pending_jobs: usize,
    pub processed_jobs: u64,
    pub failed_jobs: u64,
    pub workers_active: usize,
}

/// The main job queue that distributes work to agent workers
pub struct AgentQueue {
    job_tx: mpsc::Sender<AgentJob>,
    stats: Arc<RwLock<QueueStats>>,
}

impl AgentQueue {
    /// Create a new agent queue with specified buffer size
    pub fn new(buffer_size: usize) -> (Self, mpsc::Receiver<AgentJob>) {
        let (job_tx, job_rx) = mpsc::channel(buffer_size);
        let stats = Arc::new(RwLock::new(QueueStats::default()));

        (Self { job_tx, stats }, job_rx)
    }

    /// Submit a job to the queue
    pub async fn submit(&self, job: AgentJob) -> Result<(), String> {
        {
            let mut stats = self.stats.write().await;
            stats.pending_jobs += 1;
        }

        self.job_tx.send(job).await
            .map_err(|e| format!("Failed to submit job: {}", e))
    }

    /// Get current queue statistics
    pub async fn get_stats(&self) -> QueueStats {
        self.stats.read().await.clone()
    }

    /// Mark a job as completed
    pub async fn mark_completed(&self) {
        let mut stats = self.stats.write().await;
        if stats.pending_jobs > 0 {
            stats.pending_jobs -= 1;
        }
        stats.processed_jobs += 1;
    }

    /// Mark a job as failed
    pub async fn mark_failed(&self) {
        let mut stats = self.stats.write().await;
        if stats.pending_jobs > 0 {
            stats.pending_jobs -= 1;
        }
        stats.failed_jobs += 1;
    }
}

/// Worker pool that processes jobs from the queue
pub struct WorkerPool {
    handles: Vec<tokio::task::JoinHandle<()>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl WorkerPool {
    /// Start the worker pool with the given number of workers
    pub fn start<F, Fut>(
        num_workers: usize,
        job_rx: mpsc::Receiver<AgentJob>,
        queue_stats: Arc<RwLock<QueueStats>>,
        process_job: F,
    ) -> Self
    where
        F: Fn(AgentJob, Arc<RwLock<QueueStats>>) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let job_rx = Arc::new(Mutex::new(job_rx));
        let (shutdown_tx, _shutdown_rx) = mpsc::channel::<()>(1);
        let mut handles = Vec::with_capacity(num_workers);

        for worker_id in 0..num_workers {
            let job_rx = job_rx.clone();
            let stats = queue_stats.clone();
            let process = process_job.clone();

            let handle = tokio::spawn(async move {
                println!("[Worker-{}] Started", worker_id);

                loop {
                    let job = {
                        let mut rx = job_rx.lock().await;
                        rx.recv().await
                    };

                    match job {
                        Some(AgentJob::Shutdown) => {
                            println!("[Worker-{}] Received shutdown signal", worker_id);
                            break;
                        }
                        Some(job) => {
                            {
                                let mut s = stats.write().await;
                                s.workers_active += 1;
                            }

                            process(job, stats.clone()).await;

                            {
                                let mut s = stats.write().await;
                                s.workers_active = s.workers_active.saturating_sub(1);
                            }
                        }
                        None => {
                            println!("[Worker-{}] Channel closed, shutting down", worker_id);
                            break;
                        }
                    }
                }

                println!("[Worker-{}] Stopped", worker_id);
            });

            handles.push(handle);
        }

        Self {
            handles,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Shutdown all workers gracefully
    pub async fn shutdown(mut self) {
        drop(self.shutdown_tx.take());

        for handle in self.handles {
            let _ = handle.await;
        }
    }
}

/// Helper to create a one-shot response channel
pub fn response_channel<T>() -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
    mpsc::channel(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_queue_submit() {
        let (queue, mut rx) = AgentQueue::new(10);

        let (resp_tx, _resp_rx) = response_channel();
        queue.submit(AgentJob::AnswerQuestion {
            question: "Test?".to_string(),
            context: None,
            response_tx: resp_tx,
        }).await.unwrap();

        let stats = queue.get_stats().await;
        assert_eq!(stats.pending_jobs, 1);

        // Receive the job
        let job = rx.recv().await;
        assert!(matches!(job, Some(AgentJob::AnswerQuestion { .. })));
    }
}
