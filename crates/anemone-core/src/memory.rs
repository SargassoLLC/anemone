//! Smallville-inspired memory stream with three-factor retrieval.
//! 1:1 port of Python memory.py.

use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{error, info};

use crate::config::Config;
use crate::prompts::IMPORTANCE_PROMPT;
use crate::providers;
use crate::types::Memory;

const STREAM_FILENAME: &str = "memory_stream.jsonl";

/// Pure cosine similarity — no numpy/nalgebra needed.
fn cosine_sim(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Append-only memory stream with recency x importance x relevance retrieval.
pub struct MemoryStream {
    pub path: PathBuf,
    pub memories: Vec<Memory>,
    pub importance_sum: f64,
    next_id: u32,
    config: Config,
}

impl MemoryStream {
    /// Create a new MemoryStream, loading existing memories from JSONL on disk.
    pub fn new(environment_path: &Path, config: Config) -> Self {
        let path = environment_path.join(STREAM_FILENAME);
        let mut stream = Self {
            path,
            memories: Vec::new(),
            importance_sum: 0.0,
            next_id: 0,
            config,
        };
        stream.load();
        stream
    }

    fn load(&mut self) {
        if !self.path.is_file() {
            return;
        }
        match std::fs::read_to_string(&self.path) {
            Ok(content) => {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Memory>(line) {
                        Ok(mem) => self.memories.push(mem),
                        Err(e) => error!("Failed to parse memory line: {}", e),
                    }
                }
                if let Some(max_id) = self
                    .memories
                    .iter()
                    .filter_map(|m| m.id.strip_prefix("m_").and_then(|s| s.parse::<u32>().ok()))
                    .max()
                {
                    self.next_id = max_id + 1;
                }
                info!("Loaded {} memories from stream", self.memories.len());
            }
            Err(e) => error!("Failed to load memory stream: {}", e),
        }
    }

    /// Score importance via LLM. Returns 1-10.
    async fn score_importance(&self, content: &str) -> i32 {
        let input = vec![serde_json::json!({"role": "user", "content": content})];
        match providers::chat_short(&self.config, &input, Some(IMPORTANCE_PROMPT)).await {
            Ok(result) => {
                // Extract the first integer from the response
                if let Some(num) = result
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<i32>()
                    .ok()
                {
                    num.clamp(1, 10)
                } else {
                    5
                }
            }
            Err(e) => {
                error!("Importance scoring failed: {}", e);
                5 // default to middle
            }
        }
    }

    /// Add a memory entry. Scores importance and computes embedding via LLM.
    pub async fn add(
        &mut self,
        content: &str,
        kind: &str,
        depth: i32,
        references: Vec<String>,
    ) -> Result<Memory> {
        let importance = self.score_importance(content).await;

        let embedding = match providers::embed(&self.config, content).await {
            Ok(emb) => emb,
            Err(e) => {
                error!("Embedding failed: {}", e);
                Vec::new()
            }
        };

        let entry = Memory {
            id: format!("m_{:04}", self.next_id),
            timestamp: chrono::Utc::now().to_rfc3339(),
            kind: kind.to_string(),
            content: content.to_string(),
            importance,
            depth,
            references,
            embedding,
        };

        self.memories.push(entry.clone());
        self.next_id += 1;
        self.importance_sum += importance as f64;

        if let Err(e) = self.append_to_file(&entry) {
            error!("Failed to write memory: {}", e);
        }

        info!(
            "Memory {}: importance={}, kind={}",
            entry.id, importance, kind
        );
        Ok(entry)
    }

    fn append_to_file(&self, entry: &Memory) -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(entry)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Three-factor retrieval: recency x importance x relevance.
    pub async fn retrieve(&self, query: &str, top_k: Option<usize>) -> Vec<&Memory> {
        let top_k = top_k.unwrap_or(self.config.memory_retrieval_count);

        if self.memories.is_empty() {
            return Vec::new();
        }

        // Embed the query for relevance scoring
        let query_embedding = match providers::embed(&self.config, query).await {
            Ok(emb) => emb,
            Err(e) => {
                error!("Query embedding failed: {}", e);
                // Fallback to recent memories
                let start = self.memories.len().saturating_sub(top_k);
                return self.memories[start..].iter().collect();
            }
        };

        let decay_rate = self.config.recency_decay_rate;
        let now = chrono::Utc::now();

        let mut scored: Vec<(f64, &Memory)> = self
            .memories
            .iter()
            .map(|mem| {
                // Recency score
                let hours_ago =
                    chrono::DateTime::parse_from_rfc3339(&mem.timestamp)
                        .map(|t| {
                            (now - t.with_timezone(&chrono::Utc)).num_seconds() as f64 / 3600.0
                        })
                        .unwrap_or(1000.0);
                let recency = (-(1.0 - decay_rate) * hours_ago).exp();

                // Importance score (normalized 0-1)
                let importance = mem.importance as f64 / 10.0;

                // Relevance score (cosine similarity)
                let relevance = if !mem.embedding.is_empty() && !query_embedding.is_empty() {
                    cosine_sim(&query_embedding, &mem.embedding)
                } else {
                    0.0
                };

                let score = recency + importance + relevance;
                (score, mem)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(top_k).map(|(_, mem)| mem).collect()
    }

    /// Synchronous retrieve (uses pre-computed embeddings only, no new embedding call).
    pub fn retrieve_sync(&self, _query: &str, top_k: Option<usize>) -> Vec<&Memory> {
        let top_k = top_k.unwrap_or(self.config.memory_retrieval_count);

        if self.memories.is_empty() {
            return Vec::new();
        }

        let decay_rate = self.config.recency_decay_rate;
        let now = chrono::Utc::now();

        let mut scored: Vec<(f64, &Memory)> = self
            .memories
            .iter()
            .map(|mem| {
                let hours_ago =
                    chrono::DateTime::parse_from_rfc3339(&mem.timestamp)
                        .map(|t| {
                            (now - t.with_timezone(&chrono::Utc)).num_seconds() as f64 / 3600.0
                        })
                        .unwrap_or(1000.0);
                let recency = (-(1.0 - decay_rate) * hours_ago).exp();
                let importance = mem.importance as f64 / 10.0;
                let score = recency + importance;
                (score, mem)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(top_k).map(|(_, mem)| mem).collect()
    }

    pub fn should_reflect(&self) -> bool {
        self.importance_sum >= self.config.reflection_threshold
    }

    pub fn reset_importance_sum(&mut self) {
        self.importance_sum = 0.0;
    }

    pub fn get_recent(&self, n: usize, kind: Option<&str>) -> Vec<&Memory> {
        let filtered: Vec<&Memory> = if let Some(k) = kind {
            self.memories.iter().filter(|m| m.kind == k).collect()
        } else {
            self.memories.iter().collect()
        };
        let start = filtered.len().saturating_sub(n);
        filtered[start..].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_sim_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        assert!((cosine_sim(&a, &b) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_sim_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_sim(&a, &b).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_sim_empty() {
        assert_eq!(cosine_sim(&[], &[]), 0.0);
    }
}
