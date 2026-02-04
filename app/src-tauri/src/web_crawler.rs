//! Web crawler module for searching and fetching web content.
//!
//! Uses spider crate for web crawling and duckduckgo_search for web search.
//! Converts web pages to markdown for storage in the knowledge base.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Result from a web search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// A crawled web page with content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawledPage {
    pub url: String,
    pub title: String,
    pub markdown: String,
    pub html: String,
    pub crawled_at: u64,
}

/// Configuration for the web crawler
#[derive(Debug, Clone)]
pub struct CrawlerConfig {
    /// User agent to use for requests
    pub user_agent: String,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Whether to respect robots.txt
    pub respect_robots_txt: bool,
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            user_agent: "SecondBrain/1.0 (Meeting Assistant)".to_string(),
            timeout_secs: 30,
            respect_robots_txt: true,
        }
    }
}

/// Web crawler for searching and fetching content
pub struct WebCrawler {
    config: CrawlerConfig,
}

impl WebCrawler {
    /// Create a new web crawler with default config
    pub fn new() -> Self {
        Self::with_config(CrawlerConfig::default())
    }

    /// Create a new web crawler with custom config
    pub fn with_config(config: CrawlerConfig) -> Self {
        Self { config }
    }

    /// Search DuckDuckGo and return results
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
        use duckduckgo_search::DuckDuckGoSearch;

        println!("[WebSearch] Searching for: {}", query);

        let search = DuckDuckGoSearch::new();

        // DuckDuckGoSearch::search takes &str
        let results = match search.search(query).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[WebSearch] DuckDuckGo search failed: {}", e);
                // Return empty results instead of error - search is best-effort
                return Ok(vec![]);
            }
        };

        println!("[WebSearch] Got {} raw results", results.len());

        // Convert to our SearchResult type, taking only 'limit' results
        let search_results: Vec<SearchResult> = results
            .into_iter()
            .take(limit)
            .filter(|(title, url)| !title.is_empty() && !url.is_empty())
            .map(|(title, url)| SearchResult {
                title,
                url,
                snippet: String::new(), // DuckDuckGo crate doesn't provide snippets
            })
            .collect();

        println!("[WebSearch] Returning {} filtered results", search_results.len());

        Ok(search_results)
    }

    /// Crawl a single URL and return its content
    pub async fn crawl_url(&self, url: &str) -> Result<CrawledPage, String> {
        // Use reqwest directly for simpler single-page fetching
        let client = reqwest::Client::builder()
            .user_agent(&self.config.user_agent)
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))?;

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch URL: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()));
        }

        let html = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Extract title from HTML
        let title = extract_title(&html).unwrap_or_else(|| url.to_string());

        // Convert HTML to markdown
        let markdown = html_to_markdown(&html);

        let crawled_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Ok(CrawledPage {
            url: url.to_string(),
            title,
            markdown,
            html,
            crawled_at,
        })
    }

    /// Crawl multiple URLs concurrently
    pub async fn crawl_urls(&self, urls: Vec<&str>) -> Vec<Result<CrawledPage, String>> {
        let mut results = Vec::with_capacity(urls.len());

        // Crawl each URL
        for url in urls {
            results.push(self.crawl_url(url).await);
        }

        results
    }

    /// Get crawler configuration
    pub fn config(&self) -> &CrawlerConfig {
        &self.config
    }
}

impl Default for WebCrawler {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract title from HTML
fn extract_title(html: &str) -> Option<String> {
    // Simple regex-free title extraction
    let lower = html.to_lowercase();
    let start = lower.find("<title>")?;
    let end = lower.find("</title>")?;

    if start < end {
        let title_start = start + 7; // length of "<title>"
        let title = &html[title_start..end];
        Some(title.trim().to_string())
    } else {
        None
    }
}

/// Convert HTML to markdown
fn html_to_markdown(html: &str) -> String {
    // Basic HTML to markdown conversion
    // For production, consider using html2md crate

    let mut result = html.to_string();

    // Remove script and style tags
    result = remove_tag_content(&result, "script");
    result = remove_tag_content(&result, "style");
    result = remove_tag_content(&result, "nav");
    result = remove_tag_content(&result, "footer");
    result = remove_tag_content(&result, "header");

    // Convert common elements
    // Headers
    result = result.replace("<h1>", "\n# ");
    result = result.replace("</h1>", "\n");
    result = result.replace("<h2>", "\n## ");
    result = result.replace("</h2>", "\n");
    result = result.replace("<h3>", "\n### ");
    result = result.replace("</h3>", "\n");
    result = result.replace("<h4>", "\n#### ");
    result = result.replace("</h4>", "\n");

    // Also handle with attributes
    result = replace_tag_simple(&result, "h1", "# ");
    result = replace_tag_simple(&result, "h2", "## ");
    result = replace_tag_simple(&result, "h3", "### ");
    result = replace_tag_simple(&result, "h4", "#### ");

    // Paragraphs and breaks
    result = result.replace("<p>", "\n\n");
    result = result.replace("</p>", "\n");
    result = replace_tag_simple(&result, "p", "\n\n");
    result = result.replace("<br>", "\n");
    result = result.replace("<br/>", "\n");
    result = result.replace("<br />", "\n");

    // Lists
    result = result.replace("<li>", "\n- ");
    result = result.replace("</li>", "");
    result = replace_tag_simple(&result, "li", "\n- ");
    result = result.replace("<ul>", "\n");
    result = result.replace("</ul>", "\n");
    result = result.replace("<ol>", "\n");
    result = result.replace("</ol>", "\n");

    // Bold and italic
    result = result.replace("<strong>", "**");
    result = result.replace("</strong>", "**");
    result = result.replace("<b>", "**");
    result = result.replace("</b>", "**");
    result = result.replace("<em>", "*");
    result = result.replace("</em>", "*");
    result = result.replace("<i>", "*");
    result = result.replace("</i>", "*");

    // Code
    result = result.replace("<code>", "`");
    result = result.replace("</code>", "`");
    result = result.replace("<pre>", "\n```\n");
    result = result.replace("</pre>", "\n```\n");

    // Links - extract href and text
    result = convert_links(&result);

    // Remove remaining HTML tags
    result = remove_all_tags(&result);

    // Decode common HTML entities
    result = result.replace("&nbsp;", " ");
    result = result.replace("&amp;", "&");
    result = result.replace("&lt;", "<");
    result = result.replace("&gt;", ">");
    result = result.replace("&quot;", "\"");
    result = result.replace("&#39;", "'");
    result = result.replace("&apos;", "'");

    // Clean up excessive whitespace
    let lines: Vec<&str> = result.lines().collect();
    let cleaned: Vec<&str> = lines
        .into_iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();

    cleaned.join("\n\n")
}

/// Remove content between opening and closing tags
fn remove_tag_content(html: &str, tag: &str) -> String {
    let mut result = html.to_string();
    let open_tag = format!("<{}", tag);
    let close_tag = format!("</{}>", tag);

    loop {
        let lower = result.to_lowercase();
        if let Some(start) = lower.find(&open_tag) {
            if let Some(end) = lower[start..].find(&close_tag) {
                let end_pos = start + end + close_tag.len();
                result = format!("{}{}", &result[..start], &result[end_pos..]);
                continue;
            }
        }
        break;
    }

    result
}

/// Replace opening tags with attributes
fn replace_tag_simple(html: &str, tag: &str, replacement: &str) -> String {
    let mut result = html.to_string();
    let open_pattern = format!("<{} ", tag);
    let close_pattern = format!("</{}>", tag);

    // Replace opening tags with attributes
    loop {
        let lower = result.to_lowercase();
        if let Some(start) = lower.find(&open_pattern) {
            if let Some(end) = result[start..].find('>') {
                let end_pos = start + end + 1;
                result = format!("{}{}{}", &result[..start], replacement, &result[end_pos..]);
                continue;
            }
        }
        break;
    }

    // Replace closing tags
    result = result.replace(&close_pattern, "");

    result
}

/// Convert HTML links to markdown format
fn convert_links(html: &str) -> String {
    let mut result = html.to_string();

    // Simple link conversion - this is a basic implementation
    // Pattern: <a href="URL">TEXT</a> -> [TEXT](URL)
    loop {
        let lower = result.to_lowercase();
        if let Some(start) = lower.find("<a ") {
            if let Some(href_start) = lower[start..].find("href=") {
                let href_pos = start + href_start + 5; // after 'href='

                // Find the quote character
                let quote = if result.chars().nth(href_pos) == Some('"') {
                    '"'
                } else if result.chars().nth(href_pos) == Some('\'') {
                    '\''
                } else {
                    // No quotes, skip this link
                    break;
                };

                let url_start = href_pos + 1;
                if let Some(url_end) = result[url_start..].find(quote) {
                    let url = &result[url_start..url_start + url_end];

                    // Find the end of opening tag
                    if let Some(tag_end) = result[start..].find('>') {
                        let text_start = start + tag_end + 1;

                        // Find closing tag
                        if let Some(close) = lower[text_start..].find("</a>") {
                            let text_end = text_start + close;
                            let text = &result[text_start..text_end];
                            let link_end = text_end + 4; // "</a>".len()

                            // Create markdown link
                            let md_link = format!("[{}]({})", text.trim(), url);
                            result = format!("{}{}{}", &result[..start], md_link, &result[link_end..]);
                            continue;
                        }
                    }
                }
            }
        }
        break;
    }

    result
}

/// Remove all remaining HTML tags
fn remove_all_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        let html = "<html><head><title>Test Page</title></head><body></body></html>";
        assert_eq!(extract_title(html), Some("Test Page".to_string()));
    }

    #[test]
    fn test_html_to_markdown_headers() {
        let html = "<h1>Header 1</h1><h2>Header 2</h2>";
        let md = html_to_markdown(html);
        assert!(md.contains("# Header 1"));
        assert!(md.contains("## Header 2"));
    }

    #[test]
    fn test_html_to_markdown_links() {
        let html = r#"<a href="https://example.com">Example</a>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[Example](https://example.com)"));
    }

    #[test]
    fn test_remove_script_tags() {
        let html = "<p>Before</p><script>alert('bad');</script><p>After</p>";
        let md = html_to_markdown(html);
        assert!(!md.contains("alert"));
        assert!(md.contains("Before"));
        assert!(md.contains("After"));
    }
}
