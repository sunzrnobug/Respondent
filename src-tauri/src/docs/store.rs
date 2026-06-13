use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::bm25::{bm25_field_score, tokenize, RetrievedChunk};
use super::chunk::{parse_markdown_chunks, Chunk};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub name: String,
    pub chunk_count: usize,
    pub char_count: usize,
}

/// In-memory BM25-indexed store for Markdown document chunks.
/// `load` with the same name replaces the existing document (overwrite semantics).
#[derive(Default)]
pub struct DocumentStore {
    chunks: Vec<Chunk>,
    text_tokens: Vec<Vec<String>>,
    heading_tokens: Vec<Vec<String>>,
    docname_tokens: Vec<Vec<String>>,
    idf: HashMap<String, f32>,
    avg_text_len: f32,
}

impl DocumentStore {
    pub fn load(&mut self, name: String, content: String) -> DocumentSummary {
        self.unload(&name);
        let char_count = content.chars().count();
        let new_chunks = parse_markdown_chunks(&name, &content);
        let chunk_count = new_chunks.len();

        for chunk in new_chunks {
            self.text_tokens.push(tokenize(&chunk.text));
            self.heading_tokens.push(tokenize(&chunk.heading_path));
            self.docname_tokens.push(tokenize(&chunk.doc_name));
            self.chunks.push(chunk);
        }

        self.rebuild_index();
        DocumentSummary {
            name,
            chunk_count,
            char_count,
        }
    }

    pub fn unload(&mut self, name: &str) {
        let remove: Vec<usize> = self
            .chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| c.doc_name == name)
            .map(|(i, _)| i)
            .collect();
        for i in remove.iter().rev() {
            self.chunks.remove(*i);
            self.text_tokens.remove(*i);
            self.heading_tokens.remove(*i);
            self.docname_tokens.remove(*i);
        }
        self.rebuild_index();
    }

    pub fn list(&self) -> Vec<DocumentSummary> {
        let mut seen: Vec<&str> = Vec::new();
        let mut result = Vec::new();
        for chunk in &self.chunks {
            let name = chunk.doc_name.as_str();
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            let chunk_count = self.chunks.iter().filter(|c| c.doc_name == name).count();
            let char_count = self
                .chunks
                .iter()
                .filter(|c| c.doc_name == name)
                .map(|c| c.text.chars().count())
                .sum();
            result.push(DocumentSummary {
                name: name.to_string(),
                chunk_count,
                char_count,
            });
        }
        result
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn query(&self, text: &str, top_k: usize) -> Vec<RetrievedChunk> {
        if self.chunks.is_empty() {
            return Vec::new();
        }
        let query_terms = tokenize(text);
        if query_terms.is_empty() {
            return Vec::new();
        }

        let avg_h = avg_len(&self.heading_tokens);
        let avg_d = avg_len(&self.docname_tokens);

        let mut scored: Vec<(usize, f32)> = self
            .chunks
            .iter()
            .enumerate()
            .filter_map(|(i, _)| {
                let s = bm25_field_score(
                    &query_terms,
                    &self.text_tokens[i],
                    &self.idf,
                    self.avg_text_len,
                ) + 1.5
                    * bm25_field_score(
                        &query_terms,
                        &self.heading_tokens[i],
                        &self.idf,
                        avg_h,
                    )
                    + 0.5
                        * bm25_field_score(
                            &query_terms,
                            &self.docname_tokens[i],
                            &self.idf,
                            avg_d,
                        );
                if s > 0.0 {
                    Some((i, s))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_unstable_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        scored
            .iter()
            .take(top_k)
            .map(|(i, score)| {
                let c = &self.chunks[*i];
                RetrievedChunk {
                    doc_name: c.doc_name.clone(),
                    heading_path: c.heading_path.clone(),
                    text: c.text.clone(),
                    score: *score,
                }
            })
            .collect()
    }

    fn rebuild_index(&mut self) {
        let n = self.chunks.len() as f32;
        if n == 0.0 {
            self.idf = HashMap::new();
            self.avg_text_len = 1.0;
            return;
        }

        let mut df: HashMap<String, usize> = HashMap::new();
        for tokens in &self.text_tokens {
            let unique: HashSet<&String> = tokens.iter().collect();
            for term in unique {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
        }

        self.idf = df
            .iter()
            .map(|(term, &d)| {
                let idf = ((n - d as f32 + 0.5) / (d as f32 + 0.5) + 1.0).ln().max(0.0);
                (term.clone(), idf)
            })
            .collect();

        self.avg_text_len = self.text_tokens.iter().map(|t| t.len() as f32).sum::<f32>() / n;
    }
}

fn avg_len(token_lists: &[Vec<String>]) -> f32 {
    if token_lists.is_empty() {
        return 1.0;
    }
    token_lists.iter().map(|t| t.len() as f32).sum::<f32>() / token_lists.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with_two_docs() -> DocumentStore {
        let mut s = DocumentStore::default();
        s.load(
            "auth.md".into(),
            "## API Authentication\nUse Bearer token in the Authorization header. The accessToken expires after 1 hour and must be refreshed periodically.\n".into(),
        );
        s.load(
            "install.md".into(),
            "## Installation\nRequires Python 3.10 or newer. Run pip install respondent to get started with the local development environment.\n".into(),
        );
        s
    }

    #[test]
    fn query_returns_relevant_chunk() {
        let s = store_with_two_docs();
        let results = s.query("Bearer token Authorization authentication", 3);
        assert!(!results.is_empty(), "expected at least one result");
        assert_eq!(results[0].doc_name, "auth.md");
    }

    #[test]
    fn query_no_match_returns_empty() {
        let s = store_with_two_docs();
        assert!(s.query("zzzunrelatedxxx", 3).is_empty());
    }

    #[test]
    fn same_name_overwrite() {
        let mut s = DocumentStore::default();
        s.load(
            "a.md".into(),
            "## Old\nOld text that should disappear completely after the document is overwritten with new searchable content.\n".into(),
        );
        s.load(
            "a.md".into(),
            "## New\nNew text searchable after overwrite with enough characters to pass the minimum chunk body length filter.\n".into(),
        );
        assert_eq!(s.list().len(), 1);
        let old_results = s.query("old text disappear", 3);
        assert!(
            old_results.is_empty() || !old_results[0].text.contains("Old text"),
            "old content still present: {:?}",
            old_results.first().map(|r| &r.text)
        );
        assert!(!s.query("new text searchable", 3).is_empty());
    }

    #[test]
    fn unload_removes_document() {
        let mut s = store_with_two_docs();
        s.unload("auth.md");
        assert_eq!(s.list().len(), 1);
        assert_eq!(s.list()[0].name, "install.md");
        assert!(s.query("bearer token authentication", 3).is_empty());
    }

    #[test]
    fn is_empty_reflects_load_unload() {
        let mut s = DocumentStore::default();
        assert!(s.is_empty());
        s.load(
            "a.md".into(),
            "## Sec\nContent that is long enough to keep in the index and pass the minimum chunk body length filter.\n".into(),
        );
        assert!(!s.is_empty());
        s.unload("a.md");
        assert!(s.is_empty());
    }
}
