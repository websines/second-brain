use crate::embeddings::EmbeddingEngine;
use crate::entities::{Entity, EntityEngine, Relationship};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::sql::Thing;
use surrealdb::Surreal;

/// A meeting record in the knowledge base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: Option<Thing>,
    pub title: String,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub participants: Vec<String>,
    pub summary: Option<String>,
}

/// A transcript segment from a meeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: Option<Thing>,
    pub meeting_id: String,
    pub speaker: String,
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub embedding: Vec<f32>,
}

/// An action item extracted from meetings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub id: Option<Thing>,
    pub meeting_id: String,
    pub text: String,
    pub assignee: Option<String>,
    pub deadline: Option<String>,
    pub status: String, // "open", "in_progress", "done"
    pub created_at: u64,
}

/// A decision made in a meeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: Option<Thing>,
    pub meeting_id: String,
    pub text: String,
    pub participants: Vec<String>,
    pub created_at: u64,
}

/// A person mentioned in meetings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: Option<Thing>,
    pub name: String,
    pub aliases: Vec<String>,
    pub first_seen: u64,
    pub last_seen: u64,
}

/// A topic/project discussed in meetings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topic {
    pub id: Option<Thing>,
    pub name: String,
    pub embedding: Vec<f32>,
    pub mention_count: u32,
    pub last_mentioned: u64,
}

/// A knowledge source (URL, document, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSource {
    pub id: Option<Thing>,
    pub url: String,
    pub title: String,
    pub source_type: String, // "url", "file", "search"
    pub raw_content: String,
    pub tags: Vec<String>,
    pub created_at: u64,
    pub last_updated: u64,
}

/// A chunk from a knowledge source with embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeChunk {
    pub id: Option<Thing>,
    pub source_id: String,
    pub text: String,
    pub chunk_index: i32,
    pub embedding: Vec<f32>,
}

/// Link between a meeting and a knowledge source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingKnowledge {
    pub id: Option<Thing>,
    pub meeting_id: String,
    pub source_id: String,
    pub relevance_score: f32,
    pub assigned_by: String, // "user" or "auto"
}

/// Search result from knowledge chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub chunk: KnowledgeChunk,
    pub source_title: String,
    pub source_url: String,
    pub similarity: f32,
}

// ============================================================================
// Graph-RAG Types
// ============================================================================

/// Context retrieved via Graph-RAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRAGContext {
    /// Entities extracted from the query
    pub query_entities: Vec<Entity>,
    /// Relevant meetings (from graph traversal)
    pub related_meetings: Vec<MeetingContext>,
    /// Related people (from graph)
    pub related_people: Vec<PersonContext>,
    /// Related topics (from graph)
    pub related_topics: Vec<TopicContext>,
    /// Open action items (temporal)
    pub open_actions: Vec<ActionItem>,
    /// Recent decisions (temporal)
    pub recent_decisions: Vec<Decision>,
    /// Vector-similar chunks
    pub similar_chunks: Vec<KnowledgeSearchResult>,
    /// Temporal info
    pub temporal_context: Option<TemporalContext>,
}

/// Meeting with temporal context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingContext {
    pub meeting: Meeting,
    pub days_ago: i64,
    pub relevant_segments: Vec<TranscriptSegment>,
}

/// Person with meeting history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonContext {
    pub name: String,
    pub last_seen_days_ago: i64,
    pub meeting_count: usize,
    pub recent_topics: Vec<String>,
}

/// Topic with temporal info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicContext {
    pub name: String,
    pub last_mentioned_days_ago: i64,
    pub mention_count: u32,
    pub related_people: Vec<String>,
}

/// Temporal context parsed from query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalContext {
    pub time_reference: String,        // "3 weeks ago", "last month", etc.
    pub start_timestamp: Option<u64>,  // Computed timestamp range
    pub end_timestamp: Option<u64>,
}

/// Internal struct for deserializing chunk with similarity from query
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChunkWithSimilarity {
    pub id: Option<Thing>,
    pub source_id: String,
    pub text: String,
    pub chunk_index: i32,
    pub embedding: Vec<f32>,
    pub similarity: f32,
}

/// Search result from the knowledge base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub segment: TranscriptSegment,
    pub meeting_title: String,
    pub similarity: f32,
}

/// Meeting statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingStats {
    pub segment_count: usize,
    pub action_count: usize,
    pub decision_count: usize,
    pub topic_count: usize,
    pub people_count: usize,
    pub duration_ms: u64,
    pub total_words: usize,
}

/// The main knowledge base powered by SurrealDB
pub struct KnowledgeBase {
    db: Surreal<Db>,
    embedding_engine: Arc<EmbeddingEngine>,
    entity_engine: Arc<EntityEngine>,
}

impl KnowledgeBase {
    /// Initialize the knowledge base
    pub async fn new(
        data_dir: &PathBuf,
        embedding_engine: Arc<EmbeddingEngine>,
        entity_engine: Arc<EntityEngine>,
    ) -> Result<Self, String> {
        let db_path = data_dir.join("knowledge.db");

        // Connect to embedded SurrealDB with RocksDB backend
        let db = Surreal::new::<RocksDb>(db_path.to_str().unwrap())
            .await
            .map_err(|e| format!("Failed to open database: {}", e))?;

        // Select namespace and database
        db.use_ns("second_brain")
            .use_db("knowledge")
            .await
            .map_err(|e| format!("Failed to select namespace: {}", e))?;

        let kb = Self {
            db,
            embedding_engine,
            entity_engine,
        };

        // Initialize schema
        kb.init_schema().await?;

        println!("Knowledge base initialized at {:?}", data_dir);
        Ok(kb)
    }

    /// Initialize database schema
    async fn init_schema(&self) -> Result<(), String> {
        // Define tables with indexes
        let schema = r#"
            -- Meetings table
            DEFINE TABLE meeting SCHEMAFULL;
            DEFINE FIELD title ON meeting TYPE string;
            DEFINE FIELD start_time ON meeting TYPE int;
            DEFINE FIELD end_time ON meeting TYPE option<int>;
            DEFINE FIELD participants ON meeting TYPE array<string>;
            DEFINE FIELD summary ON meeting TYPE option<string>;
            DEFINE INDEX idx_meeting_time ON meeting FIELDS start_time;

            -- Transcript segments with vector embeddings
            DEFINE TABLE segment SCHEMAFULL;
            DEFINE FIELD meeting_id ON segment TYPE string;
            DEFINE FIELD speaker ON segment TYPE string;
            DEFINE FIELD text ON segment TYPE string;
            DEFINE FIELD start_ms ON segment TYPE int;
            DEFINE FIELD end_ms ON segment TYPE int;
            DEFINE FIELD embedding ON segment TYPE array<float>;
            DEFINE INDEX idx_segment_meeting ON segment FIELDS meeting_id;
            DEFINE INDEX idx_segment_speaker ON segment FIELDS speaker;

            -- Action items
            DEFINE TABLE action_item SCHEMAFULL;
            DEFINE FIELD meeting_id ON action_item TYPE string;
            DEFINE FIELD text ON action_item TYPE string;
            DEFINE FIELD assignee ON action_item TYPE option<string>;
            DEFINE FIELD deadline ON action_item TYPE option<string>;
            DEFINE FIELD status ON action_item TYPE string;
            DEFINE FIELD created_at ON action_item TYPE int;
            DEFINE INDEX idx_action_status ON action_item FIELDS status;
            DEFINE INDEX idx_action_assignee ON action_item FIELDS assignee;

            -- Decisions
            DEFINE TABLE decision SCHEMAFULL;
            DEFINE FIELD meeting_id ON decision TYPE string;
            DEFINE FIELD text ON decision TYPE string;
            DEFINE FIELD participants ON decision TYPE array<string>;
            DEFINE FIELD created_at ON decision TYPE int;

            -- People
            DEFINE TABLE person SCHEMAFULL;
            DEFINE FIELD name ON person TYPE string;
            DEFINE FIELD aliases ON person TYPE array<string>;
            DEFINE FIELD first_seen ON person TYPE int;
            DEFINE FIELD last_seen ON person TYPE int;
            DEFINE INDEX idx_person_name ON person FIELDS name UNIQUE;

            -- Topics
            DEFINE TABLE topic SCHEMAFULL;
            DEFINE FIELD name ON topic TYPE string;
            DEFINE FIELD embedding ON topic TYPE array<float>;
            DEFINE FIELD mention_count ON topic TYPE int;
            DEFINE FIELD last_mentioned ON topic TYPE int;
            DEFINE INDEX idx_topic_name ON topic FIELDS name UNIQUE;

            -- Relations (graph edges)
            DEFINE TABLE mentioned_in SCHEMAFULL;
            DEFINE TABLE participated_in SCHEMAFULL;
            DEFINE TABLE discussed_in SCHEMAFULL;
            DEFINE TABLE assigned_to SCHEMAFULL;

            -- Entity relationships (extracted by GLiNER multitask)
            DEFINE TABLE entity_relation SCHEMAFULL;
            DEFINE FIELD source_entity ON entity_relation TYPE string;
            DEFINE FIELD source_type ON entity_relation TYPE string;
            DEFINE FIELD relation ON entity_relation TYPE string;
            DEFINE FIELD target_entity ON entity_relation TYPE string;
            DEFINE FIELD target_type ON entity_relation TYPE string;
            DEFINE FIELD confidence ON entity_relation TYPE float;
            DEFINE FIELD meeting_id ON entity_relation TYPE option<string>;
            DEFINE FIELD knowledge_source_id ON entity_relation TYPE option<string>;
            DEFINE FIELD created_at ON entity_relation TYPE int;
            DEFINE INDEX idx_relation_source ON entity_relation FIELDS source_entity;
            DEFINE INDEX idx_relation_target ON entity_relation FIELDS target_entity;
            DEFINE INDEX idx_relation_type ON entity_relation FIELDS relation;

            -- Knowledge sources (crawled URLs, documents)
            DEFINE TABLE knowledge_source SCHEMAFULL;
            DEFINE FIELD url ON knowledge_source TYPE string;
            DEFINE FIELD title ON knowledge_source TYPE string;
            DEFINE FIELD source_type ON knowledge_source TYPE string;
            DEFINE FIELD raw_content ON knowledge_source TYPE string;
            DEFINE FIELD tags ON knowledge_source TYPE array<string>;
            DEFINE FIELD created_at ON knowledge_source TYPE int;
            DEFINE FIELD last_updated ON knowledge_source TYPE int;
            DEFINE INDEX idx_source_url ON knowledge_source FIELDS url UNIQUE;
            DEFINE INDEX idx_source_tags ON knowledge_source FIELDS tags;

            -- Knowledge chunks with embeddings
            DEFINE TABLE knowledge_chunk SCHEMAFULL;
            DEFINE FIELD source_id ON knowledge_chunk TYPE string;
            DEFINE FIELD text ON knowledge_chunk TYPE string;
            DEFINE FIELD chunk_index ON knowledge_chunk TYPE int;
            DEFINE FIELD embedding ON knowledge_chunk TYPE array<float>;
            DEFINE INDEX idx_chunk_source ON knowledge_chunk FIELDS source_id;

            -- Meeting-knowledge links
            DEFINE TABLE meeting_knowledge SCHEMAFULL;
            DEFINE FIELD meeting_id ON meeting_knowledge TYPE string;
            DEFINE FIELD source_id ON meeting_knowledge TYPE string;
            DEFINE FIELD relevance_score ON meeting_knowledge TYPE float;
            DEFINE FIELD assigned_by ON meeting_knowledge TYPE string;
            DEFINE INDEX idx_mk_meeting ON meeting_knowledge FIELDS meeting_id;
            DEFINE INDEX idx_mk_source ON meeting_knowledge FIELDS source_id;
        "#;

        self.db
            .query(schema)
            .await
            .map_err(|e| format!("Failed to create schema: {}", e))?;

        Ok(())
    }

    /// Create a new meeting
    pub async fn create_meeting(&self, title: &str, participants: Vec<String>) -> Result<String, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let meeting = Meeting {
            id: None,
            title: title.to_string(),
            start_time: now,
            end_time: None,
            participants,
            summary: None,
        };

        let created: Option<Meeting> = self.db
            .create("meeting")
            .content(meeting)
            .await
            .map_err(|e| format!("Failed to create meeting: {}", e))?;

        match created {
            Some(m) => Ok(m.id.map(|t| t.to_string()).unwrap_or_default()),
            None => Err("Failed to create meeting".to_string()),
        }
    }

    /// End a meeting and set summary
    pub async fn end_meeting(&self, meeting_id: &str, summary: Option<String>) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Normalize meeting_id - strip prefix if present
        let id_part = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };

        println!("[KB] Ending meeting: {} (normalized: {})", meeting_id, id_part);

        self.db
            .query("UPDATE type::thing('meeting', $id) SET end_time = $end_time, summary = $summary")
            .bind(("id", id_part.to_string()))
            .bind(("end_time", now))
            .bind(("summary", summary))
            .await
            .map_err(|e| format!("Failed to end meeting: {}", e))?;

        println!("[KB] Meeting ended successfully with end_time: {}", now);
        Ok(())
    }

    /// Auto-end stale meetings (meetings without end_time older than max_age_hours)
    /// Returns the number of meetings that were auto-ended
    pub async fn auto_end_stale_meetings(&self, max_age_hours: u64) -> Result<usize, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let max_age_ms = max_age_hours * 60 * 60 * 1000;
        let cutoff_time = now.saturating_sub(max_age_ms);

        // Find all meetings without end_time that started before the cutoff
        let mut result = self.db
            .query("SELECT id, title, start_time FROM meeting WHERE end_time IS NONE AND start_time < $cutoff")
            .bind(("cutoff", cutoff_time))
            .await
            .map_err(|e| format!("Failed to query stale meetings: {}", e))?;

        #[derive(serde::Deserialize)]
        struct StaleMeeting {
            id: surrealdb::sql::Thing,
            title: String,
            start_time: u64,
        }

        let stale_meetings: Vec<StaleMeeting> = result.take(0)
            .map_err(|e| format!("Failed to parse stale meetings: {}", e))?;

        if stale_meetings.is_empty() {
            return Ok(0);
        }

        println!("[KB] Found {} stale meetings to auto-end", stale_meetings.len());

        // End each stale meeting
        for meeting in &stale_meetings {
            let meeting_id = &meeting.id.id.to_string();
            println!("[KB] Auto-ending stale meeting: {} ({})", meeting.title, meeting_id);

            // Set end_time based on last segment or estimate 1 hour duration
            let estimated_end = meeting.start_time + (60 * 60 * 1000);

            // Just set end_time, leave summary as None so user can generate it later
            self.db
                .query("UPDATE type::thing('meeting', $id) SET end_time = $end_time")
                .bind(("id", meeting_id.clone()))
                .bind(("end_time", estimated_end))
                .await
                .map_err(|e| format!("Failed to auto-end meeting {}: {}", meeting_id, e))?;
        }

        Ok(stale_meetings.len())
    }

    /// Add a transcript segment
    pub async fn add_segment(
        &self,
        meeting_id: &str,
        speaker: &str,
        text: &str,
        start_ms: u64,
        end_ms: u64,
    ) -> Result<String, String> {
        println!("[KB::add_segment] Starting for meeting={}, speaker={}, text_len={}",
            meeting_id, speaker, text.len());

        // Generate embedding for the text
        println!("[KB::add_segment] Generating embedding...");
        let embedding = self.embedding_engine.embed(text)?;
        println!("[KB::add_segment] Embedding generated, dim={}", embedding.len());

        let segment = TranscriptSegment {
            id: None,
            meeting_id: meeting_id.to_string(),
            speaker: speaker.to_string(),
            text: text.to_string(),
            start_ms,
            end_ms,
            embedding,
        };

        println!("[KB::add_segment] Creating segment in DB...");
        let created: Option<TranscriptSegment> = self.db
            .create("segment")
            .content(segment)
            .await
            .map_err(|e| format!("Failed to create segment: {}", e))?;
        println!("[KB::add_segment] Segment created in DB");

        // Extract entities and relationships using GLiNER multitask
        println!("[KB::add_segment] Extracting entities...");
        let (entities, relationships) = self.entity_engine.extract_with_relations(text)?;
        println!("[KB::add_segment] Found {} entities, {} relationships", entities.len(), relationships.len());

        self.process_entities(meeting_id, &entities).await?;
        self.process_relationships(meeting_id, &relationships).await?;
        println!("[KB::add_segment] Entities and relationships processed");

        match created {
            Some(s) => {
                let id = s.id.map(|t| t.to_string()).unwrap_or_default();
                println!("[KB::add_segment] Success! Segment ID: {}", id);
                Ok(id)
            }
            None => Err("Failed to create segment".to_string()),
        }
    }

    /// Process extracted entities and create graph relations
    async fn process_entities(&self, meeting_id: &str, entities: &[Entity]) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Extract just the ID part for use with type::thing()
        let meeting_id_part = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };
        let meeting_id_owned = meeting_id_part.to_string();

        for entity in entities {
            let entity_text = entity.text.clone();
            let meeting_id_clone = meeting_id_owned.clone();

            match entity.label.as_str() {
                "person" => {
                    // Upsert person
                    self.db
                        .query(r#"
                            UPSERT person SET
                                name = $name,
                                aliases = array::union(aliases, []),
                                first_seen = math::min(first_seen, $now),
                                last_seen = $now
                            WHERE name = $name
                        "#)
                        .bind(("name", entity_text.clone()))
                        .bind(("now", now))
                        .await
                        .ok();

                    // Create relation
                    self.db
                        .query("RELATE (SELECT * FROM person WHERE name = $name) -> mentioned_in -> type::thing('meeting', $meeting_id)")
                        .bind(("name", entity_text))
                        .bind(("meeting_id", meeting_id_clone))
                        .await
                        .ok();
                }
                "topic" | "project" | "product" => {
                    // Upsert topic
                    let topic_embedding = self.embedding_engine.embed(&entity.text).unwrap_or_default();

                    self.db
                        .query(r#"
                            UPSERT topic SET
                                name = $name,
                                embedding = $embedding,
                                mention_count = mention_count + 1,
                                last_mentioned = $now
                            WHERE name = $name
                        "#)
                        .bind(("name", entity_text.clone()))
                        .bind(("embedding", topic_embedding))
                        .bind(("now", now))
                        .await
                        .ok();

                    // Create relation
                    self.db
                        .query("RELATE (SELECT * FROM topic WHERE name = $name) -> discussed_in -> type::thing('meeting', $meeting_id)")
                        .bind(("name", entity_text))
                        .bind(("meeting_id", meeting_id_clone))
                        .await
                        .ok();
                }
                "action_item" => {
                    let action = ActionItem {
                        id: None,
                        meeting_id: meeting_id_clone,
                        text: entity_text,
                        assignee: None,
                        deadline: None,
                        status: "open".to_string(),
                        created_at: now,
                    };

                    self.db
                        .create::<Option<ActionItem>>("action_item")
                        .content(action)
                        .await
                        .ok();
                }
                "decision" => {
                    let decision = Decision {
                        id: None,
                        meeting_id: meeting_id_clone,
                        text: entity_text,
                        participants: vec![],
                        created_at: now,
                    };

                    self.db
                        .create::<Option<Decision>>("decision")
                        .content(decision)
                        .await
                        .ok();
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Process extracted relationships and store in graph
    async fn process_relationships(&self, meeting_id: &str, relationships: &[Relationship]) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for rel in relationships {
            // Only store relationships with reasonable confidence
            if rel.confidence < 0.5 {
                continue;
            }

            #[derive(Serialize)]
            struct EntityRelation {
                source_entity: String,
                source_type: String,
                relation: String,
                target_entity: String,
                target_type: String,
                confidence: f32,
                meeting_id: Option<String>,
                created_at: u64,
            }

            let entity_rel = EntityRelation {
                source_entity: rel.source.clone(),
                source_type: rel.source_type.clone(),
                relation: rel.relation.clone(),
                target_entity: rel.target.clone(),
                target_type: rel.target_type.clone(),
                confidence: rel.confidence,
                meeting_id: Some(meeting_id.to_string()),
                created_at: now,
            };

            self.db
                .create::<Option<serde_json::Value>>("entity_relation")
                .content(entity_rel)
                .await
                .ok(); // Ignore errors for individual relations
        }

        if !relationships.is_empty() {
            println!("Stored {} relationships for meeting {}", relationships.len(), meeting_id);
        }

        Ok(())
    }

    /// Process entities from a knowledge source (not a meeting)
    async fn process_entities_for_source(&self, _source_id: &str, entities: &[Entity]) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for entity in entities {
            let entity_text = entity.text.clone();

            match entity.label.as_str() {
                "person" => {
                    // Upsert person
                    self.db
                        .query(r#"
                            UPSERT person SET
                                name = $name,
                                aliases = array::union(aliases, []),
                                first_seen = math::min(first_seen, $now),
                                last_seen = $now
                            WHERE name = $name
                        "#)
                        .bind(("name", entity_text.clone()))
                        .bind(("now", now))
                        .await
                        .ok();
                }
                "topic" | "project" | "product" | "organization" => {
                    // Upsert topic
                    let topic_embedding = self.embedding_engine.embed(&entity.text).unwrap_or_default();

                    self.db
                        .query(r#"
                            UPSERT topic SET
                                name = $name,
                                embedding = $embedding,
                                mention_count = mention_count + 1,
                                last_mentioned = $now
                            WHERE name = $name
                        "#)
                        .bind(("name", entity_text.clone()))
                        .bind(("embedding", topic_embedding))
                        .bind(("now", now))
                        .await
                        .ok();
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Process relationships from a knowledge source (not a meeting)
    async fn process_relationships_for_source(&self, source_id: &str, relationships: &[Relationship]) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for rel in relationships {
            if rel.confidence < 0.5 {
                continue;
            }

            #[derive(Serialize)]
            struct EntityRelation {
                source_entity: String,
                source_type: String,
                relation: String,
                target_entity: String,
                target_type: String,
                confidence: f32,
                meeting_id: Option<String>,
                knowledge_source_id: Option<String>,
                created_at: u64,
            }

            let entity_rel = EntityRelation {
                source_entity: rel.source.clone(),
                source_type: rel.source_type.clone(),
                relation: rel.relation.clone(),
                target_entity: rel.target.clone(),
                target_type: rel.target_type.clone(),
                confidence: rel.confidence,
                meeting_id: None,
                knowledge_source_id: Some(source_id.to_string()),
                created_at: now,
            };

            self.db
                .create::<Option<serde_json::Value>>("entity_relation")
                .content(entity_rel)
                .await
                .ok();
        }

        Ok(())
    }

    /// Search for similar segments using vector similarity
    pub async fn search_similar(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, String> {
        let query_embedding = self.embedding_engine.embed(query)?;

        // SurrealDB vector search
        let results: Vec<TranscriptSegment> = self.db
            .query(r#"
                SELECT *, vector::similarity::cosine(embedding, $embedding) AS similarity
                FROM segment
                ORDER BY similarity DESC
                LIMIT $limit
            "#)
            .bind(("embedding", query_embedding))
            .bind(("limit", limit))
            .await
            .map_err(|e| format!("Search failed: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract results: {}", e))?;

        // Get meeting titles
        let mut search_results = Vec::new();
        for segment in results {
            let meeting_title = self.get_meeting_title(&segment.meeting_id).await?;
            search_results.push(SearchResult {
                segment,
                meeting_title,
                similarity: 0.0, // Will be filled by the query
            });
        }

        Ok(search_results)
    }

    /// Get meeting title by ID
    async fn get_meeting_title(&self, meeting_id: &str) -> Result<String, String> {
        let meeting: Option<Meeting> = self.db
            .select(("meeting", meeting_id))
            .await
            .map_err(|e| format!("Failed to get meeting: {}", e))?;

        Ok(meeting.map(|m| m.title).unwrap_or_else(|| "Unknown".to_string()))
    }

    /// Get all open action items
    pub async fn get_open_actions(&self) -> Result<Vec<ActionItem>, String> {
        let actions: Vec<ActionItem> = self.db
            .query("SELECT * FROM action_item WHERE status = 'open' ORDER BY created_at DESC")
            .await
            .map_err(|e| format!("Query failed: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract actions: {}", e))?;

        Ok(actions)
    }

    /// Get recent decisions
    pub async fn get_recent_decisions(&self, limit: usize) -> Result<Vec<Decision>, String> {
        let decisions: Vec<Decision> = self.db
            .query("SELECT * FROM decision ORDER BY created_at DESC LIMIT $limit")
            .bind(("limit", limit))
            .await
            .map_err(|e| format!("Query failed: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract decisions: {}", e))?;

        Ok(decisions)
    }

    /// Get people mentioned with a person
    pub async fn get_related_people(&self, person_name: &str) -> Result<Vec<String>, String> {
        let name_owned = person_name.to_string();

        let people: Vec<Person> = self.db
            .query(r#"
                SELECT DISTINCT person.name FROM person
                WHERE id IN (
                    SELECT in FROM mentioned_in
                    WHERE out IN (
                        SELECT out FROM mentioned_in
                        WHERE in = (SELECT id FROM person WHERE name = $name)
                    )
                )
                AND name != $name
            "#)
            .bind(("name", name_owned))
            .await
            .map_err(|e| format!("Query failed: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract people: {}", e))?;

        Ok(people.into_iter().map(|p| p.name).collect())
    }

    /// Full-text search in transcripts
    pub async fn search_text(&self, query: &str, limit: usize) -> Result<Vec<TranscriptSegment>, String> {
        let query_owned = query.to_string();

        let segments: Vec<TranscriptSegment> = self.db
            .query("SELECT * FROM segment WHERE text CONTAINS $query LIMIT $limit")
            .bind(("query", query_owned))
            .bind(("limit", limit))
            .await
            .map_err(|e| format!("Search failed: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract segments: {}", e))?;

        Ok(segments)
    }

    // ==================== Knowledge Source Methods ====================

    /// Add a knowledge source (URL, document) and chunk it
    pub async fn add_knowledge_source(
        &self,
        url: &str,
        title: &str,
        content: &str,
        source_type: &str,
        tags: Vec<String>,
    ) -> Result<String, String> {
        use crate::chunker::DocumentChunker;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Create the knowledge source
        let source = KnowledgeSource {
            id: None,
            url: url.to_string(),
            title: title.to_string(),
            source_type: source_type.to_string(),
            raw_content: content.to_string(),
            tags,
            created_at: now,
            last_updated: now,
        };

        let created: Option<KnowledgeSource> = self.db
            .create("knowledge_source")
            .content(source)
            .await
            .map_err(|e| format!("Failed to create knowledge source: {}", e))?;

        let source_id = match created {
            Some(s) => s.id.map(|t| t.to_string()).unwrap_or_default(),
            None => return Err("Failed to create knowledge source".to_string()),
        };

        // Chunk the content
        let chunker = DocumentChunker::new();
        let chunks = chunker.chunk_markdown(content);

        println!("Chunking content: {} chars -> {} chunks", content.len(), chunks.len());

        // Create chunks with embeddings
        let mut chunk_count = 0;
        for chunk in chunks {
            let embedding = self.embedding_engine.embed(&chunk.text)?;

            let kb_chunk = KnowledgeChunk {
                id: None,
                source_id: source_id.clone(),
                text: chunk.text,
                chunk_index: chunk.chunk_index as i32,
                embedding,
            };

            self.db
                .create::<Option<KnowledgeChunk>>("knowledge_chunk")
                .content(kb_chunk)
                .await
                .map_err(|e| format!("Failed to create chunk: {}", e))?;

            chunk_count += 1;
        }

        println!("Added knowledge source: {} (id={}) with {} chunks", title, source_id, chunk_count);

        // Extract entities and relationships from the content for Graph-RAG
        // Process in chunks to avoid overwhelming the model with huge texts
        let text_chunks: Vec<&str> = content.split("\n\n").filter(|s| s.len() > 50).take(20).collect();
        let mut total_entities = 0;
        let mut total_relationships = 0;

        for text_chunk in text_chunks {
            match self.entity_engine.extract_with_relations(text_chunk) {
                Ok((entities, relationships)) => {
                    // Store entities (without meeting_id since this is a knowledge source)
                    self.process_entities_for_source(&source_id, &entities).await.ok();
                    self.process_relationships_for_source(&source_id, &relationships).await.ok();
                    total_entities += entities.len();
                    total_relationships += relationships.len();
                }
                Err(e) => {
                    println!("Entity extraction failed for chunk: {}", e);
                }
            }
        }

        println!("Extracted {} entities and {} relationships from knowledge source", total_entities, total_relationships);
        Ok(source_id)
    }

    /// Get all knowledge sources, optionally filtered by tags
    pub async fn get_knowledge_sources(
        &self,
        tags: Option<Vec<String>>,
    ) -> Result<Vec<KnowledgeSource>, String> {
        let sources: Vec<KnowledgeSource> = if let Some(tag_list) = tags {
            self.db
                .query("SELECT * FROM knowledge_source WHERE tags CONTAINSANY $tags ORDER BY last_updated DESC")
                .bind(("tags", tag_list))
                .await
                .map_err(|e| format!("Query failed: {}", e))?
                .take(0)
                .map_err(|e| format!("Failed to extract sources: {}", e))?
        } else {
            self.db
                .query("SELECT * FROM knowledge_source ORDER BY last_updated DESC")
                .await
                .map_err(|e| format!("Query failed: {}", e))?
                .take(0)
                .map_err(|e| format!("Failed to extract sources: {}", e))?
        };

        Ok(sources)
    }

    /// Get a single knowledge source by ID
    /// Accepts either full Thing string (knowledge_source:id) or just the ID part
    pub async fn get_knowledge_source(&self, source_id: &str) -> Result<Option<KnowledgeSource>, String> {
        // Extract just the ID part if full Thing string is passed
        let id_part = if source_id.starts_with("knowledge_source:") {
            source_id.strip_prefix("knowledge_source:").unwrap_or(source_id)
        } else {
            source_id
        };

        // Try using select first
        let source: Option<KnowledgeSource> = self.db
            .select(("knowledge_source", id_part))
            .await
            .map_err(|e| format!("Failed to get source: {}", e))?;

        // If select didn't find it, try a query with the full source_id
        if source.is_none() {
            // Try query with full Thing format
            let source_id_owned = source_id.to_string();
            let query_result: Vec<KnowledgeSource> = self.db
                .query("SELECT * FROM knowledge_source WHERE id = $id")
                .bind(("id", source_id_owned))
                .await
                .map_err(|e| format!("Query failed: {}", e))?
                .take(0)
                .map_err(|e| format!("Failed to extract source: {}", e))?;

            if let Some(s) = query_result.into_iter().next() {
                return Ok(Some(s));
            }
        }

        Ok(source)
    }

    /// Delete a knowledge source and its chunks
    pub async fn delete_knowledge_source(&self, source_id: &str) -> Result<(), String> {
        // Chunks store source_id as the full Thing string (knowledge_source:xyz)
        // But frontend may pass just the ID part (xyz)
        // We need to try both formats for deletion

        let full_source_id = if source_id.starts_with("knowledge_source:") {
            source_id.to_string()
        } else {
            format!("knowledge_source:{}", source_id)
        };

        let id_part = if source_id.starts_with("knowledge_source:") {
            source_id.strip_prefix("knowledge_source:").unwrap_or(source_id).to_string()
        } else {
            source_id.to_string()
        };

        println!("[KB Delete] Deleting source: id_part={}, full_source_id={}", id_part, full_source_id);

        // Delete all chunks for this source (try both formats)
        let delete_result = self.db
            .query("DELETE FROM knowledge_chunk WHERE source_id = $full_id OR source_id = $short_id")
            .bind(("full_id", full_source_id.clone()))
            .bind(("short_id", id_part.clone()))
            .await
            .map_err(|e| format!("Failed to delete chunks: {}", e))?;

        println!("[KB Delete] Chunk delete result: {:?}", delete_result.num_statements());

        // Delete all meeting links (try both formats)
        self.db
            .query("DELETE FROM meeting_knowledge WHERE source_id = $full_id OR source_id = $short_id")
            .bind(("full_id", full_source_id.clone()))
            .bind(("short_id", id_part.clone()))
            .await
            .map_err(|e| format!("Failed to delete meeting links: {}", e))?;

        // Delete the source itself
        self.db
            .delete::<Option<KnowledgeSource>>(("knowledge_source", id_part.as_str()))
            .await
            .map_err(|e| format!("Failed to delete source: {}", e))?;

        println!("[KB Delete] Source deleted successfully");
        Ok(())
    }

    /// Update tags for a knowledge source
    pub async fn update_source_tags(
        &self,
        source_id: &str,
        tags: Vec<String>,
    ) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let source_id_owned = source_id.to_string();

        self.db
            .query("UPDATE type::thing('knowledge_source', $id) SET tags = $tags, last_updated = $now")
            .bind(("id", source_id_owned))
            .bind(("tags", tags))
            .bind(("now", now))
            .await
            .map_err(|e| format!("Failed to update tags: {}", e))?;

        Ok(())
    }

    /// Search knowledge chunks using vector similarity
    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
        tags: Option<Vec<String>>,
    ) -> Result<Vec<KnowledgeSearchResult>, String> {
        let query_embedding = self.embedding_engine.embed(query)?;

        // Search with optional tag filtering using ChunkWithSimilarity to capture similarity
        let chunks_with_sim: Vec<ChunkWithSimilarity> = if let Some(tag_list) = tags {
            self.db
                .query(r#"
                    SELECT *, vector::similarity::cosine(embedding, $embedding) AS similarity
                    FROM knowledge_chunk
                    WHERE source_id IN (
                        SELECT VALUE id FROM knowledge_source WHERE tags CONTAINSANY $tags
                    )
                    ORDER BY similarity DESC
                    LIMIT $limit
                "#)
                .bind(("embedding", query_embedding.clone()))
                .bind(("tags", tag_list))
                .bind(("limit", limit))
                .await
                .map_err(|e| format!("Search failed: {}", e))?
                .take(0)
                .map_err(|e| format!("Failed to extract chunks: {}", e))?
        } else {
            self.db
                .query(r#"
                    SELECT *, vector::similarity::cosine(embedding, $embedding) AS similarity
                    FROM knowledge_chunk
                    ORDER BY similarity DESC
                    LIMIT $limit
                "#)
                .bind(("embedding", query_embedding.clone()))
                .bind(("limit", limit))
                .await
                .map_err(|e| format!("Search failed: {}", e))?
                .take(0)
                .map_err(|e| format!("Failed to extract chunks: {}", e))?
        };

        println!("Found {} chunks with similarity", chunks_with_sim.len());

        // Get source info for each chunk
        let mut results = Vec::new();
        for chunk_sim in &chunks_with_sim {
            println!(
                "  Chunk: source_id={}, text_len={}, similarity={:.4}",
                chunk_sim.source_id,
                chunk_sim.text.len(),
                chunk_sim.similarity
            );
        }

        for chunk_sim in chunks_with_sim {
            // Convert ChunkWithSimilarity to KnowledgeChunk
            let chunk = KnowledgeChunk {
                id: chunk_sim.id,
                source_id: chunk_sim.source_id.clone(),
                text: chunk_sim.text,
                chunk_index: chunk_sim.chunk_index,
                embedding: chunk_sim.embedding,
            };

            // Try to get source info, but still include the chunk even if source lookup fails
            let (source_title, source_url) = match self.get_knowledge_source(&chunk_sim.source_id).await {
                Ok(Some(source)) => (source.title, source.url),
                Ok(None) => {
                    println!("  Warning: No source found for source_id={}, using fallback", chunk_sim.source_id);
                    // Use source_id as fallback title, empty URL
                    (format!("Source {}", chunk_sim.source_id), String::new())
                }
                Err(e) => {
                    println!("  Error getting source for {}: {}, using fallback", chunk_sim.source_id, e);
                    (format!("Source {}", chunk_sim.source_id), String::new())
                }
            };

            results.push(KnowledgeSearchResult {
                chunk,
                source_title,
                source_url,
                similarity: chunk_sim.similarity,
            });
        }

        println!("Returning {} search results", results.len());
        Ok(results)
    }

    /// Link a knowledge source to a meeting
    pub async fn link_knowledge_to_meeting(
        &self,
        meeting_id: &str,
        source_id: &str,
        assigned_by: &str,
    ) -> Result<(), String> {
        let link = MeetingKnowledge {
            id: None,
            meeting_id: meeting_id.to_string(),
            source_id: source_id.to_string(),
            relevance_score: 1.0,
            assigned_by: assigned_by.to_string(),
        };

        self.db
            .create::<Option<MeetingKnowledge>>("meeting_knowledge")
            .content(link)
            .await
            .map_err(|e| format!("Failed to link knowledge: {}", e))?;

        Ok(())
    }

    /// Get knowledge sources linked to a meeting
    pub async fn get_meeting_knowledge(&self, meeting_id: &str) -> Result<Vec<KnowledgeSource>, String> {
        let meeting_id_owned = meeting_id.to_string();

        // Get linked source IDs
        let links: Vec<MeetingKnowledge> = self.db
            .query("SELECT * FROM meeting_knowledge WHERE meeting_id = $meeting_id")
            .bind(("meeting_id", meeting_id_owned))
            .await
            .map_err(|e| format!("Query failed: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract links: {}", e))?;

        // Get the actual sources
        let mut sources = Vec::new();
        for link in links {
            if let Ok(Some(source)) = self.get_knowledge_source(&link.source_id).await {
                sources.push(source);
            }
        }

        Ok(sources)
    }

    /// Get chunk count for a source
    pub async fn get_source_chunk_count(&self, source_id: &str) -> Result<usize, String> {
        let source_id_owned = source_id.to_string();

        let chunks: Vec<KnowledgeChunk> = self.db
            .query("SELECT * FROM knowledge_chunk WHERE source_id = $source_id")
            .bind(("source_id", source_id_owned))
            .await
            .map_err(|e| format!("Query failed: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to count chunks: {}", e))?;

        Ok(chunks.len())
    }

    // ==================== Graph-RAG Methods ====================

    /// Query using Graph-RAG: combines entity extraction, graph traversal, and vector search
    pub async fn graph_rag_query(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<GraphRAGContext, String> {
        let start = std::time::Instant::now();

        // 1. Extract entities from the query (sync, fast)
        let query_entities = self.entity_engine.extract(query)?;
        println!("[Graph-RAG] Query entities: {:?} ({:?})",
            query_entities.iter().map(|e| (&e.text, &e.label)).collect::<Vec<_>>(),
            start.elapsed());

        // 2. Parse temporal context from query (sync, fast)
        let temporal_context = self.parse_temporal_context(query);

        // 3. Run all async queries in PARALLEL for speed
        let (
            meetings_result,
            people_result,
            topics_result,
            actions_result,
            decisions_result,
            chunks_result,
        ) = tokio::join!(
            self.get_meetings_for_entities(&query_entities, &temporal_context),
            self.get_people_context(&query_entities),
            self.get_topic_context(&query_entities),
            self.get_open_actions(),
            self.get_recent_decisions(10),
            self.search_knowledge(query, limit, None),
        );

        // Unwrap results (use empty defaults on error to avoid blocking)
        let related_meetings = meetings_result.unwrap_or_default();
        let related_people = people_result.unwrap_or_default();
        let related_topics = topics_result.unwrap_or_default();
        let open_actions = actions_result.unwrap_or_default();
        let recent_decisions = decisions_result.unwrap_or_default();
        let similar_chunks = chunks_result.unwrap_or_default();

        println!("[Graph-RAG] Parallel queries completed in {:?}: {} meetings, {} people, {} topics, {} chunks",
            start.elapsed(),
            related_meetings.len(),
            related_people.len(),
            related_topics.len(),
            similar_chunks.len());

        Ok(GraphRAGContext {
            query_entities,
            related_meetings,
            related_people,
            related_topics,
            open_actions,
            recent_decisions,
            similar_chunks,
            temporal_context,
        })
    }

    /// Parse temporal references from query (e.g., "3 weeks ago", "last month")
    fn parse_temporal_context(&self, query: &str) -> Option<TemporalContext> {
        let query_lower = query.to_lowercase();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let day_ms: u64 = 24 * 60 * 60 * 1000;
        let week_ms: u64 = 7 * day_ms;

        // Parse common temporal patterns
        if let Some(caps) = regex::Regex::new(r"(\d+)\s*weeks?\s*ago")
            .ok()
            .and_then(|re| re.captures(&query_lower))
        {
            if let Some(weeks) = caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()) {
                let start = now - (weeks * week_ms) - week_ms; // Start of that week
                let end = now - (weeks * week_ms) + week_ms;   // End of that week
                return Some(TemporalContext {
                    time_reference: format!("{} weeks ago", weeks),
                    start_timestamp: Some(start),
                    end_timestamp: Some(end),
                });
            }
        }

        if let Some(caps) = regex::Regex::new(r"(\d+)\s*days?\s*ago")
            .ok()
            .and_then(|re| re.captures(&query_lower))
        {
            if let Some(days) = caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()) {
                let start = now - (days * day_ms) - day_ms;
                let end = now - (days * day_ms) + day_ms;
                return Some(TemporalContext {
                    time_reference: format!("{} days ago", days),
                    start_timestamp: Some(start),
                    end_timestamp: Some(end),
                });
            }
        }

        if query_lower.contains("last week") {
            return Some(TemporalContext {
                time_reference: "last week".to_string(),
                start_timestamp: Some(now - (2 * week_ms)),
                end_timestamp: Some(now - week_ms),
            });
        }

        if query_lower.contains("last month") {
            return Some(TemporalContext {
                time_reference: "last month".to_string(),
                start_timestamp: Some(now - (30 * day_ms)),
                end_timestamp: Some(now),
            });
        }

        if query_lower.contains("yesterday") {
            return Some(TemporalContext {
                time_reference: "yesterday".to_string(),
                start_timestamp: Some(now - (2 * day_ms)),
                end_timestamp: Some(now - day_ms),
            });
        }

        None
    }

    /// Get meetings related to extracted entities
    async fn get_meetings_for_entities(
        &self,
        entities: &[Entity],
        temporal: &Option<TemporalContext>,
    ) -> Result<Vec<MeetingContext>, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let day_ms: i64 = 24 * 60 * 60 * 1000;

        let mut meeting_contexts = Vec::new();

        // Get person names from entities (reserved for future entity-based filtering)
        let _person_names: Vec<String> = entities
            .iter()
            .filter(|e| e.label == "person")
            .map(|e| e.text.clone())
            .collect();

        // Get topic names from entities (reserved for future entity-based filtering)
        let _topic_names: Vec<String> = entities
            .iter()
            .filter(|e| e.label == "topic" || e.label == "project" || e.label == "product")
            .map(|e| e.text.clone())
            .collect();

        // Query for meetings involving these entities
        let base_query = if let Some(temp) = temporal {
            if let (Some(start), Some(end)) = (temp.start_timestamp, temp.end_timestamp) {
                format!(
                    "SELECT * FROM meeting WHERE start_time >= {} AND start_time <= {} ORDER BY start_time DESC LIMIT 20",
                    start, end
                )
            } else {
                "SELECT * FROM meeting ORDER BY start_time DESC LIMIT 20".to_string()
            }
        } else {
            "SELECT * FROM meeting ORDER BY start_time DESC LIMIT 20".to_string()
        };

        let meetings: Vec<Meeting> = self.db
            .query(&base_query)
            .await
            .map_err(|e| format!("Failed to query meetings: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract meetings: {}", e))?;

        for meeting in meetings {
            let meeting_id = meeting.id.as_ref().map(|t| t.to_string()).unwrap_or_default();
            let days_ago = (now as i64 - meeting.start_time as i64) / day_ms;

            // Get relevant segments from this meeting
            let segments: Vec<TranscriptSegment> = self.db
                .query("SELECT * FROM segment WHERE meeting_id = $meeting_id LIMIT 5")
                .bind(("meeting_id", meeting_id.clone()))
                .await
                .map_err(|e| format!("Failed to get segments: {}", e))?
                .take(0)
                .unwrap_or_default();

            meeting_contexts.push(MeetingContext {
                meeting,
                days_ago,
                relevant_segments: segments,
            });
        }

        Ok(meeting_contexts)
    }

    /// Get context about people mentioned in query
    async fn get_people_context(&self, entities: &[Entity]) -> Result<Vec<PersonContext>, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let day_ms: i64 = 24 * 60 * 60 * 1000;

        let mut people_contexts = Vec::new();

        // Get person names from entities
        let person_names: Vec<&str> = entities
            .iter()
            .filter(|e| e.label == "person")
            .map(|e| e.text.as_str())
            .collect();

        for name in person_names {
            // Get person record
            let people: Vec<Person> = self.db
                .query("SELECT * FROM person WHERE name = $name")
                .bind(("name", name.to_string()))
                .await
                .map_err(|e| format!("Failed to query person: {}", e))?
                .take(0)
                .unwrap_or_default();

            if let Some(person) = people.into_iter().next() {
                let last_seen_days_ago = (now as i64 - person.last_seen as i64) / day_ms;

                // Get topics this person has discussed
                let topics: Vec<serde_json::Value> = self.db
                    .query(r#"
                        SELECT target_entity FROM entity_relation
                        WHERE source_entity = $name AND source_type = 'person'
                        AND (target_type = 'topic' OR target_type = 'project')
                        LIMIT 5
                    "#)
                    .bind(("name", name.to_string()))
                    .await
                    .map_err(|e| format!("Failed to query topics: {}", e))?
                    .take(0)
                    .unwrap_or_default();

                let recent_topics: Vec<String> = topics
                    .iter()
                    .filter_map(|v| v.get("target_entity").and_then(|t| t.as_str()).map(|s| s.to_string()))
                    .collect();

                people_contexts.push(PersonContext {
                    name: person.name,
                    last_seen_days_ago,
                    meeting_count: 0, // Would need a separate query
                    recent_topics,
                });
            }
        }

        Ok(people_contexts)
    }

    /// Get context about topics mentioned in query
    async fn get_topic_context(&self, entities: &[Entity]) -> Result<Vec<TopicContext>, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let day_ms: i64 = 24 * 60 * 60 * 1000;

        let mut topic_contexts = Vec::new();

        // Get topic/project names from entities
        let topic_names: Vec<&str> = entities
            .iter()
            .filter(|e| e.label == "topic" || e.label == "project" || e.label == "product")
            .map(|e| e.text.as_str())
            .collect();

        for name in topic_names {
            // Get topic record
            let topics: Vec<serde_json::Value> = self.db
                .query("SELECT * FROM topic WHERE name = $name")
                .bind(("name", name.to_string()))
                .await
                .map_err(|e| format!("Failed to query topic: {}", e))?
                .take(0)
                .unwrap_or_default();

            if let Some(topic) = topics.into_iter().next() {
                let last_mentioned = topic.get("last_mentioned").and_then(|v| v.as_u64()).unwrap_or(0);
                let mention_count = topic.get("mention_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let last_mentioned_days_ago = (now as i64 - last_mentioned as i64) / day_ms;

                // Get people who discussed this topic
                let people: Vec<serde_json::Value> = self.db
                    .query(r#"
                        SELECT source_entity FROM entity_relation
                        WHERE target_entity = $name AND source_type = 'person'
                        LIMIT 5
                    "#)
                    .bind(("name", name.to_string()))
                    .await
                    .map_err(|e| format!("Failed to query people: {}", e))?
                    .take(0)
                    .unwrap_or_default();

                let related_people: Vec<String> = people
                    .iter()
                    .filter_map(|v| v.get("source_entity").and_then(|t| t.as_str()).map(|s| s.to_string()))
                    .collect();

                topic_contexts.push(TopicContext {
                    name: name.to_string(),
                    last_mentioned_days_ago,
                    mention_count,
                    related_people,
                });
            }
        }

        Ok(topic_contexts)
    }

    /// Get entity relationships for Graph-RAG context
    pub async fn get_entity_relationships(
        &self,
        entity_name: &str,
        limit: usize,
    ) -> Result<Vec<Relationship>, String> {
        #[derive(Deserialize)]
        struct StoredRelation {
            source_entity: String,
            source_type: String,
            relation: String,
            target_entity: String,
            target_type: String,
            confidence: f32,
        }

        let relations: Vec<StoredRelation> = self.db
            .query(r#"
                SELECT * FROM entity_relation
                WHERE source_entity = $name OR target_entity = $name
                ORDER BY confidence DESC
                LIMIT $limit
            "#)
            .bind(("name", entity_name.to_string()))
            .bind(("limit", limit))
            .await
            .map_err(|e| format!("Failed to query relations: {}", e))?
            .take(0)
            .unwrap_or_default();

        Ok(relations.into_iter().map(|r| Relationship {
            source: r.source_entity,
            source_type: r.source_type,
            relation: r.relation,
            target: r.target_entity,
            target_type: r.target_type,
            confidence: r.confidence,
        }).collect())
    }

    // ==================== Meeting Query Methods ====================

    /// Get all meetings, ordered by start time descending
    pub async fn get_meetings(&self, limit: Option<usize>) -> Result<Vec<Meeting>, String> {
        let query_limit = limit.unwrap_or(50);

        let meetings: Vec<Meeting> = self.db
            .query("SELECT * FROM meeting ORDER BY start_time DESC LIMIT $limit")
            .bind(("limit", query_limit))
            .await
            .map_err(|e| format!("Failed to query meetings: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract meetings: {}", e))?;

        Ok(meetings)
    }

    /// Get a single meeting by ID
    pub async fn get_meeting(&self, meeting_id: &str) -> Result<Option<Meeting>, String> {
        // Extract just the ID part if full Thing string is passed
        let id_part = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };

        let meeting: Option<Meeting> = self.db
            .select(("meeting", id_part))
            .await
            .map_err(|e| format!("Failed to get meeting: {}", e))?;

        Ok(meeting)
    }

    /// Get all transcript segments for a meeting
    pub async fn get_meeting_segments(&self, meeting_id: &str) -> Result<Vec<TranscriptSegment>, String> {
        let meeting_id_owned = meeting_id.to_string();

        let segments: Vec<TranscriptSegment> = self.db
            .query("SELECT * FROM segment WHERE meeting_id = $meeting_id ORDER BY start_ms ASC")
            .bind(("meeting_id", meeting_id_owned))
            .await
            .map_err(|e| format!("Failed to query segments: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract segments: {}", e))?;

        Ok(segments)
    }

    /// Get action items for a specific meeting
    pub async fn get_meeting_action_items(&self, meeting_id: &str) -> Result<Vec<ActionItem>, String> {
        // Normalize meeting_id - strip prefix if present
        let normalized_id = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };

        println!("[KB] Getting action items for meeting: {} (normalized: {})", meeting_id, normalized_id);

        let actions: Vec<ActionItem> = self.db
            .query("SELECT * FROM action_item WHERE meeting_id = $meeting_id ORDER BY created_at DESC")
            .bind(("meeting_id", normalized_id.to_string()))
            .await
            .map_err(|e| format!("Failed to query action items: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract action items: {}", e))?;

        println!("[KB] Found {} action items", actions.len());
        Ok(actions)
    }

    /// Get decisions for a specific meeting
    pub async fn get_meeting_decisions(&self, meeting_id: &str) -> Result<Vec<Decision>, String> {
        // Normalize meeting_id - strip prefix if present
        let normalized_id = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };

        println!("[KB] Getting decisions for meeting: {} (normalized: {})", meeting_id, normalized_id);

        let decisions: Vec<Decision> = self.db
            .query("SELECT * FROM decision WHERE meeting_id = $meeting_id ORDER BY created_at DESC")
            .bind(("meeting_id", normalized_id.to_string()))
            .await
            .map_err(|e| format!("Failed to query decisions: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract decisions: {}", e))?;

        println!("[KB] Found {} decisions", decisions.len());
        Ok(decisions)
    }

    /// Get ALL action items across all meetings with meeting title
    pub async fn get_all_action_items(&self, limit: usize) -> Result<Vec<serde_json::Value>, String> {
        let results: Vec<serde_json::Value> = self.db
            .query(r#"
                SELECT
                    id,
                    text,
                    assignee,
                    deadline,
                    status,
                    meeting_id,
                    (SELECT title FROM meeting WHERE id = $parent.meeting_id)[0].title AS meeting_title,
                    created_at
                FROM action_item
                ORDER BY created_at DESC
                LIMIT $limit
            "#)
            .bind(("limit", limit))
            .await
            .map_err(|e| format!("Failed to query all action items: {}", e))?
            .take(0)
            .unwrap_or_default();

        Ok(results)
    }

    /// Get ALL decisions across all meetings with meeting title
    pub async fn get_all_decisions(&self, limit: usize) -> Result<Vec<serde_json::Value>, String> {
        let results: Vec<serde_json::Value> = self.db
            .query(r#"
                SELECT
                    id,
                    text,
                    meeting_id,
                    (SELECT title FROM meeting WHERE id = $parent.meeting_id)[0].title AS meeting_title,
                    created_at
                FROM decision
                ORDER BY created_at DESC
                LIMIT $limit
            "#)
            .bind(("limit", limit))
            .await
            .map_err(|e| format!("Failed to query all decisions: {}", e))?
            .take(0)
            .unwrap_or_default();

        Ok(results)
    }

    /// Get global knowledge base statistics
    pub async fn get_global_stats(&self) -> Result<serde_json::Value, String> {
        // Count total segments
        let segment_counts: Vec<serde_json::Value> = self.db
            .query("SELECT count() AS count FROM transcript_segment GROUP ALL")
            .await
            .map_err(|e| format!("Failed to count segments: {}", e))?
            .take(0)
            .unwrap_or_default();

        let total_segments = segment_counts
            .first()
            .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
            .unwrap_or(0);

        // Get entity counts by label
        let entity_counts: Vec<serde_json::Value> = self.db
            .query(r#"
                SELECT label, count() AS count
                FROM entity
                GROUP BY label
                ORDER BY count DESC
                LIMIT 10
            "#)
            .await
            .map_err(|e| format!("Failed to count entities: {}", e))?
            .take(0)
            .unwrap_or_default();

        Ok(serde_json::json!({
            "total_segments": total_segments,
            "entity_counts": entity_counts
        }))
    }

    /// Get topics discussed in a meeting
    pub async fn get_meeting_topics(&self, meeting_id: &str) -> Result<Vec<Topic>, String> {
        // Extract just the ID part for use with type::thing()
        let meeting_id_part = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };
        let meeting_id_owned = meeting_id_part.to_string();

        // Query topics that are linked to this meeting via discussed_in relation
        let topics: Vec<Topic> = self.db
            .query(r#"
                SELECT * FROM topic WHERE id IN (
                    SELECT in FROM discussed_in WHERE out = type::thing('meeting', $meeting_id)
                )
            "#)
            .bind(("meeting_id", meeting_id_owned))
            .await
            .map_err(|e| format!("Failed to query topics: {}", e))?
            .take(0)
            .unwrap_or_default();

        Ok(topics)
    }

    /// Get people mentioned in a meeting
    pub async fn get_meeting_people(&self, meeting_id: &str) -> Result<Vec<Person>, String> {
        // Extract just the ID part for use with type::thing()
        let meeting_id_part = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };
        let meeting_id_owned = meeting_id_part.to_string();

        // Query people that are linked to this meeting via mentioned_in relation
        let people: Vec<Person> = self.db
            .query(r#"
                SELECT * FROM person WHERE id IN (
                    SELECT in FROM mentioned_in WHERE out = type::thing('meeting', $meeting_id)
                )
            "#)
            .bind(("meeting_id", meeting_id_owned))
            .await
            .map_err(|e| format!("Failed to query people: {}", e))?
            .take(0)
            .unwrap_or_default();

        Ok(people)
    }

    /// Update action item status
    pub async fn update_action_item_status(&self, action_id: &str, status: &str) -> Result<(), String> {
        let id_part = if action_id.starts_with("action_item:") {
            action_id.strip_prefix("action_item:").unwrap_or(action_id)
        } else {
            action_id
        };

        self.db
            .query("UPDATE type::thing('action_item', $id) SET status = $status")
            .bind(("id", id_part.to_string()))
            .bind(("status", status.to_string()))
            .await
            .map_err(|e| format!("Failed to update action item: {}", e))?;

        Ok(())
    }

    /// Add an action item to a meeting
    pub async fn add_action_item(
        &self,
        meeting_id: &str,
        text: &str,
        assignee: Option<&str>,
        deadline: Option<&str>,
    ) -> Result<String, String> {
        // Normalize meeting_id - strip prefix if present
        let normalized_id = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };

        println!("[KB] Adding action item for meeting: {} (normalized: {})", meeting_id, normalized_id);

        let action: Option<ActionItem> = self.db
            .query("CREATE action_item SET meeting_id = $meeting_id, text = $text, assignee = $assignee, deadline = $deadline, status = 'open', created_at = time::now()")
            .bind(("meeting_id", normalized_id.to_string()))
            .bind(("text", text.to_string()))
            .bind(("assignee", assignee.map(|s| s.to_string())))
            .bind(("deadline", deadline.map(|s| s.to_string())))
            .await
            .map_err(|e| format!("Failed to create action item: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract action item: {}", e))?;

        let id = action.and_then(|a| a.id).map(|id| id.to_string()).unwrap_or_default();
        println!("[KB] Created action item: {}", id);
        Ok(id)
    }

    /// Add a decision to a meeting
    pub async fn add_decision(&self, meeting_id: &str, text: &str) -> Result<String, String> {
        // Normalize meeting_id - strip prefix if present
        let normalized_id = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };

        println!("[KB] Adding decision for meeting: {} (normalized: {})", meeting_id, normalized_id);

        let decision: Option<Decision> = self.db
            .query("CREATE decision SET meeting_id = $meeting_id, text = $text, created_at = time::now()")
            .bind(("meeting_id", normalized_id.to_string()))
            .bind(("text", text.to_string()))
            .await
            .map_err(|e| format!("Failed to create decision: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract decision: {}", e))?;

        let id = decision.and_then(|d| d.id).map(|id| id.to_string()).unwrap_or_default();
        println!("[KB] Created decision: {}", id);
        Ok(id)
    }

    /// Update meeting summary
    pub async fn update_meeting_summary(&self, meeting_id: &str, summary: &str) -> Result<(), String> {
        // Normalize meeting_id - strip prefix if present
        let id_part = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };

        println!("[KB] Updating summary for meeting: {} (id_part: {})", meeting_id, id_part);

        self.db
            .query("UPDATE type::thing('meeting', $id) SET summary = $summary")
            .bind(("id", id_part.to_string()))
            .bind(("summary", summary.to_string()))
            .await
            .map_err(|e| format!("Failed to update meeting summary: {}", e))?;

        Ok(())
    }

    /// Get meeting statistics
    pub async fn get_meeting_stats(&self, meeting_id: &str) -> Result<MeetingStats, String> {
        let segments = self.get_meeting_segments(meeting_id).await?;
        let actions = self.get_meeting_action_items(meeting_id).await?;
        let decisions = self.get_meeting_decisions(meeting_id).await?;
        let topics = self.get_meeting_topics(meeting_id).await?;
        let people = self.get_meeting_people(meeting_id).await?;

        // Calculate duration from segments
        let duration_ms = if !segments.is_empty() {
            segments.last().map(|s| s.end_ms).unwrap_or(0) -
            segments.first().map(|s| s.start_ms).unwrap_or(0)
        } else {
            0
        };

        // Count words
        let total_words: usize = segments.iter()
            .map(|s| s.text.split_whitespace().count())
            .sum();

        Ok(MeetingStats {
            segment_count: segments.len(),
            action_count: actions.len(),
            decision_count: decisions.len(),
            topic_count: topics.len(),
            people_count: people.len(),
            duration_ms,
            total_words,
        })
    }

    /// Delete a meeting and all associated data
    pub async fn delete_meeting(&self, meeting_id: &str) -> Result<(), String> {
        // Extract just the ID part if full Thing string is passed
        let id_part = if meeting_id.starts_with("meeting:") {
            meeting_id.strip_prefix("meeting:").unwrap_or(meeting_id)
        } else {
            meeting_id
        };

        let full_meeting_id = format!("meeting:{}", id_part);

        println!("[KB Delete Meeting] Deleting meeting: id_part={}, full={}", id_part, full_meeting_id);

        // Delete all segments for this meeting
        self.db
            .query("DELETE FROM segment WHERE meeting_id = $meeting_id OR meeting_id = $full_id")
            .bind(("meeting_id", id_part.to_string()))
            .bind(("full_id", full_meeting_id.clone()))
            .await
            .map_err(|e| format!("Failed to delete segments: {}", e))?;

        // Delete all action items for this meeting
        self.db
            .query("DELETE FROM action_item WHERE meeting_id = $meeting_id OR meeting_id = $full_id")
            .bind(("meeting_id", id_part.to_string()))
            .bind(("full_id", full_meeting_id.clone()))
            .await
            .map_err(|e| format!("Failed to delete action items: {}", e))?;

        // Delete all decisions for this meeting
        self.db
            .query("DELETE FROM decision WHERE meeting_id = $meeting_id OR meeting_id = $full_id")
            .bind(("meeting_id", id_part.to_string()))
            .bind(("full_id", full_meeting_id.clone()))
            .await
            .map_err(|e| format!("Failed to delete decisions: {}", e))?;

        // Delete entity relations for this meeting
        self.db
            .query("DELETE FROM entity_relation WHERE meeting_id = $meeting_id OR meeting_id = $full_id")
            .bind(("meeting_id", id_part.to_string()))
            .bind(("full_id", full_meeting_id.clone()))
            .await
            .map_err(|e| format!("Failed to delete entity relations: {}", e))?;

        // Delete meeting-knowledge links
        self.db
            .query("DELETE FROM meeting_knowledge WHERE meeting_id = $meeting_id OR meeting_id = $full_id")
            .bind(("meeting_id", id_part.to_string()))
            .bind(("full_id", full_meeting_id.clone()))
            .await
            .map_err(|e| format!("Failed to delete meeting links: {}", e))?;

        // Delete graph relations (mentioned_in, discussed_in edges pointing to this meeting)
        self.db
            .query("DELETE FROM mentioned_in WHERE out = type::thing('meeting', $id)")
            .bind(("id", id_part.to_string()))
            .await
            .ok(); // Ignore errors for graph relations

        self.db
            .query("DELETE FROM discussed_in WHERE out = type::thing('meeting', $id)")
            .bind(("id", id_part.to_string()))
            .await
            .ok(); // Ignore errors for graph relations

        // Finally, delete the meeting itself
        self.db
            .delete::<Option<Meeting>>(("meeting", id_part))
            .await
            .map_err(|e| format!("Failed to delete meeting: {}", e))?;

        println!("[KB Delete Meeting] Meeting deleted successfully: {}", meeting_id);
        Ok(())
    }

    /// Clean up orphaned chunks (chunks whose source no longer exists)
    pub async fn cleanup_orphaned_chunks(&self) -> Result<usize, String> {
        // Get all unique source_ids from chunks using GROUP BY (SurrealDB syntax)
        let chunk_source_ids: Vec<serde_json::Value> = self.db
            .query("SELECT source_id FROM knowledge_chunk GROUP BY source_id")
            .await
            .map_err(|e| format!("Failed to get chunk source_ids: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract source_ids: {}", e))?;

        println!("[KB Cleanup] Found {} unique source_ids in chunks", chunk_source_ids.len());

        let mut deleted_count = 0;

        for row in chunk_source_ids {
            if let Some(source_id) = row.get("source_id").and_then(|v| v.as_str()) {
                // Check if source exists
                if self.get_knowledge_source(source_id).await?.is_none() {
                    println!("[KB Cleanup] Orphaned source_id: {}", source_id);

                    // Delete orphaned chunks
                    self.db
                        .query("DELETE FROM knowledge_chunk WHERE source_id = $source_id")
                        .bind(("source_id", source_id.to_string()))
                        .await
                        .map_err(|e| format!("Failed to delete orphaned chunks: {}", e))?;

                    deleted_count += 1;
                }
            }
        }

        println!("[KB Cleanup] Cleaned up {} orphaned source_id groups", deleted_count);
        Ok(deleted_count)
    }

    /// Relabel speakers in a meeting based on diarization results
    /// Updates "Guest" segments to have proper speaker labels (Speaker 1, Speaker 2, etc.)
    pub async fn relabel_speakers(
        &self,
        meeting_id: &str,
        diarization: &[(u64, u64, i32, String)],  // (start_ms, end_ms, speaker_id, speaker_label)
    ) -> Result<usize, String> {
        // Get all segments for this meeting that have "Guest" as speaker
        let meeting_id_owned = meeting_id.to_string();
        let segments: Vec<TranscriptSegment> = self.db
            .query("SELECT * FROM segment WHERE meeting_id = $meeting_id AND speaker = 'Guest'")
            .bind(("meeting_id", meeting_id_owned))
            .await
            .map_err(|e| format!("Failed to get segments: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract segments: {}", e))?;

        let mut relabeled_count = 0;

        for segment in segments {
            let segment_mid = (segment.start_ms + segment.end_ms) / 2;

            // Find overlapping diarization segment
            if let Some((_, _, _, speaker_label)) = diarization.iter().find(|(start, end, _, _)| {
                segment_mid >= *start && segment_mid <= *end
            }) {
                // Update the speaker label
                if let Some(ref id) = segment.id {
                    self.db
                        .query("UPDATE $id SET speaker = $speaker")
                        .bind(("id", id.clone()))
                        .bind(("speaker", speaker_label.clone()))
                        .await
                        .map_err(|e| format!("Failed to update segment speaker: {}", e))?;

                    relabeled_count += 1;
                }
            }
        }

        println!("[KB] Relabeled {} segments with diarization results", relabeled_count);
        Ok(relabeled_count)
    }

    /// Relabel ALL speakers in a meeting based on diarization results
    /// Updates ALL segments (both "You" and "Guest") with proper speaker labels from diarization
    pub async fn relabel_all_speakers(
        &self,
        meeting_id: &str,
        diarization: &[(u64, u64, i32, String)],  // (start_ms, end_ms, speaker_id, speaker_label)
    ) -> Result<usize, String> {
        if diarization.is_empty() {
            println!("[KB] No diarization results to apply");
            return Ok(0);
        }

        // Get ALL segments for this meeting (regardless of current speaker label)
        let meeting_id_owned = meeting_id.to_string();
        let segments: Vec<TranscriptSegment> = self.db
            .query("SELECT * FROM segment WHERE meeting_id = $meeting_id ORDER BY start_ms")
            .bind(("meeting_id", meeting_id_owned))
            .await
            .map_err(|e| format!("Failed to get segments: {}", e))?
            .take(0)
            .map_err(|e| format!("Failed to extract segments: {}", e))?;

        println!("[KB] Found {} segments to potentially relabel", segments.len());

        let mut relabeled_count = 0;

        for segment in segments {
            let segment_mid = (segment.start_ms + segment.end_ms) / 2;

            // Find overlapping diarization segment by timestamp
            // Use a tolerance window since ASR and diarization timestamps may not align perfectly
            if let Some((_, _, _, speaker_label)) = diarization.iter().find(|(start, end, _, _)| {
                // Check if segment midpoint falls within diarization window
                // Or if there's any overlap
                let overlap = segment.start_ms <= *end && segment.end_ms >= *start;
                let midpoint_in_range = segment_mid >= *start && segment_mid <= *end;
                overlap || midpoint_in_range
            }) {
                // Only update if the label is different
                if segment.speaker != *speaker_label {
                    if let Some(ref id) = segment.id {
                        self.db
                            .query("UPDATE $id SET speaker = $speaker")
                            .bind(("id", id.clone()))
                            .bind(("speaker", speaker_label.clone()))
                            .await
                            .map_err(|e| format!("Failed to update segment speaker: {}", e))?;

                        relabeled_count += 1;
                    }
                }
            }
        }

        println!("[KB] Relabeled {} segments with diarization results", relabeled_count);
        Ok(relabeled_count)
    }
}
