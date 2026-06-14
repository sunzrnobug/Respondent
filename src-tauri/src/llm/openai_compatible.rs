use std::sync::Arc;

use serde_json::{json, Value};

use super::client::{LlmError, ReplyGeneration, ReplyRequest, StreamingReplyClient};
use super::openai_responses::format_context;
use super::prompt::build_system_prompt;
use super::streaming::{spawn_streaming_reply, ReplyChunk, ReqwestSseStream, SseValueStream};

#[derive(Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub fn join_chat_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

pub fn build_chat_body(config: &ProviderConfig, request: &ReplyRequest) -> Value {
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
        "messages": [
            {"role": "system", "content": build_system_prompt(request.reply_style.as_ref())},
            {"role": "user", "content": user_content}
        ]
    })
}

/// Map one Chat Completions SSE value to an engine action, tolerating provider
/// quirks (missing/null content, reasoning_content, empty-choices usage chunk).
pub fn chat_map(value: &Value) -> ReplyChunk {
    if value.get("error").is_some() {
        return ReplyChunk::Error;
    }
    match value["choices"][0]["delta"]["content"].as_str() {
        Some(content) if !content.is_empty() => ReplyChunk::Token(content.to_string()),
        _ => ReplyChunk::Ignore,
    }
}

pub trait ChatTransport: Send + Sync {
    fn stream(
        &self,
        config: &ProviderConfig,
        request: &ReplyRequest,
    ) -> Result<Box<dyn SseValueStream>, LlmError>;
}

pub struct ReqwestChatTransport {
    client: reqwest::blocking::Client,
}

impl Default for ReqwestChatTransport {
    fn default() -> Self {
        Self {
            client: super::streaming::build_streaming_http_client(),
        }
    }
}

impl ChatTransport for ReqwestChatTransport {
    fn stream(
        &self,
        config: &ProviderConfig,
        request: &ReplyRequest,
    ) -> Result<Box<dyn SseValueStream>, LlmError> {
        let response = self
            .client
            .post(join_chat_url(&config.base_url))
            .bearer_auth(&config.api_key)
            .json(&build_chat_body(config, request))
            .send()
            .map_err(|err| LlmError::Provider(format!("chat completions request: {err}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(LlmError::Provider(format!(
                "chat completions http {status}: {}",
                super::streaming::truncate_for_error(&body)
            )));
        }
        Ok(Box::new(ReqwestSseStream::new(response)))
    }
}

pub struct OpenAiCompatibleReplyClient {
    config: ProviderConfig,
    transport: Arc<dyn ChatTransport>,
}

impl OpenAiCompatibleReplyClient {
    pub fn connect(config: ProviderConfig) -> Result<Self, LlmError> {
        Self::with_transport(config, Arc::new(ReqwestChatTransport::default()))
    }

    pub fn with_transport(
        config: ProviderConfig,
        transport: Arc<dyn ChatTransport>,
    ) -> Result<Self, LlmError> {
        if config.api_key.trim().is_empty() {
            return Err(LlmError::Provider("missing API key".to_string()));
        }
        if config.base_url.trim().is_empty() {
            return Err(LlmError::Provider("missing base_url".to_string()));
        }
        if config.model.trim().is_empty() {
            return Err(LlmError::Provider("missing model".to_string()));
        }
        Ok(Self { config, transport })
    }
}

impl StreamingReplyClient for OpenAiCompatibleReplyClient {
    fn name(&self) -> &'static str {
        "openai-compatible-llm"
    }

    fn start(&self, request: ReplyRequest) -> Box<dyn ReplyGeneration> {
        let config = self.config.clone();
        let transport = Arc::clone(&self.transport);
        let open = {
            let request = request.clone();
            move || -> Result<Box<dyn SseValueStream>, LlmError> {
                transport.stream(&config, &request)
            }
        };
        spawn_streaming_reply(request, open, chat_map)
    }
}
