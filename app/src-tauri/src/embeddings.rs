use ndarray::{Array1, Array2, Axis};
use ort::session::{builder::GraphOptimizationLevel, Session};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokenizers::Tokenizer;

/// Embedding dimension for EmbeddingGemma-300M (768-dim)
pub const EMBEDDING_DIM: usize = 768;

/// Result of embedding a text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResult {
    pub text: String,
    pub embedding: Vec<f32>,
}

/// Text embedding engine using EmbeddingGemma-300M
pub struct EmbeddingEngine {
    session: Session,
    tokenizer: Tokenizer,
}

impl EmbeddingEngine {
    /// Create a new embedding engine
    ///
    /// # Arguments
    /// * `models_dir` - Directory containing embedding-model.onnx and embedding-tokenizer.json
    pub fn new(models_dir: &PathBuf) -> Result<Self, String> {
        // Use original filename - .onnx file references .onnx_data by name internally
        let model_path = models_dir.join("model_q4.onnx");
        let tokenizer_path = models_dir.join("embedding-tokenizer.json");

        if !model_path.exists() {
            return Err(format!("Embedding model not found at {:?}", model_path));
        }
        if !tokenizer_path.exists() {
            return Err(format!("Embedding tokenizer not found at {:?}", tokenizer_path));
        }

        // Load ONNX model
        let session = Session::builder()
            .map_err(|e| format!("Failed to create session builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| format!("Failed to set optimization level: {}", e))?
            .with_intra_threads(4)
            .map_err(|e| format!("Failed to set threads: {}", e))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("Failed to load embedding model: {}", e))?;

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        println!("Embedding engine initialized (EmbeddingGemma-300M)");
        Ok(Self { session, tokenizer })
    }

    /// Generate embedding for a single text
    pub fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let embeddings = self.embed_batch(&[text])?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }

    /// Generate embeddings for multiple texts (batched)
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // Tokenize all texts
        let encodings = self.tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| format!("Tokenization failed: {}", e))?;

        let batch_size = encodings.len();

        // Find max sequence length
        let max_len = encodings.iter()
            .map(|e| e.get_ids().len())
            .max()
            .unwrap_or(0);

        // Pad sequences and create input tensors
        let mut input_ids: Vec<i64> = Vec::with_capacity(batch_size * max_len);
        let mut attention_mask: Vec<i64> = Vec::with_capacity(batch_size * max_len);

        for encoding in &encodings {
            let ids = encoding.get_ids();
            let mask = encoding.get_attention_mask();

            // Add actual tokens
            for &id in ids {
                input_ids.push(id as i64);
            }
            for &m in mask {
                attention_mask.push(m as i64);
            }

            // Pad to max_len
            let padding = max_len - ids.len();
            for _ in 0..padding {
                input_ids.push(0); // PAD token
                attention_mask.push(0);
            }
        }

        // Create ndarray tensors
        let input_ids_array = Array2::from_shape_vec(
            (batch_size, max_len),
            input_ids
        ).map_err(|e| format!("Failed to create input_ids array: {}", e))?;

        let attention_mask_array = Array2::from_shape_vec(
            (batch_size, max_len),
            attention_mask
        ).map_err(|e| format!("Failed to create attention_mask array: {}", e))?;

        // Run inference
        let outputs = self.session
            .run(ort::inputs![
                "input_ids" => input_ids_array.view(),
                "attention_mask" => attention_mask_array.view(),
            ].map_err(|e| format!("Failed to create inputs: {}", e))?)
            .map_err(|e| format!("Inference failed: {}", e))?;

        // Extract embeddings from output
        // EmbeddingGemma outputs last_hidden_state, we take mean pooling
        let output_tensor = outputs.get("last_hidden_state")
            .or_else(|| outputs.get("sentence_embedding"))
            .ok_or("No embedding output found")?;

        let embeddings_array = output_tensor
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract embeddings: {}", e))?;

        let shape = embeddings_array.shape();

        // Handle different output shapes
        let result = if shape.len() == 3 {
            // Shape: [batch, seq_len, hidden_dim] - need mean pooling
            let view = embeddings_array.view();
            let batch = shape[0];
            let hidden_dim = shape[2];

            (0..batch)
                .map(|i| {
                    // Mean pooling over sequence length
                    let slice = view.slice(ndarray::s![i, .., ..]);
                    let mean = slice.mean_axis(Axis(0))
                        .unwrap_or_else(|| Array1::zeros(hidden_dim));
                    mean.to_vec()
                })
                .collect()
        } else if shape.len() == 2 {
            // Shape: [batch, hidden_dim] - already pooled
            let view = embeddings_array.view();
            (0..shape[0])
                .map(|i| view.slice(ndarray::s![i, ..]).to_vec())
                .collect()
        } else {
            return Err(format!("Unexpected output shape: {:?}", shape));
        };

        Ok(result)
    }

    /// Embed text and return with metadata
    pub fn embed_with_result(&self, text: &str) -> Result<EmbeddingResult, String> {
        let embedding = self.embed(text)?;
        Ok(EmbeddingResult {
            text: text.to_string(),
            embedding,
        })
    }
}

/// Compute cosine similarity between two embeddings
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Find top-k most similar embeddings
pub fn find_similar(
    query: &[f32],
    candidates: &[(String, Vec<f32>)],
    top_k: usize,
) -> Vec<(String, f32)> {
    let mut scored: Vec<(String, f32)> = candidates
        .iter()
        .map(|(text, emb)| (text.clone(), cosine_similarity(query, emb)))
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.001);

        let d = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &d) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_find_similar() {
        let query = vec![1.0, 0.0, 0.0];
        let candidates = vec![
            ("exact".to_string(), vec![1.0, 0.0, 0.0]),
            ("perpendicular".to_string(), vec![0.0, 1.0, 0.0]),
            ("close".to_string(), vec![0.9, 0.1, 0.0]),
        ];

        let results = find_similar(&query, &candidates, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "exact");
        assert_eq!(results[1].0, "close");
    }
}
