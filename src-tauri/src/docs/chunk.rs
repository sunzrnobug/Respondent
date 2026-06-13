use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_CHUNK_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: usize,
    pub doc_name: String,
    pub heading_path: String,
    pub text: String,
    pub token_count_estimate: usize,
}

impl Chunk {
    fn new(doc_name: &str, heading_path: String, text: String) -> Self {
        let char_count = text.chars().count();
        Chunk {
            id: NEXT_CHUNK_ID.fetch_add(1, Ordering::Relaxed),
            doc_name: doc_name.to_string(),
            heading_path,
            text,
            token_count_estimate: (char_count / 3).max(1),
        }
    }
}

fn flush_chunk(
    chunks: &mut Vec<Chunk>,
    doc_name: &str,
    h1: &str,
    h2: &str,
    h3: &str,
    current_text: &mut String,
) {
    let body = current_text.trim();
    if body.chars().count() < 50 {
        current_text.clear();
        return;
    }
    let mut parts: Vec<&str> = Vec::new();
    if !h1.is_empty() {
        parts.push(h1);
    }
    if !h2.is_empty() {
        parts.push(h2);
    }
    if !h3.is_empty() {
        parts.push(h3);
    }
    let heading_path = parts.join(" > ");
    chunks.push(Chunk::new(doc_name, heading_path, body.to_string()));
    current_text.clear();
}

/// Parse Markdown into semantic chunks split at H2/H3 headings.
/// Code fences (``` or ~~~) protect content from being split internally.
pub fn parse_markdown_chunks(doc_name: &str, content: &str) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut h1 = String::new();
    let mut h2 = String::new();
    let mut h3 = String::new();
    let mut current_text = String::new();
    let mut in_code_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            current_text.push_str(line);
            current_text.push('\n');
            continue;
        }

        if in_code_block {
            current_text.push_str(line);
            current_text.push('\n');
            continue;
        }

        if trimmed.starts_with("# ") || trimmed == "#" {
            let rest = trimmed.trim_start_matches('#').trim();
            flush_chunk(&mut chunks, doc_name, &h1, &h2, &h3, &mut current_text);
            h1 = rest.to_string();
            h2.clear();
            h3.clear();
            continue;
        }

        if trimmed.starts_with("## ") || trimmed == "##" {
            let rest = trimmed.trim_start_matches('#').trim();
            flush_chunk(&mut chunks, doc_name, &h1, &h2, &h3, &mut current_text);
            h2 = rest.to_string();
            h3.clear();
            continue;
        }

        if trimmed.starts_with("### ") || trimmed == "###" {
            let rest = trimmed.trim_start_matches('#').trim();
            flush_chunk(&mut chunks, doc_name, &h1, &h2, &h3, &mut current_text);
            h3 = rest.to_string();
            continue;
        }

        if trimmed.is_empty() && current_text.chars().count() > 800 {
            flush_chunk(&mut chunks, doc_name, &h1, &h2, &h3, &mut current_text);
        }

        current_text.push_str(line);
        current_text.push('\n');
    }

    flush_chunk(&mut chunks, doc_name, &h1, &h2, &h3, &mut current_text);
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_h2() {
        let md = "# Doc\n## Section A\nContent A here with enough characters to pass the minimum chunk body length filter.\n## Section B\nContent B here with enough characters to pass the minimum chunk body length filter.\n";
        let chunks = parse_markdown_chunks("test.md", md);
        assert_eq!(chunks.len(), 2);
        assert!(
            chunks[0].heading_path.contains("Section A"),
            "got: {}",
            chunks[0].heading_path
        );
        assert!(chunks[1].heading_path.contains("Section B"));
    }

    #[test]
    fn heading_path_inherits_h1_h2_h3() {
        let md = "# Product Docs\n## API\n### Auth\nText about authentication with enough characters to pass the minimum chunk body length filter.\n";
        let chunks = parse_markdown_chunks("test.md", md);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading_path, "Product Docs > API > Auth");
    }

    #[test]
    fn code_block_not_split_at_blank_line() {
        let preamble = "Some context.\n".repeat(15);
        let md = format!(
            "## Section\n{}\n```rust\nfn foo() {{\n\n    let x = 1;\n}}\n```\nMore text.\n",
            preamble
        );
        let chunks = parse_markdown_chunks("test.md", &md);
        let all_text: String = chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all_text.contains("fn foo()"), "code block must be present");
        for chunk in &chunks {
            let fences = chunk
                .text
                .lines()
                .filter(|l| l.trim().starts_with("```") || l.trim().starts_with("~~~"))
                .count();
            assert_eq!(fences % 2, 0, "unbalanced fences in chunk: {}", chunk.text);
        }
    }

    #[test]
    fn filters_short_chunks() {
        let md = "## Title\n\n## Another\nActual content here is long enough to keep in the parsed chunk output.\n";
        let chunks = parse_markdown_chunks("test.md", md);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("Actual content"));
    }

    #[test]
    fn token_count_estimate_is_nonzero() {
        let md = "## Section\nSome content that is definitely longer than fifty characters total.\n";
        let chunks = parse_markdown_chunks("test.md", md);
        assert!(!chunks.is_empty());
        assert!(chunks[0].token_count_estimate > 0);
    }
}
