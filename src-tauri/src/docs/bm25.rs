use std::collections::HashMap;

/// Returns sorted, deduplicated tokens.
/// - CJK: unigrams + consecutive bigrams
/// - ASCII words: whole word (lowercased), camelCase parts, snake_case parts + joined form
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut ascii_buf = String::new();
    let mut prev_cjk: Option<char> = None;

    for ch in text.chars() {
        if is_cjk(ch) {
            if !ascii_buf.is_empty() {
                tokenize_ascii_word(&ascii_buf, &mut tokens);
                ascii_buf.clear();
            }
            tokens.push(ch.to_string());
            if let Some(prev) = prev_cjk {
                tokens.push(format!("{prev}{ch}"));
            }
            prev_cjk = Some(ch);
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            prev_cjk = None;
            ascii_buf.push(ch);
        } else {
            prev_cjk = None;
            if !ascii_buf.is_empty() {
                tokenize_ascii_word(&ascii_buf, &mut tokens);
                ascii_buf.clear();
            }
        }
    }
    if !ascii_buf.is_empty() {
        tokenize_ascii_word(&ascii_buf, &mut tokens);
    }

    tokens.sort_unstable();
    tokens.dedup();
    tokens
}

fn tokenize_ascii_word(word: &str, tokens: &mut Vec<String>) {
    if word.is_empty() {
        return;
    }
    let lower = word.to_ascii_lowercase();
    tokens.push(lower.clone());

    let snake_parts: Vec<&str> = lower.split('_').filter(|s| !s.is_empty()).collect();
    if snake_parts.len() > 1 {
        for p in &snake_parts {
            tokens.push((*p).to_string());
        }
        tokens.push(snake_parts.concat());
    }

    let camel_parts = split_camel_case(word);
    if camel_parts.len() > 1 {
        tokens.extend(camel_parts);
    }
}

fn split_camel_case(s: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if i > 0 && ch.is_uppercase() && chars[i - 1].is_lowercase() {
            if !current.is_empty() {
                parts.push(current.to_lowercase());
                current.clear();
            }
        }
        current.push(ch);
    }
    if !current.is_empty() {
        parts.push(current.to_lowercase());
    }
    parts
}

fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}'
    )
}

const K1: f32 = 1.5;
const B: f32 = 0.75;

#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub doc_name: String,
    pub heading_path: String,
    pub text: String,
    pub score: f32,
}

/// BM25 score of one field (pre-tokenized) against the query terms.
pub fn bm25_field_score(
    query_terms: &[String],
    field_tokens: &[String],
    idf: &HashMap<String, f32>,
    avg_len: f32,
) -> f32 {
    let field_len = field_tokens.len() as f32;
    let mut score = 0.0_f32;
    for term in query_terms {
        let Some(&idf_val) = idf.get(term) else {
            continue;
        };
        let tf = field_tokens.iter().filter(|t| *t == term).count() as f32;
        let denom = tf + K1 * (1.0 - B + B * field_len / avg_len.max(1.0));
        score += idf_val * (tf * (K1 + 1.0)) / denom;
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_unigram_and_bigram() {
        let tokens = tokenize("接口认证");
        assert!(tokens.contains(&"接".to_string()));
        assert!(tokens.contains(&"认".to_string()));
        assert!(
            tokens.contains(&"接口".to_string()),
            "missing bigram: {tokens:?}"
        );
        assert!(tokens.contains(&"口认".to_string()));
        assert!(tokens.contains(&"认证".to_string()));
    }

    #[test]
    fn camel_case_split() {
        let tokens = tokenize("accessToken");
        assert!(tokens.contains(&"accesstoken".to_string()));
        assert!(
            tokens.contains(&"access".to_string()),
            "missing 'access': {tokens:?}"
        );
        assert!(tokens.contains(&"token".to_string()));
    }

    #[test]
    fn snake_case_split() {
        let tokens = tokenize("api_key");
        assert!(tokens.contains(&"api_key".to_string()));
        assert!(tokens.contains(&"api".to_string()));
        assert!(tokens.contains(&"key".to_string()));
        assert!(tokens.contains(&"apikey".to_string()));
    }

    #[test]
    fn mixed_chinese_english() {
        let tokens = tokenize("Bearer accessToken 认证");
        assert!(tokens.contains(&"bearer".to_string()));
        assert!(tokens.contains(&"access".to_string()));
        assert!(tokens.contains(&"认".to_string()));
        assert!(tokens.contains(&"认证".to_string()));
    }

    #[test]
    fn bm25_zero_score_on_missing_term() {
        let idf: HashMap<String, f32> = HashMap::new();
        let score = bm25_field_score(
            &["nonexistent".to_string()],
            &["token".to_string()],
            &idf,
            10.0,
        );
        assert_eq!(score, 0.0);
    }

    #[test]
    fn bm25_nonzero_on_matching_term() {
        let mut idf = HashMap::new();
        idf.insert("token".to_string(), 1.0_f32);
        let field_tokens = tokenize("use the accessToken for Bearer auth");
        let score = bm25_field_score(
            &["token".to_string()],
            &field_tokens,
            &idf,
            field_tokens.len() as f32,
        );
        assert!(score > 0.0, "expected nonzero score");
    }
}
