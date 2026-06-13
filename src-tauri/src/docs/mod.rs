pub mod bm25;
pub mod chunk;
pub mod store;

pub use bm25::RetrievedChunk;
pub use store::{DocumentStore, DocumentSummary};

const MAX_DOC_CONTEXT_CHARS: usize = 3000;

/// Formats retrieved chunks into prompt-ready document context, capped by character count.
pub fn format_document_context(chunks: &[RetrievedChunk]) -> Option<String> {
    if chunks.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    let mut total = 0usize;
    for chunk in chunks {
        let entry = format!(
            "Source: {}\nSection: {}\n{}",
            chunk.doc_name, chunk.heading_path, chunk.text
        );
        let entry_chars = entry.chars().count();
        if total + entry_chars > MAX_DOC_CONTEXT_CHARS {
            break;
        }
        total += entry_chars;
        parts.push(entry);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n---\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(text: &str) -> RetrievedChunk {
        RetrievedChunk {
            doc_name: "doc.md".into(),
            heading_path: "Section".into(),
            text: text.into(),
            score: 1.0,
        }
    }

    #[test]
    fn format_document_context_empty_chunks_returns_none() {
        assert!(format_document_context(&[]).is_none());
    }

    #[test]
    fn format_document_context_uses_char_count_not_bytes() {
        // 2000 CJK chars = 6000 UTF-8 bytes; a byte budget of 3000 would reject this entirely.
        let result = format_document_context(&[chunk(&"测".repeat(2000))]);
        let ctx = result.expect("2000 Chinese chars should fit under 3000 char budget");
        assert!(ctx.chars().count() <= MAX_DOC_CONTEXT_CHARS);
    }

    #[test]
    fn format_document_context_truncates_across_multiple_chunks() {
        let chunks: Vec<_> = (0..5)
            .map(|i| chunk(&format!("段落{}内容{}", i, "文".repeat(800))))
            .collect();
        let result = format_document_context(&chunks).expect("at least one chunk fits");
        assert!(result.chars().count() <= MAX_DOC_CONTEXT_CHARS);
        assert!(result.contains("段落0"));
    }
}
