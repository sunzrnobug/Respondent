use std::fmt;
use std::io::{BufRead, BufReader};
use std::sync::Arc;

use serde_json::{json, Value};

use super::client::{LlmError, ReplyGeneration, ReplyRequest, StreamingReplyClient};
use super::prompt::build_system_prompt;
use super::streaming::{spawn_streaming_reply, truncate_for_error, ReplyChunk, SseValueStream};

const DEFAULT_OPENAI_REPLY_MODEL: &str = "gpt-5.4-mini";
const RESPONSES_URL: &str = "https://api.openai.com/v1/responses";

#[derive(Clone, PartialEq, Eq)]
pub struct OpenAiReplyConfig {
    pub api_key: String,
    pub model: String,
}

impl fmt::Debug for OpenAiReplyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiReplyConfig")
            .field("api_key", &"<redacted>")
            .field("model", &self.model)
            .finish()
    }
}

impl OpenAiReplyConfig {
    pub fn from_api_key(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: std::env::var("OPENAI_LLM_MODEL")
                .ok()
                .filter(|model| !model.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_OPENAI_REPLY_MODEL.to_string()),
        }
    }
}

pub trait ResponsesTransport: Send + Sync {
    fn stream(
        &self,
        config: &OpenAiReplyConfig,
        request: &ReplyRequest,
    ) -> Result<Box<dyn ResponsesEventStream>, LlmError>;
}

pub trait ResponsesEventStream: Send {
    fn next_event(&mut self) -> Result<Option<Value>, LlmError>;
}

pub struct ReqwestResponsesTransport {
    client: reqwest::blocking::Client,
}

impl Default for ReqwestResponsesTransport {
    fn default() -> Self {
        Self {
            client: super::streaming::build_streaming_http_client(),
        }
    }
}

impl ResponsesTransport for ReqwestResponsesTransport {
    fn stream(
        &self,
        config: &OpenAiReplyConfig,
        request: &ReplyRequest,
    ) -> Result<Box<dyn ResponsesEventStream>, LlmError> {
        let response = self
            .client
            .post(RESPONSES_URL)
            .bearer_auth(&config.api_key)
            .json(&build_responses_body(config, request))
            .send()
            .map_err(|err| LlmError::Provider(format!("openai responses request: {err}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(LlmError::Provider(format!(
                "openai responses http {status}: {}",
                truncate_for_error(&body)
            )));
        }

        Ok(Box::new(SseResponsesEventStream {
            reader: BufReader::new(response),
        }))
    }
}

struct SseResponsesEventStream {
    reader: BufReader<reqwest::blocking::Response>,
}

impl ResponsesEventStream for SseResponsesEventStream {
    fn next_event(&mut self) -> Result<Option<Value>, LlmError> {
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = self
                .reader
                .read_line(&mut line)
                .map_err(|err| LlmError::Provider(format!("openai responses read: {err}")))?;
            if bytes == 0 {
                return Ok(None);
            }

            let trimmed = line.trim();
            let Some(data) = trimmed.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                return Ok(None);
            }

            let value = serde_json::from_str(data)
                .map_err(|err| LlmError::Provider(format!("openai responses json: {err}")))?;
            return Ok(Some(value));
        }
    }
}

pub struct OpenAiReplyClient {
    config: OpenAiReplyConfig,
    transport: Arc<dyn ResponsesTransport>,
}

impl OpenAiReplyClient {
    pub fn connect(config: OpenAiReplyConfig) -> Result<Self, LlmError> {
        Self::with_transport(config, Arc::new(ReqwestResponsesTransport::default()))
    }

    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| LlmError::Provider("missing OPENAI_API_KEY".to_string()))?;
        Self::connect(OpenAiReplyConfig::from_api_key(api_key))
    }

    pub fn with_transport(
        config: OpenAiReplyConfig,
        transport: Arc<dyn ResponsesTransport>,
    ) -> Result<Self, LlmError> {
        if config.api_key.trim().is_empty() {
            return Err(LlmError::Provider("missing OPENAI_API_KEY".to_string()));
        }

        Ok(Self { config, transport })
    }
}

/// Adapter so a `ResponsesEventStream` is usable as an `SseValueStream`.
struct ResponsesValueStream(Box<dyn ResponsesEventStream>);

impl SseValueStream for ResponsesValueStream {
    fn next_value(&mut self) -> Result<Option<Value>, LlmError> {
        self.0.next_event()
    }
}

impl StreamingReplyClient for OpenAiReplyClient {
    fn name(&self) -> &'static str {
        "openai-responses-llm"
    }

    fn start(&self, request: ReplyRequest) -> Box<dyn ReplyGeneration> {
        let config = self.config.clone();
        let transport = Arc::clone(&self.transport);
        let open = {
            let request = request.clone();
            move || -> Result<Box<dyn SseValueStream>, LlmError> {
                let stream = transport.stream(&config, &request)?;
                Ok(Box::new(ResponsesValueStream(stream)))
            }
        };
        spawn_streaming_reply(request, open, responses_map)
    }
}

fn responses_map(value: &Value) -> ReplyChunk {
    match value["type"].as_str() {
        Some("response.output_text.delta") => match value["delta"].as_str() {
            Some(delta) => ReplyChunk::Token(delta.to_string()),
            None => ReplyChunk::Ignore,
        },
        Some("response.completed") => ReplyChunk::Complete,
        Some("response.error") | Some("error") => ReplyChunk::Error,
        _ => ReplyChunk::Ignore,
    }
}

pub fn build_responses_body(config: &OpenAiReplyConfig, request: &ReplyRequest) -> Value {
    let user_content = match &request.document_context {
        Some(doc_ctx) => format!(
            "Reference documents (factual background only):\n---\n{}\n---\n\nConversation context:\n{}\n\nCurrent turn:\n{}\n\nWrite the suggested reply only.",
            doc_ctx,
            format_context(&request.context),
            request.transcript
        ),
        None => format!(
            "Conversation context:\n{}\n\nCurrent turn:\n{}\n\nWrite the suggested reply only.",
            format_context(&request.context),
            request.transcript
        ),
    };
    json!({
        "model": config.model,
        "stream": true,
        "input": [
            {"role": "system", "content": build_system_prompt(request.reply_style.as_ref())},
            {"role": "user", "content": user_content}
        ]
    })
}

pub fn format_context(context: &[String]) -> String {
    if context.is_empty() {
        return "(none)".to_string();
    }

    context
        .iter()
        .map(|turn| format!("- {turn}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::super::streaming::truncate_for_error;

    #[test]
    fn truncate_for_error_handles_unicode_boundaries() {
        let text = format!("a{}", "测".repeat(100));
        let truncated = truncate_for_error(&text);

        assert!(truncated.ends_with("..."));
        assert!(truncated.len() < text.len());
    }
}
