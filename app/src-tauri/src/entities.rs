use gliner::model::params::Parameters;
use gliner::model::input::text::TextInput;
use gliner::model::input::relation::schema::RelationSchema;
use gliner::model::pipeline::token::TokenPipeline;
use gliner::model::pipeline::relation::RelationPipeline;
use gliner::model::output::decoded::SpanOutput;
use gliner::model::output::relation::RelationOutput;
use orp::model::Model;
use orp::params::RuntimeParameters;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Entity types we extract from meeting transcripts
/// Expanded labels for technical interviews and general conversations
pub const ENTITY_LABELS: &[&str] = &[
    // People & Organizations
    "person",
    "organization",
    "company",
    // Work & Projects
    "project",
    "product",
    "technology",
    "programming_language",
    // Technical concepts (for interviews)
    "algorithm",
    "data_structure",
    "concept",
    "problem",
    // Meeting-specific
    "action_item",
    "deadline",
    "decision",
    "topic",
    "question",
    // Measurements
    "metric",
    "number",
    "time",
    // Location
    "location",
];

/// An extracted entity from text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub text: String,
    pub label: String,
    pub sequence: usize,
    pub confidence: f32,
}

/// A relationship between two entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub source: String,        // Source entity text
    pub source_type: String,   // Source entity type
    pub relation: String,      // Relationship type
    pub target: String,        // Target entity text
    pub target_type: String,   // Target entity type
    pub confidence: f32,
}

/// Relationship types we extract
pub const RELATIONSHIP_TYPES: &[&str] = &[
    "discussed",      // Person discussed Topic
    "assigned_to",    // ActionItem assigned_to Person
    "decided",        // Person decided Decision
    "mentioned",      // Person mentioned Person/Topic
    "works_on",       // Person works_on Project
    "reported",       // Person reported Metric
    "deadline_for",   // Deadline deadline_for ActionItem
    "related_to",     // Topic related_to Topic
];

/// Result of entity extraction on a piece of text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub text: String,
    pub entities: Vec<Entity>,
    pub relationships: Vec<Relationship>,
    pub timestamp_ms: u64,
    pub source: String,
}

/// GLiNER Multitask-based entity and relationship extraction engine
/// Uses the gliner-multitask-large-v0.5 model for both NER and RE
pub struct EntityEngine {
    model: Model,
    params: Parameters,
    tokenizer_path: String,
}

/// Build relationship schema for meeting-related relations
fn build_relation_schema() -> RelationSchema {
    let mut schema = RelationSchema::new();

    // Person-centric relations
    schema.push_with_allowed_labels("discussed", &["person"], &["topic", "project", "product", "concept", "problem", "algorithm"]);
    schema.push_with_allowed_labels("assigned_to", &["action_item"], &["person"]);
    schema.push_with_allowed_labels("decided", &["person"], &["decision"]);
    schema.push_with_allowed_labels("mentioned", &["person"], &["person", "organization", "company", "topic", "project", "technology"]);
    schema.push_with_allowed_labels("works_on", &["person"], &["project", "product", "technology"]);
    schema.push_with_allowed_labels("works_at", &["person"], &["company", "organization"]);
    schema.push_with_allowed_labels("reported", &["person"], &["metric", "number"]);
    schema.push_with_allowed_labels("belongs_to", &["person"], &["organization", "company"]);
    schema.push_with_allowed_labels("asked", &["person"], &["question"]);

    // Technical relations (for interviews)
    schema.push_with_allowed_labels("uses", &["algorithm", "data_structure", "concept"], &["programming_language", "technology", "data_structure"]);
    schema.push_with_allowed_labels("solves", &["algorithm", "concept"], &["problem"]);
    schema.push_with_allowed_labels("implements", &["technology", "product"], &["algorithm", "concept"]);
    schema.push_with_allowed_labels("requires", &["problem", "question"], &["algorithm", "data_structure", "concept"]);

    // Non-person relations
    schema.push_with_allowed_labels("deadline_for", &["deadline", "time"], &["action_item", "project"]);
    schema.push_with_allowed_labels("related_to", &["topic", "project", "concept"], &["topic", "project", "product", "technology", "concept"]);
    schema.push_with_allowed_labels("located_in", &["organization", "company", "person"], &["location"]);

    schema
}

impl EntityEngine {
    /// Create a new entity extraction engine
    ///
    /// # Arguments
    /// * `models_dir` - Directory containing gliner tokenizer.json and model.onnx
    pub fn new(models_dir: &PathBuf) -> Result<Self, String> {
        let tokenizer_path = models_dir.join("gliner-tokenizer.json");
        let model_path = models_dir.join("gliner-model.onnx");

        if !tokenizer_path.exists() {
            return Err(format!("GLiNER tokenizer not found at {:?}", tokenizer_path));
        }
        if !model_path.exists() {
            return Err(format!("GLiNER model not found at {:?}", model_path));
        }

        let tokenizer_str = tokenizer_path.to_str()
            .ok_or("Invalid tokenizer path")?
            .to_string();
        let model_str = model_path.to_str()
            .ok_or("Invalid model path")?;

        let model = Model::new(model_str, RuntimeParameters::default())
            .map_err(|e| format!("Failed to load GLiNER model: {}", e))?;

        println!("GLiNER multitask entity engine initialized");
        Ok(Self {
            model,
            params: Parameters::default(),
            tokenizer_path: tokenizer_str,
        })
    }

    /// Extract entities from a single text
    pub fn extract(&self, text: &str) -> Result<Vec<Entity>, String> {
        if text.trim().is_empty() {
            return Ok(vec![]);
        }

        let input = TextInput::from_str(&[text], ENTITY_LABELS)
            .map_err(|e| format!("Failed to create input: {}", e))?;

        let token_pipeline = TokenPipeline::new(&self.tokenizer_path)
            .map_err(|e| format!("Failed to create token pipeline: {}", e))?;

        let output: SpanOutput = self.model.inference(input, &token_pipeline, &self.params)
            .map_err(|e| format!("Inference failed: {}", e))?;

        let mut entities = Vec::new();

        // Process first (and only) text result - output.spans is Vec<Vec<Span>>
        if let Some(text_spans) = output.spans.into_iter().next() {
            for span in text_spans {
                entities.push(Entity {
                    text: span.text().to_string(),
                    label: span.class().to_string(),
                    sequence: span.sequence(),
                    confidence: span.probability(),
                });
            }
        }

        // Sort by confidence descending
        entities.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        Ok(entities)
    }

    /// Extract entities AND relationships from a single text
    /// This uses the GLiNER multitask model for both NER and RE
    pub fn extract_with_relations(&self, text: &str) -> Result<(Vec<Entity>, Vec<Relationship>), String> {
        if text.trim().is_empty() {
            return Ok((vec![], vec![]));
        }

        let input = TextInput::from_str(&[text], ENTITY_LABELS)
            .map_err(|e| format!("Failed to create input: {}", e))?;

        // First pass: Entity extraction with TokenPipeline
        let token_pipeline = TokenPipeline::new(&self.tokenizer_path)
            .map_err(|e| format!("Failed to create token pipeline: {}", e))?;

        let entity_output: SpanOutput = self.model.inference(input, &token_pipeline, &self.params)
            .map_err(|e| format!("Entity inference failed: {}", e))?;

        // Collect entities
        let mut entities = Vec::new();
        if let Some(text_spans) = entity_output.spans.iter().next() {
            for span in text_spans {
                entities.push(Entity {
                    text: span.text().to_string(),
                    label: span.class().to_string(),
                    sequence: span.sequence(),
                    confidence: span.probability(),
                });
            }
        }

        // Sort entities by confidence
        entities.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        // Skip relationship extraction if no entities found (would fail anyway)
        if entities.is_empty() {
            return Ok((entities, vec![]));
        }

        // Second pass: Relationship extraction using entity output (optional - don't fail if this errors)
        let relationships = match self.try_extract_relationships(entity_output, &entities) {
            Ok(rels) => rels,
            Err(_) => vec![], // Silently skip relationship extraction on error
        };

        Ok((entities, relationships))
    }

    /// Try to extract relationships from entity output (helper that can fail gracefully)
    fn try_extract_relationships(&self, entity_output: SpanOutput, entities: &[Entity]) -> Result<Vec<Relationship>, String> {
        let relation_schema = build_relation_schema();
        let relation_pipeline = RelationPipeline::default(&self.tokenizer_path, &relation_schema)
            .map_err(|e| format!("Failed to create relation pipeline: {}", e))?;

        let relation_output: RelationOutput = self.model.inference(entity_output, &relation_pipeline, &self.params)
            .map_err(|e| format!("Relation inference failed: {}", e))?;

        let mut relationships = Vec::new();
        for seq_relations in relation_output.relations {
            for rel in seq_relations {
                let source_type = entities.iter()
                    .find(|e| e.text == rel.subject())
                    .map(|e| e.label.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                let target_type = entities.iter()
                    .find(|e| e.text == rel.object())
                    .map(|e| e.label.clone())
                    .unwrap_or_else(|| "unknown".to_string());

                relationships.push(Relationship {
                    source: rel.subject().to_string(),
                    source_type,
                    relation: rel.class().to_string(),
                    target: rel.object().to_string(),
                    target_type,
                    confidence: rel.probability(),
                });
            }
        }

        relationships.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        Ok(relationships)
    }

    /// Extract entities from multiple texts (batched for efficiency)
    pub fn extract_batch(&self, texts: &[&str]) -> Result<Vec<Vec<Entity>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let input = TextInput::from_str(texts, ENTITY_LABELS)
            .map_err(|e| format!("Failed to create batch input: {}", e))?;

        let token_pipeline = TokenPipeline::new(&self.tokenizer_path)
            .map_err(|e| format!("Failed to create token pipeline: {}", e))?;

        let output: SpanOutput = self.model.inference(input, &token_pipeline, &self.params)
            .map_err(|e| format!("Batch inference failed: {}", e))?;

        let mut results = Vec::new();

        for text_spans in output.spans {
            let mut entities: Vec<Entity> = text_spans
                .into_iter()
                .map(|span| Entity {
                    text: span.text().to_string(),
                    label: span.class().to_string(),
                    sequence: span.sequence(),
                    confidence: span.probability(),
                })
                .collect();

            entities.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
            results.push(entities);
        }

        Ok(results)
    }

    /// Extract entities AND relationships with metadata
    pub fn extract_with_metadata(
        &self,
        text: &str,
        timestamp_ms: u64,
        source: &str,
    ) -> Result<ExtractionResult, String> {
        let (entities, relationships) = self.extract_with_relations(text)?;

        Ok(ExtractionResult {
            text: text.to_string(),
            entities,
            relationships,
            timestamp_ms,
            source: source.to_string(),
        })
    }
}

/// Filter entities by minimum confidence threshold
pub fn filter_by_confidence(entities: Vec<Entity>, min_confidence: f32) -> Vec<Entity> {
    entities
        .into_iter()
        .filter(|e| e.confidence >= min_confidence)
        .collect()
}

/// Group entities by their label
pub fn group_by_label(entities: Vec<Entity>) -> std::collections::HashMap<String, Vec<Entity>> {
    let mut groups: std::collections::HashMap<String, Vec<Entity>> = std::collections::HashMap::new();

    for entity in entities {
        groups
            .entry(entity.label.clone())
            .or_default()
            .push(entity);
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_serialization() {
        let entity = Entity {
            text: "John Smith".to_string(),
            label: "person".to_string(),
            sequence: 0,
            confidence: 0.95,
        };

        let json = serde_json::to_string(&entity).unwrap();
        let deserialized: Entity = serde_json::from_str(&json).unwrap();

        assert_eq!(entity.text, deserialized.text);
        assert_eq!(entity.label, deserialized.label);
    }

    #[test]
    fn test_group_by_label() {
        let entities = vec![
            Entity { text: "Alice".to_string(), label: "person".to_string(), sequence: 0, confidence: 0.9 },
            Entity { text: "Bob".to_string(), label: "person".to_string(), sequence: 0, confidence: 0.85 },
            Entity { text: "Acme Corp".to_string(), label: "organization".to_string(), sequence: 0, confidence: 0.8 },
        ];

        let groups = group_by_label(entities);

        assert_eq!(groups.get("person").unwrap().len(), 2);
        assert_eq!(groups.get("organization").unwrap().len(), 1);
    }

    #[test]
    fn test_filter_by_confidence() {
        let entities = vec![
            Entity { text: "High".to_string(), label: "test".to_string(), sequence: 0, confidence: 0.9 },
            Entity { text: "Low".to_string(), label: "test".to_string(), sequence: 0, confidence: 0.3 },
        ];

        let filtered = filter_by_confidence(entities, 0.5);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].text, "High");
    }
}
