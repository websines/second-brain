//! Text chunking module for semantic document splitting.
//!
//! Uses text-splitter crate for markdown-aware chunking that preserves
//! semantic boundaries (paragraphs, sentences, headings).

use serde::{Deserialize, Serialize};
use text_splitter::{Characters, MarkdownSplitter};

/// Configuration for the document chunker
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Target chunk size in characters
    pub chunk_size: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            chunk_size: 1000,    // ~250 tokens at 4 chars/token
        }
    }
}

/// A chunk of text with position metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub text: String,
    pub start_char: usize,
    pub end_char: usize,
    pub chunk_index: usize,
}

/// A chunk with source metadata for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkWithMeta {
    pub text: String,
    pub source_url: String,
    pub source_title: String,
    pub chunk_index: usize,
    pub total_chunks: usize,
}

/// Document chunker for splitting text into semantic chunks
pub struct DocumentChunker {
    config: ChunkerConfig,
    splitter: MarkdownSplitter<Characters>,
}

impl DocumentChunker {
    /// Create a new document chunker with default config
    pub fn new() -> Self {
        Self::with_config(ChunkerConfig::default())
    }

    /// Create a new document chunker with custom config
    pub fn with_config(config: ChunkerConfig) -> Self {
        // Create splitter with target chunk size in characters
        let splitter = MarkdownSplitter::new(config.chunk_size);

        Self { config, splitter }
    }

    /// Chunk markdown content into semantic pieces
    pub fn chunk_markdown(&self, content: &str) -> Vec<Chunk> {
        let chunks: Vec<_> = self.splitter.chunks(content).collect();

        let mut result = Vec::with_capacity(chunks.len());
        let mut current_pos = 0;

        for (index, chunk_text) in chunks.into_iter().enumerate() {
            // Find the actual position in the original content
            let start = content[current_pos..]
                .find(chunk_text)
                .map(|pos| current_pos + pos)
                .unwrap_or(current_pos);

            let end = start + chunk_text.len();
            current_pos = end;

            result.push(Chunk {
                text: chunk_text.to_string(),
                start_char: start,
                end_char: end,
                chunk_index: index,
            });
        }

        result
    }

    /// Chunk content with source metadata attached
    pub fn chunk_with_metadata(
        &self,
        content: &str,
        source_url: &str,
        source_title: &str,
    ) -> Vec<ChunkWithMeta> {
        let chunks = self.chunk_markdown(content);
        let total = chunks.len();

        chunks
            .into_iter()
            .map(|chunk| ChunkWithMeta {
                text: chunk.text,
                source_url: source_url.to_string(),
                source_title: source_title.to_string(),
                chunk_index: chunk.chunk_index,
                total_chunks: total,
            })
            .collect()
    }

    /// Chunk plain text (non-markdown)
    pub fn chunk_text(&self, content: &str) -> Vec<Chunk> {
        // For plain text, we still use markdown splitter as it handles
        // paragraphs and sentences well even without markdown syntax
        self.chunk_markdown(content)
    }

    /// Get the current chunk size configuration
    pub fn chunk_size(&self) -> usize {
        self.config.chunk_size
    }
}

impl Default for DocumentChunker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_markdown() {
        let chunker = DocumentChunker::new();
        let content = r#"
# Heading 1

This is the first paragraph with some content.

## Heading 2

This is another paragraph with different content.

- List item 1
- List item 2
- List item 3

Final paragraph here.
"#;

        let chunks = chunker.chunk_markdown(content);
        assert!(!chunks.is_empty());

        // Verify chunks are non-empty
        for chunk in &chunks {
            assert!(!chunk.text.trim().is_empty());
        }
    }

    #[test]
    fn test_chunk_with_metadata() {
        let chunker = DocumentChunker::new();
        let content = "Short content for testing.";

        let chunks = chunker.chunk_with_metadata(
            content,
            "https://example.com",
            "Test Page",
        );

        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].source_url, "https://example.com");
        assert_eq!(chunks[0].source_title, "Test Page");
        assert_eq!(chunks[0].total_chunks, chunks.len());
    }
}
