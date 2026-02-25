//! Web tools — fetch_url, web_search, web_fetch (Ollama cloud).
//! 1:1 port of Python tools.py web functions.

use anyhow::Result;
use reqwest;

const OLLAMA_WEB_SEARCH_URL: &str = "https://ollama.com/api/web_search";
const OLLAMA_WEB_FETCH_URL: &str = "https://ollama.com/api/web_fetch";

/// Fetch a URL and return its content (for research).
/// Simple HTML-to-text conversion, 1:1 with Python fetch_url.
pub async fn fetch_url(url: &str) -> Result<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Ok("Error: Only http and https URLs are allowed.".to_string());
    }

    let client = reqwest::Client::builder()
        .user_agent("Anemone/1.0 (research)")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    match client.get(url).send().await {
        Ok(resp) => {
            let body = resp.text().await.unwrap_or_default();
            let max_chars = 12000;

            // Simple HTML-to-text: strip scripts, styles, tags, collapse whitespace
            let text = strip_html(&body);
            let result = if text.len() > max_chars {
                format!("{}...(truncated)", &text[..max_chars])
            } else if text.is_empty() {
                if body.len() > max_chars {
                    format!("{}...(truncated)", &body[..max_chars])
                } else {
                    body
                }
            } else {
                text
            };
            Ok(result)
        }
        Err(e) => Ok(format!("Error fetching URL: {}", e)),
    }
}

/// Strip HTML tags and normalize whitespace.
fn strip_html(html: &str) -> String {
    // Remove script and style blocks
    let mut text = html.to_string();

    // Remove <script>...</script>
    while let Some(start) = text.to_lowercase().find("<script") {
        if let Some(end) = text.to_lowercase()[start..].find("</script>") {
            text = format!("{}{}", &text[..start], &text[start + end + 9..]);
        } else {
            break;
        }
    }

    // Remove <style>...</style>
    while let Some(start) = text.to_lowercase().find("<style") {
        if let Some(end) = text.to_lowercase()[start..].find("</style>") {
            text = format!("{}{}", &text[..start], &text[start + end + 8..]);
        } else {
            break;
        }
    }

    // Strip all HTML tags
    let mut result = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
            result.push(' ');
        } else if !in_tag {
            result.push(ch);
        }
    }

    // Collapse whitespace
    let mut collapsed = String::with_capacity(result.len());
    let mut last_was_space = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                collapsed.push(' ');
                last_was_space = true;
            }
        } else {
            collapsed.push(ch);
            last_was_space = false;
        }
    }

    collapsed.trim().to_string()
}

/// Call Ollama cloud web search API.
pub async fn ollama_web_search(
    query: &str,
    max_results: usize,
    api_key: Option<&str>,
) -> Result<String> {
    let api_key = match api_key {
        Some(k) => k.to_string(),
        None => {
            return Ok("Error: OLLAMA_API_KEY is required for web search. Get one at https://ollama.com/settings/keys".to_string());
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let body = serde_json::json!({
        "query": query,
        "max_results": max_results.min(10),
    });

    match client
        .post(OLLAMA_WEB_SEARCH_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let data: serde_json::Value = resp.json().await.unwrap_or_default();
            let results = data
                .get("results")
                .and_then(|r| r.as_array())
                .cloned()
                .unwrap_or_default();

            let mut lines = Vec::new();
            for r in results {
                let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let content = r.get("content").and_then(|v| v.as_str()).unwrap_or("");
                lines.push(format!("**{}**", title));
                lines.push(format!("URL: {}", url));
                let truncated: String = content.chars().take(2000).collect();
                lines.push(truncated);
                lines.push(String::new());
            }

            let result = lines.join("\n");
            let result: String = result.chars().take(8000).collect();
            if result.trim().is_empty() {
                Ok("No results found.".to_string())
            } else {
                Ok(result)
            }
        }
        Err(e) => Ok(format!("Error: {}", e)),
    }
}

/// Call Ollama cloud web fetch API.
pub async fn ollama_web_fetch(url: &str, api_key: Option<&str>) -> Result<String> {
    let api_key = match api_key {
        Some(k) => k.to_string(),
        None => {
            return Ok("Error: OLLAMA_API_KEY is required for web fetch. Get one at https://ollama.com/settings/keys".to_string());
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let body = serde_json::json!({ "url": url });

    match client
        .post(OLLAMA_WEB_FETCH_URL)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let data: serde_json::Value = resp.json().await.unwrap_or_default();
            let title = data.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let truncated: String = content.chars().take(6000).collect();
            if title.is_empty() {
                Ok(truncated)
            } else {
                Ok(format!("**{}**\n\n{}", title, truncated))
            }
        }
        Err(e) => Ok(format!("Error: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_basic() {
        let html = "<p>Hello <b>world</b></p>";
        let text = strip_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
        assert!(!text.contains("<"));
    }

    #[test]
    fn test_strip_html_scripts() {
        let html = "<p>before</p><script>evil()</script><p>after</p>";
        let text = strip_html(html);
        assert!(text.contains("before"));
        assert!(text.contains("after"));
        assert!(!text.contains("evil"));
    }

    #[test]
    fn test_strip_html_styles() {
        let html = "<p>text</p><style>.x{color:red}</style><p>more</p>";
        let text = strip_html(html);
        assert!(text.contains("text"));
        assert!(text.contains("more"));
        assert!(!text.contains("color"));
    }
}
