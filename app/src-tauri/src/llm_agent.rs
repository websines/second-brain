use crate::knowledge_base::KnowledgeBase;
use crate::web_crawler::WebCrawler;
use rig::{
    completion::{AssistantContent, CompletionModel, Prompt, ToolDefinition},
    providers::openai,
    tool::Tool,
};

/// Extract text from AssistantContent and strip thinking tags
fn extract_text(content: &AssistantContent) -> String {
    let raw_text = match content {
        AssistantContent::Text(text_content) => text_content.text.clone(),
        AssistantContent::ToolCall(tool_call) => {
            format!("[Tool call: {}]", tool_call.function.name)
        }
    };
    // Strip thinking tags from the response
    strip_thinking_tags(&raw_text)
}

/// Extract JSON object from a response that might contain other text
fn extract_json_from_response(response: &str) -> String {
    // First strip thinking tags
    let cleaned = strip_thinking_tags(response);

    // Try to find JSON object in the response
    if let Some(start) = cleaned.find('{') {
        // Find matching closing brace
        let mut depth = 0;
        let mut end = start;
        for (i, c) in cleaned[start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        if end > start {
            return cleaned[start..end].to_string();
        }
    }
    cleaned
}

/// Strip <think>...</think> and similar reasoning tags from LLM responses
/// Some models (like Qwen, DeepSeek) output thinking process in these tags
fn strip_thinking_tags(response: &str) -> String {
    let mut result = response.to_string();

    // Strip <think>...</think> tags (case insensitive, handles newlines)
    loop {
        let lower = result.to_lowercase();
        if let Some(start) = lower.find("<think>") {
            if let Some(end_tag_start) = lower[start..].find("</think>") {
                let end = start + end_tag_start + "</think>".len();
                result = format!("{}{}", &result[..start], &result[end..]);
                continue;
            }
        }
        break;
    }

    // Strip <thinking>...</thinking> tags
    loop {
        let lower = result.to_lowercase();
        if let Some(start) = lower.find("<thinking>") {
            if let Some(end_tag_start) = lower[start..].find("</thinking>") {
                let end = start + end_tag_start + "</thinking>".len();
                result = format!("{}{}", &result[..start], &result[end..]);
                continue;
            }
        }
        break;
    }

    // Strip <reasoning>...</reasoning> tags
    loop {
        let lower = result.to_lowercase();
        if let Some(start) = lower.find("<reasoning>") {
            if let Some(end_tag_start) = lower[start..].find("</reasoning>") {
                let end = start + end_tag_start + "</reasoning>".len();
                result = format!("{}{}", &result[..start], &result[end..]);
                continue;
            }
        }
        break;
    }

    // Clean up any extra whitespace left behind
    result.trim().to_string()
}
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Custom error type for tool operations
#[derive(Debug)]
pub struct ToolError(String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ToolError {}

impl From<String> for ToolError {
    fn from(s: String) -> Self {
        ToolError(s)
    }
}

impl From<&str> for ToolError {
    fn from(s: &str) -> Self {
        ToolError(s.to_string())
    }
}

/// Real-time suggestion generated during a meeting
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RealtimeSuggestion {
    /// Key insight about the current discussion
    pub insight: Option<String>,
    /// Suggested question to ask
    pub question: Option<String>,
    /// Related information from knowledge base
    pub related_info: Option<String>,
}

/// Action item extracted from meeting
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractedActionItem {
    pub task: String,
    pub assignee: Option<String>,
    pub deadline: Option<String>,
}

/// Highlights and structured data extracted from meeting after it ends
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeetingHighlights {
    /// 2-3 sentence summary
    pub summary: Option<String>,
    /// Key topics discussed
    pub key_topics: Vec<String>,
    /// Action items with assignees
    pub action_items: Vec<ExtractedActionItem>,
    /// Decisions made
    pub decisions: Vec<String>,
    /// Key moments or quotes
    pub highlights: Vec<String>,
    /// Items needing follow-up
    pub follow_ups: Vec<String>,
}

/// Tool arguments for searching transcripts
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchTranscriptsArgs {
    /// The search query to find relevant meeting segments
    pub query: String,
    /// Maximum number of results to return (default: 5)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize { 5 }

/// Tool for searching past meeting transcripts
pub struct SearchTranscriptsTool {
    pub kb: Arc<RwLock<Option<KnowledgeBase>>>,
}

impl Tool for SearchTranscriptsTool {
    const NAME: &'static str = "search_transcripts";

    type Args = SearchTranscriptsArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search through past meeting transcripts to find relevant information. \
                         Use this when you need to recall what was discussed in previous meetings, \
                         find context about a topic, or look up what someone said."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query - can be natural language describing what you're looking for"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let kb_guard = self.kb.read().await;
        let kb = kb_guard.as_ref().ok_or(ToolError::from("Knowledge base not initialized"))?;

        let results = kb.search_similar(&args.query, args.limit).await.map_err(ToolError::from)?;

        if results.is_empty() {
            return Ok("No relevant meeting segments found.".to_string());
        }

        let formatted: Vec<String> = results
            .iter()
            .map(|r| {
                format!(
                    "Meeting: {}\nSpeaker: {}\nText: {}",
                    r.meeting_title,
                    r.segment.speaker,
                    r.segment.text
                )
            })
            .collect();

        Ok(formatted.join("\n\n---\n\n"))
    }
}

/// Tool arguments for getting action items
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetActionItemsArgs {
    /// Filter by status: "open", "in_progress", "done", or "all"
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String { "open".to_string() }

/// Tool for retrieving action items from meetings
pub struct GetActionItemsTool {
    pub kb: Arc<RwLock<Option<KnowledgeBase>>>,
}

impl Tool for GetActionItemsTool {
    const NAME: &'static str = "get_action_items";

    type Args = GetActionItemsArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get a list of action items from past meetings. \
                         Use this to track tasks, follow-ups, and commitments made during meetings."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["open", "in_progress", "done", "all"],
                        "description": "Filter by status",
                        "default": "open"
                    }
                }
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let kb_guard = self.kb.read().await;
        let kb = kb_guard.as_ref().ok_or(ToolError::from("Knowledge base not initialized"))?;

        let actions = kb.get_open_actions().await.map_err(ToolError::from)?;

        if actions.is_empty() {
            return Ok("No open action items found.".to_string());
        }

        let formatted: Vec<String> = actions
            .iter()
            .map(|a| {
                let assignee = a.assignee.as_deref().unwrap_or("Unassigned");
                let deadline = a.deadline.as_deref().unwrap_or("No deadline");
                format!(
                    "• {} (Assignee: {}, Deadline: {}, Status: {})",
                    a.text, assignee, deadline, a.status
                )
            })
            .collect();

        Ok(formatted.join("\n"))
    }
}

// ============================================================================
// Web Crawler Agent Tools
// ============================================================================

/// Tool arguments for web search
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WebSearchArgs {
    /// The search query to find relevant web pages
    pub query: String,
    /// Maximum number of results to return (default: 5)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// Tool for searching the web using DuckDuckGo
pub struct WebSearchTool;

impl Tool for WebSearchTool {
    const NAME: &'static str = "web_search";

    type Args = WebSearchArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search the web using DuckDuckGo to find relevant information. \
                         Use this when you need current information, facts, or resources \
                         that might not be in the meeting knowledge base. Returns titles and URLs."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query - be specific for better results"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let crawler = WebCrawler::new();
        let results = crawler.search(&args.query, args.limit).await.map_err(ToolError::from)?;

        if results.is_empty() {
            return Ok("No search results found.".to_string());
        }

        let formatted: Vec<String> = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!("{}. {} - {}", i + 1, r.title, r.url)
            })
            .collect();

        Ok(formatted.join("\n"))
    }
}

/// Tool arguments for crawling a URL
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CrawlUrlArgs {
    /// The URL to crawl and extract content from
    pub url: String,
    /// Tags to associate with the stored content
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether to store the content in the knowledge base (default: true)
    #[serde(default = "default_store")]
    pub store: bool,
}

fn default_store() -> bool { true }

/// Tool for crawling a URL, converting to markdown, and optionally storing
pub struct CrawlUrlTool {
    pub kb: Arc<RwLock<Option<KnowledgeBase>>>,
}

impl Tool for CrawlUrlTool {
    const NAME: &'static str = "crawl_url";

    type Args = CrawlUrlArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Crawl a web page, convert it to markdown, and optionally store it \
                         in the knowledge base for future reference. Use this to fetch and \
                         read web content, or to add useful resources to the meeting knowledge base."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The full URL to crawl (must include http:// or https://)"
                    },
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Tags to categorize this content (e.g., ['meeting-prep', 'project-x'])"
                    },
                    "store": {
                        "type": "boolean",
                        "description": "Whether to store in knowledge base for future retrieval",
                        "default": true
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let crawler = WebCrawler::new();
        let page = crawler.crawl_url(&args.url).await.map_err(ToolError::from)?;

        // Truncate content for response (full content is stored)
        let preview = if page.markdown.len() > 2000 {
            format!("{}...\n\n[Content truncated - {} total characters]",
                &page.markdown[..2000], page.markdown.len())
        } else {
            page.markdown.clone()
        };

        if args.store {
            let kb_guard = self.kb.read().await;
            if let Some(kb) = kb_guard.as_ref() {
                // add_knowledge_source handles chunking and embedding internally
                let source_id = kb.add_knowledge_source(
                    &page.url,
                    &page.title,
                    &page.markdown,
                    "web",
                    args.tags,
                ).await.map_err(ToolError::from)?;

                return Ok(format!(
                    "**{}**\nURL: {}\nStored: Yes (ID: {})\n\n---\n\n{}",
                    page.title, page.url, source_id, preview
                ));
            }
        }

        Ok(format!(
            "**{}**\nURL: {}\nStored: No\n\n---\n\n{}",
            page.title, page.url, preview
        ))
    }
}

/// Tool arguments for searching knowledge base
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchKnowledgeArgs {
    /// The search query to find relevant knowledge chunks
    pub query: String,
    /// Maximum number of results to return (default: 5)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Optional tags to filter by
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Tool for searching the knowledge base (web content, documents, etc.)
pub struct SearchKnowledgeTool {
    pub kb: Arc<RwLock<Option<KnowledgeBase>>>,
}

impl Tool for SearchKnowledgeTool {
    const NAME: &'static str = "search_knowledge";

    type Args = SearchKnowledgeArgs;
    type Output = String;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search the knowledge base for relevant information from stored web pages, \
                         documents, and other resources. Use this to find context, references, \
                         or background information that was previously saved."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query - natural language describing what you're looking for"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return",
                        "default": 5
                    },
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Filter results to only those with these tags"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let kb_guard = self.kb.read().await;
        let kb = kb_guard.as_ref().ok_or(ToolError::from("Knowledge base not initialized"))?;

        let tags_option = if args.tags.is_empty() { None } else { Some(args.tags) };
        let results = kb.search_knowledge(&args.query, args.limit, tags_option)
            .await
            .map_err(ToolError::from)?;

        if results.is_empty() {
            return Ok("No relevant knowledge found.".to_string());
        }

        let formatted: Vec<String> = results
            .iter()
            .map(|r| {
                format!(
                    "**Source:** {} ({})\n**Relevance:** {:.0}%\n**Content:**\n{}\n",
                    r.source_title,
                    r.source_url,
                    r.similarity * 100.0,
                    r.chunk.text
                )
            })
            .collect();

        Ok(formatted.join("\n---\n\n"))
    }
}

/// The LLM-powered meeting assistant
#[derive(Clone)]
pub struct MeetingAssistant {
    client: openai::Client,
    model: String,
}

impl MeetingAssistant {
    /// Create a new meeting assistant
    ///
    /// # Arguments
    /// * `api_url` - The OpenAI-compatible API URL (e.g., "https://lmstudio.subh-dev.xyz/llm/v1")
    /// * `model` - The model name (e.g., "openai/gpt-oss-20b")
    /// * `api_key` - The API key (can be empty for local servers like LM Studio/Ollama)
    pub fn new(api_url: &str, model: &str, api_key: &str) -> Self {
        // from_url signature is (api_key, base_url)
        // Use provided key or fallback to dummy for local servers
        let key = if api_key.trim().is_empty() { "not-needed" } else { api_key };
        let client = openai::Client::from_url(key, api_url);

        Self {
            client,
            model: model.to_string(),
        }
    }

    /// Ask a question using Graph-RAG (Graph + Retrieval Augmented Generation)
    /// Combines entity extraction, graph traversal, temporal awareness, and vector search
    pub async fn ask(
        &self,
        question: &str,
        kb: Arc<RwLock<Option<KnowledgeBase>>>,
    ) -> Result<String, String> {
        println!("[Graph-RAG] Asking question: {}", question);

        // Step 1: Use Graph-RAG to get comprehensive context
        let context = {
            let kb_guard = kb.read().await;
            if let Some(kb_ref) = kb_guard.as_ref() {
                println!("[Graph-RAG] Knowledge base found, running Graph-RAG query...");

                match kb_ref.graph_rag_query(question, 5).await {
                    Ok(graph_context) => {
                        // Build rich context from Graph-RAG results
                        let mut context_parts = Vec::new();

                        // Add temporal context if detected
                        if let Some(ref temporal) = graph_context.temporal_context {
                            context_parts.push(format!(
                                "## Temporal Reference Detected\nTime reference: {}\n",
                                temporal.time_reference
                            ));
                        }

                        // Add extracted query entities
                        if !graph_context.query_entities.is_empty() {
                            let entities_str: Vec<String> = graph_context.query_entities
                                .iter()
                                .map(|e| format!("{} ({})", e.text, e.label))
                                .collect();
                            context_parts.push(format!(
                                "## Entities Mentioned in Query\n{}\n",
                                entities_str.join(", ")
                            ));
                        }

                        // Add related meetings
                        if !graph_context.related_meetings.is_empty() {
                            let meetings_str: Vec<String> = graph_context.related_meetings
                                .iter()
                                .take(3)
                                .map(|m| {
                                    let segments_preview: Vec<String> = m.relevant_segments
                                        .iter()
                                        .take(2)
                                        .map(|s| format!("  - {}: \"{}...\"", s.speaker, &s.text[..s.text.len().min(100)]))
                                        .collect();
                                    format!(
                                        "**{}** ({} days ago)\n{}",
                                        m.meeting.title,
                                        m.days_ago,
                                        segments_preview.join("\n")
                                    )
                                })
                                .collect();
                            context_parts.push(format!(
                                "## Related Meetings\n{}\n",
                                meetings_str.join("\n\n")
                            ));
                        }

                        // Add related people with their topics
                        if !graph_context.related_people.is_empty() {
                            let people_str: Vec<String> = graph_context.related_people
                                .iter()
                                .map(|p| {
                                    let topics = if p.recent_topics.is_empty() {
                                        "No topics recorded".to_string()
                                    } else {
                                        p.recent_topics.join(", ")
                                    };
                                    format!("- **{}** (last seen {} days ago): discusses {}", p.name, p.last_seen_days_ago, topics)
                                })
                                .collect();
                            context_parts.push(format!(
                                "## Related People\n{}\n",
                                people_str.join("\n")
                            ));
                        }

                        // Add related topics
                        if !graph_context.related_topics.is_empty() {
                            let topics_str: Vec<String> = graph_context.related_topics
                                .iter()
                                .map(|t| {
                                    let people = if t.related_people.is_empty() {
                                        "various participants".to_string()
                                    } else {
                                        t.related_people.join(", ")
                                    };
                                    format!("- **{}**: mentioned {} times, last {} days ago (discussed by: {})",
                                        t.name, t.mention_count, t.last_mentioned_days_ago, people)
                                })
                                .collect();
                            context_parts.push(format!(
                                "## Related Topics\n{}\n",
                                topics_str.join("\n")
                            ));
                        }

                        // Add open action items (if relevant)
                        if !graph_context.open_actions.is_empty() {
                            let actions_str: Vec<String> = graph_context.open_actions
                                .iter()
                                .take(5)
                                .map(|a| {
                                    let assignee = a.assignee.as_deref().unwrap_or("Unassigned");
                                    format!("- {} (assigned to: {})", a.text, assignee)
                                })
                                .collect();
                            context_parts.push(format!(
                                "## Open Action Items\n{}\n",
                                actions_str.join("\n")
                            ));
                        }

                        // Add recent decisions
                        if !graph_context.recent_decisions.is_empty() {
                            let decisions_str: Vec<String> = graph_context.recent_decisions
                                .iter()
                                .take(5)
                                .map(|d| format!("- {}", d.text))
                                .collect();
                            context_parts.push(format!(
                                "## Recent Decisions\n{}\n",
                                decisions_str.join("\n")
                            ));
                        }

                        // Add similar knowledge chunks from vector search
                        // NOTE: These are NOT documents mentioned in meetings - they are retrieved
                        // via semantic similarity and may or may not be relevant
                        if !graph_context.similar_chunks.is_empty() {
                            let chunks_str: Vec<String> = graph_context.similar_chunks
                                .iter()
                                .map(|r| {
                                    let excerpt = if r.chunk.text.len() > 300 {
                                        format!("{}...", &r.chunk.text[..300])
                                    } else {
                                        r.chunk.text.clone()
                                    };
                                    format!(
                                        "### {} ({:.0}% similarity)\nURL: {}\n> {}\n",
                                        r.source_title,
                                        r.similarity * 100.0,
                                        r.source_url,
                                        excerpt.replace("\n", "\n> ")
                                    )
                                })
                                .collect();
                            context_parts.push(format!(
                                "## Potentially Relevant Documents (from Knowledge Base - NOT mentioned in meetings)\n{}\n",
                                chunks_str.join("\n")
                            ));
                        }

                        context_parts.join("\n")
                    }
                    Err(e) => {
                        println!("[Graph-RAG] Error: {}", e);
                        // Fall back to simple vector search
                        let results = kb_ref.search_knowledge(question, 5, None).await.unwrap_or_default();
                        if results.is_empty() {
                            String::new()
                        } else {
                            results.iter()
                                .map(|r| format!(
                                    "Source: {} ({})\n{}\n",
                                    r.source_title,
                                    r.source_url,
                                    r.chunk.text
                                ))
                                .collect::<Vec<_>>()
                                .join("\n---\n")
                        }
                    }
                }
            } else {
                println!("[Graph-RAG] Knowledge base NOT initialized!");
                String::new()
            }
        };

        // Step 2: Build prompt with rich Graph-RAG context
        let prompt = if context.is_empty() {
            println!("[Graph-RAG] No context found, sending empty KB response");
            return Ok("I couldn't find any relevant information in your knowledge base to answer this question.\n\n**Possible reasons:**\n- Your knowledge base might be empty. Try adding some content first (web pages, documents, or text).\n- The question might not match any stored content. Try rephrasing or adding more relevant content.\n\n**To add content:**\n1. Go to the \"Add Source\" tab\n2. Add a URL to crawl, or upload a document\n3. Then try asking your question again!".to_string());
        } else {
            format!(
                r#"You are Second Brain, a personal AI assistant with access to the user's meeting history, knowledge base, and documents.

RETRIEVED CONTEXT:
{}

USER QUESTION: {}

RESPONSE GUIDELINES:

**Structure your response clearly:**
1. Start with a brief, direct answer (1-2 sentences)
2. Then provide supporting details organized by category

**Formatting rules:**
- Use **bold** for meeting names, people, and document titles
- Use bullet points for lists (action items, decisions, topics)
- For documents, format as: **Document Title** - Brief description of relevance
- For meetings, include the date/time reference when available
- Keep paragraphs short (2-3 sentences max)

**Content guidelines:**
- Be concise - aim for 150-250 words unless more detail is needed
- Cite sources naturally: "In the **Project Review** meeting..."
- If action items exist, list them with assignees: "- [ ] Task (Owner)"
- Acknowledge gaps: "I found X, but couldn't find Y"

**IMPORTANT - Document Attribution:**
- The "Potentially Relevant Documents" section contains documents retrieved by similarity search
- These documents were NOT mentioned or discussed in meetings - they are just topically similar
- Do NOT say a document was "mentioned in the meeting" unless it appears in the meeting transcript
- If a document is potentially useful, say: "You may find **Document Title** relevant" (not "was discussed")

**Avoid:**
- Overly long tables (use simple bullet lists instead)
- Repeating the same information multiple ways
- Speculation beyond what's in the context
- Falsely claiming documents were mentioned in meetings when they weren't

ANSWER:"#,
                context,
                question
            )
        };

        // Step 3: Get response from LLM
        let model = self.client.completion_model(&self.model);

        let response = model.completion_request(prompt)
            .send()
            .await
            .map_err(|e| format!("Failed to get response: {}", e))?;

        Ok(extract_text(&response.choice.first()))
    }

    /// Ask a question about a specific meeting
    pub async fn ask_about_meeting(
        &self,
        question: &str,
        meeting_title: &str,
        transcript: &[String],  // Segments as "Speaker: text"
        action_items: &[String],
        decisions: &[String],
    ) -> Result<String, String> {
        // Build meeting context
        let transcript_text = if transcript.is_empty() {
            "No transcript available.".to_string()
        } else {
            transcript.join("\n")
        };

        let actions_text = if action_items.is_empty() {
            "None recorded.".to_string()
        } else {
            action_items.iter()
                .map(|a| format!("- {}", a))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let decisions_text = if decisions.is_empty() {
            "None recorded.".to_string()
        } else {
            decisions.iter()
                .map(|d| format!("- {}", d))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let prompt = format!(
            r#"You are Second Brain, answering a question about a specific meeting.

MEETING: {}

TRANSCRIPT:
{}

ACTION ITEMS:
{}

DECISIONS:
{}

USER QUESTION: {}

INSTRUCTIONS:
- Answer based ONLY on this meeting's content
- Be concise and direct
- Quote specific parts of the transcript when relevant
- If the answer isn't in this meeting, say so clearly
- Use **bold** for speaker names and key terms

ANSWER:"#,
            meeting_title,
            transcript_text,
            actions_text,
            decisions_text,
            question
        );

        let model = self.client.completion_model(&self.model);
        let response = model.completion_request(prompt)
            .send()
            .await
            .map_err(|e| format!("Failed to get response: {}", e))?;

        Ok(extract_text(&response.choice.first()))
    }

    /// Generate a meeting summary
    pub async fn summarize_meeting(
        &self,
        segments: &[String],
    ) -> Result<String, String> {
        let combined = segments.join("\n\n");

        let agent = self.client
            .agent(&self.model)
            .preamble(r#"
You are a meeting summarizer. Given a transcript, create a concise summary that includes:

1. **Key Topics Discussed** - Main subjects covered
2. **Decisions Made** - Any conclusions or agreements reached
3. **Action Items** - Tasks assigned with owners if mentioned
4. **Open Questions** - Unresolved issues that need follow-up

Be concise but comprehensive. Use bullet points for clarity.
            "#)
            .temperature(0.3)
            .build();

        let prompt = format!("Summarize this meeting transcript:\n\n{}", combined);
        let response = agent.prompt(prompt)
            .await
            .map_err(|e| format!("Failed to generate summary: {}", e))?;
        Ok(strip_thinking_tags(&response))
    }

    /// Process meeting after it ends - extract highlights, action items, decisions
    pub async fn process_meeting_end(
        &self,
        segments: &[String],
        meeting_title: &str,
    ) -> Result<MeetingHighlights, String> {
        if segments.is_empty() {
            return Ok(MeetingHighlights::default());
        }

        let combined = segments.join("\n\n");

        let prompt = format!(
            r#"Analyze this meeting transcript and extract structured information.

MEETING TITLE: {}

TRANSCRIPT:
{}

IMPORTANT: Return ONLY a valid JSON object with NO other text before or after. Do not use markdown code blocks.

JSON format:
{{
    "summary": "2-3 sentence summary of the meeting",
    "key_topics": ["topic1", "topic2"],
    "action_items": [
        {{"task": "description", "assignee": "person name or null", "deadline": "date or null"}}
    ],
    "decisions": ["decision1", "decision2"],
    "highlights": ["key moment or quote 1", "key moment 2"],
    "follow_ups": ["item needing follow-up 1"]
}}

Start your response with {{ and end with }}. No explanations."#,
            meeting_title,
            combined
        );

        let model = self.client.completion_model(&self.model);
        let response = model.completion_request(prompt)
            .send()
            .await
            .map_err(|e| format!("Failed to process meeting: {}", e))?;

        let response_text = extract_text(&response.choice.first());

        // Extract JSON from response (handles LLMs that add text around JSON)
        let json_str = extract_json_from_response(&response_text);
        println!("[MeetingHighlights] Raw response: {}", &response_text[..response_text.len().min(200)]);
        println!("[MeetingHighlights] Extracted JSON: {}", &json_str[..json_str.len().min(200)]);

        // Parse JSON response
        match serde_json::from_str::<MeetingHighlights>(&json_str) {
            Ok(highlights) => {
                println!("[MeetingHighlights] Successfully parsed: {} topics, {} action items, {} decisions",
                    highlights.key_topics.len(),
                    highlights.action_items.len(),
                    highlights.decisions.len());
                Ok(highlights)
            },
            Err(e) => {
                println!("[MeetingHighlights] JSON parse failed: {}. Trying to extract manually...", e);
                // Try to extract structured data manually from the text
                let mut highlights = MeetingHighlights::default();

                // Try to find a summary in the text
                if let Some(summary_start) = response_text.to_lowercase().find("summary") {
                    let after_summary = &response_text[summary_start..];
                    if let Some(colon) = after_summary.find(':') {
                        let summary_text = &after_summary[colon + 1..];
                        // Take text until next section marker or 200 chars
                        let end = summary_text.find("\n\n")
                            .or_else(|| summary_text.find("key_topics"))
                            .or_else(|| summary_text.find("Key Topics"))
                            .unwrap_or(summary_text.len().min(300));
                        let extracted = summary_text[..end].trim().trim_matches('"').trim();
                        if !extracted.is_empty() {
                            highlights.summary = Some(extracted.to_string());
                        }
                    }
                }

                // If still no summary, take first meaningful lines
                if highlights.summary.is_none() {
                    let clean = response_text.lines()
                        .filter(|l| !l.trim().is_empty() && !l.contains('{') && !l.contains('}'))
                        .take(3)
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !clean.is_empty() {
                        highlights.summary = Some(clean);
                    }
                }

                Ok(highlights)
            }
        }
    }

    /// Generate real-time suggestions during a meeting
    /// Uses Graph-RAG to pull rich context from KB, then synthesizes into human-like suggestions
    pub async fn generate_realtime_suggestions(
        &self,
        recent_transcript: &[String],  // Last few segments as "Speaker: text"
        meeting_context: Option<&str>,  // Optional meeting agenda/linked docs
        kb: Arc<RwLock<Option<KnowledgeBase>>>,
    ) -> Result<RealtimeSuggestion, String> {
        let start = std::time::Instant::now();

        if recent_transcript.is_empty() {
            return Ok(RealtimeSuggestion::default());
        }

        let transcript_text = recent_transcript.join("\n");

        // Step 1: Use Graph-RAG to get rich context based on current discussion (runs queries in parallel)
        let graph_context = {
            let kb_guard = kb.read().await;
            if let Some(kb_ref) = kb_guard.as_ref() {
                // Use the last transcript segment as the query for context retrieval
                let query = recent_transcript.last().map(|s| s.as_str()).unwrap_or("");
                match kb_ref.graph_rag_query(query, 3).await {
                    Ok(ctx) => {
                        println!("[Realtime] Graph-RAG completed in {:?}", start.elapsed());
                        Some(ctx)
                    }
                    Err(e) => {
                        eprintln!("[Realtime] Graph-RAG error: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        };

        // Step 2: Build context string from Graph-RAG results
        let mut kb_context = String::new();

        if let Some(ref ctx) = graph_context {
            // Related people who might be relevant
            if !ctx.related_people.is_empty() {
                let people: Vec<String> = ctx.related_people.iter()
                    .take(3)
                    .map(|p| format!("{} (discusses: {})", p.name, p.recent_topics.join(", ")))
                    .collect();
                kb_context.push_str(&format!("RELEVANT PEOPLE: {}\n", people.join("; ")));
            }

            // Related topics from past meetings
            if !ctx.related_topics.is_empty() {
                let topics: Vec<String> = ctx.related_topics.iter()
                    .take(3)
                    .map(|t| format!("{} (mentioned {} times, last {} days ago)", t.name, t.mention_count, t.last_mentioned_days_ago))
                    .collect();
                kb_context.push_str(&format!("RELATED TOPICS: {}\n", topics.join("; ")));
            }

            // Related meetings with relevant segments
            if !ctx.related_meetings.is_empty() {
                let meetings: Vec<String> = ctx.related_meetings.iter()
                    .take(2)
                    .map(|m| {
                        let snippet = m.relevant_segments.first()
                            .map(|s| format!("{}: \"{}\"", s.speaker, &s.text[..s.text.len().min(100)]))
                            .unwrap_or_default();
                        format!("{} ({} days ago): {}", m.meeting.title, m.days_ago, snippet)
                    })
                    .collect();
                kb_context.push_str(&format!("PAST DISCUSSIONS:\n{}\n", meetings.join("\n")));
            }

            // Open action items
            if !ctx.open_actions.is_empty() {
                let actions: Vec<String> = ctx.open_actions.iter()
                    .take(3)
                    .map(|a| {
                        let assignee = a.assignee.as_deref().unwrap_or("unassigned");
                        format!("- {} ({})", a.text, assignee)
                    })
                    .collect();
                kb_context.push_str(&format!("OPEN ACTION ITEMS:\n{}\n", actions.join("\n")));
            }

            // Recent decisions
            if !ctx.recent_decisions.is_empty() {
                let decisions: Vec<String> = ctx.recent_decisions.iter()
                    .take(2)
                    .map(|d| format!("- {}", d.text))
                    .collect();
                kb_context.push_str(&format!("RECENT DECISIONS:\n{}\n", decisions.join("\n")));
            }

            // Similar knowledge chunks (documents)
            if !ctx.similar_chunks.is_empty() {
                let docs: Vec<String> = ctx.similar_chunks.iter()
                    .take(2)
                    .map(|r| format!("{}: {}", r.source_title, &r.chunk.text[..r.chunk.text.len().min(150)]))
                    .collect();
                kb_context.push_str(&format!("RELEVANT DOCUMENTS:\n{}\n", docs.join("\n")));
            }
        }

        // Step 3: Build prompt for LLM
        let prompt = format!(
            r#"You are a helpful meeting assistant. Based on the current conversation and relevant context from the knowledge base, provide a brief, human-like insight.

{}
{}
CURRENT CONVERSATION:
{}

Respond with a JSON object:
{{
  "insight": "One helpful observation connecting the discussion to past context, or a key takeaway (1-2 sentences, conversational tone)",
  "question": "A question they could ask to clarify or advance the discussion (or null)",
  "related_info": "Brief mention of relevant past context if useful (or null)"
}}

Be conversational and helpful, like a knowledgeable colleague whispering useful context. Don't be formal or robotic."#,
            if let Some(ctx) = meeting_context {
                format!("MEETING AGENDA:\n{}\n", ctx)
            } else {
                String::new()
            },
            if kb_context.is_empty() {
                String::new()
            } else {
                format!("KNOWLEDGE BASE CONTEXT:\n{}\n", kb_context)
            },
            transcript_text
        );

        // Step 4: Get LLM response
        let llm_start = std::time::Instant::now();
        let model = self.client.completion_model(&self.model);
        let response = model.completion_request(prompt)
            .send()
            .await
            .map_err(|e| format!("Failed to get suggestions: {}", e))?;

        let response_text = extract_text(&response.choice.first());
        println!("[Realtime] LLM response in {:?}, total: {:?}", llm_start.elapsed(), start.elapsed());

        // Parse JSON response
        let json_str = extract_json_from_response(&response_text);

        match serde_json::from_str::<RealtimeSuggestion>(&json_str) {
            Ok(suggestion) => Ok(suggestion),
            Err(_) => {
                // Fallback: use response as insight
                Ok(RealtimeSuggestion {
                    insight: Some(response_text.lines().next().unwrap_or("").to_string()),
                    question: None,
                    related_info: None,
                })
            }
        }
    }

    /// Suggest questions to ask based on the current discussion
    pub async fn suggest_questions(
        &self,
        current_topic: &str,
        kb: Arc<RwLock<Option<KnowledgeBase>>>,
    ) -> Result<Vec<String>, String> {
        // Get relevant context from knowledge base
        let context = {
            let kb_guard = kb.read().await;
            if let Some(kb_ref) = kb_guard.as_ref() {
                let results = kb_ref.search_knowledge(current_topic, 3, None).await.unwrap_or_default();
                if results.is_empty() {
                    String::new()
                } else {
                    results.iter()
                        .map(|r| format!("- {}: {}", r.source_title, &r.chunk.text[..r.chunk.text.len().min(200)]))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            } else {
                String::new()
            }
        };

        let prompt = if context.is_empty() {
            format!(
                r#"The current topic being discussed is: {}

Suggest 2-3 relevant questions that could clarify important points or move the conversation forward.
Return ONLY a numbered list of questions, nothing else."#,
                current_topic
            )
        } else {
            format!(
                r#"The current topic being discussed is: {}

Related context from knowledge base:
{}

Suggest 2-3 relevant questions that could:
- Clarify important points
- Connect to the related context above
- Move the conversation forward

Return ONLY a numbered list of questions, nothing else."#,
                current_topic,
                context
            )
        };

        let model = self.client.completion_model(&self.model);
        let response_result = model
            .completion_request(prompt)
            .send()
            .await
            .map_err(|e| format!("Failed to generate questions: {}", e))?;

        let response = extract_text(&response_result.choice.first());

        // Parse numbered list
        let questions: Vec<String> = response
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("1.") || trimmed.starts_with("2.") || trimmed.starts_with("3.")
                    || trimmed.starts_with("- ") || trimmed.starts_with("• ")
            })
            .map(|line| {
                line.trim()
                    .trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '-' || c == '•')
                    .trim()
                    .to_string()
            })
            .filter(|q| !q.is_empty())
            .collect();

        Ok(questions)
    }

    /// Ask a question with an image (for screenshot analysis)
    /// Requires a vision-capable model (GPT-4V, Claude 3, LLaVA, etc.)
    pub async fn ask_with_image(
        &self,
        question: &str,
        image_data_url: &str,
    ) -> Result<String, String> {
        // For OpenAI-compatible APIs with vision support, we need to send the image
        // as part of a chat completion request with image_url content
        //
        // The rig-core library may not directly support multimodal, so we'll
        // construct the request manually or use a simpler approach

        // Build a prompt that describes the image context
        // For models that don't support vision, this will at least acknowledge the image
        let prompt = format!(
            r#"You are analyzing a screenshot captured during a meeting.

USER REQUEST: {}

[An image has been attached to this message. If you are a vision-capable model (GPT-4V, Claude 3, LLaVA, etc.), please analyze the image content.]

IMAGE: {}

Please provide:
1. A description of what you see in the screenshot
2. Any important text, data, or information visible
3. Key points or action items based on the content
4. Any relevant observations for the meeting context

Be concise but thorough in your analysis."#,
            question,
            if image_data_url.len() > 100 {
                format!("[Image data: {} bytes]", image_data_url.len())
            } else {
                image_data_url.to_string()
            }
        );

        // Try to use the completion API
        // Note: For full vision support, you may need to use a raw HTTP request
        // to the vision endpoint with the proper multimodal format
        let model = self.client.completion_model(&self.model);

        // For now, we'll try to send the image data URL in the prompt
        // Some local models (LLaVA) can handle this format
        let full_prompt = if self.model.contains("llava")
            || self.model.contains("vision")
            || self.model.contains("gpt-4")
            || self.model.contains("claude")
        {
            // For vision models, include the actual image data
            format!(
                "{}\n\n<image src=\"{}\" />",
                prompt,
                image_data_url
            )
        } else {
            // For non-vision models, just describe that an image was captured
            format!(
                r#"A screenshot was captured during the meeting.

The user asked: {}

Since you are a text-only model, I cannot show you the image. However, you can:
1. Acknowledge that the screenshot was captured
2. Ask the user to describe what they see
3. Suggest they use a vision-capable model for image analysis

Please respond helpfully."#,
                question
            )
        };

        let response = model
            .completion_request(full_prompt)
            .send()
            .await
            .map_err(|e| format!("Failed to analyze image: {}", e))?;

        Ok(extract_text(&response.choice.first()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_args_default() {
        let args: SearchTranscriptsArgs = serde_json::from_str(r#"{"query": "test"}"#).unwrap();
        assert_eq!(args.limit, 5);
    }
}
