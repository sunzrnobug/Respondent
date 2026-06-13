use std::net::TcpStream;
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde_json::{json, Map, Value};
use tungstenite::http::HeaderValue;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};
use uuid::Uuid;

use crate::audio::frame::AudioFrame;

use super::client::{AsrError, AsrEvent, StreamingAsrClient};
use super::websocket::connect_with_timeout;

const BAILIAN_REALTIME_URL: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference/";

pub trait BailianRealtimeTransport: Send {
    fn send_json(&mut self, value: Value) -> Result<(), AsrError>;
    fn send_binary(&mut self, bytes: Vec<u8>) -> Result<(), AsrError>;
    fn try_recv_json(&mut self) -> Result<Option<Value>, AsrError>;
    fn close(&mut self) -> Result<(), AsrError>;
}

pub struct WebSocketBailianRealtimeTransport {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl WebSocketBailianRealtimeTransport {
    pub fn connect(config: &BailianRealtimeConfig) -> Result<Self, AsrError> {
        if config.api_key.trim().is_empty() {
            return Err(AsrError::Provider("missing DASHSCOPE_API_KEY".to_string()));
        }

        let auth = format!("Bearer {}", config.api_key);
        let auth = HeaderValue::from_str(&auth)
            .map_err(|err| AsrError::Provider(format!("bailian realtime auth header: {err}")))?;
        let socket = connect_with_timeout(BAILIAN_REALTIME_URL, |request| {
            request.headers_mut().insert("Authorization", auth);
            request.headers_mut().insert(
                "user-agent",
                HeaderValue::from_static("respondent-tauri/0.1"),
            );
        })?;
        Ok(Self { socket })
    }
}

impl BailianRealtimeTransport for WebSocketBailianRealtimeTransport {
    fn send_json(&mut self, value: Value) -> Result<(), AsrError> {
        self.socket
            .send(Message::Text(value.to_string().into()))
            .map_err(|err| AsrError::Provider(format!("bailian realtime send: {err}")))
    }

    fn send_binary(&mut self, bytes: Vec<u8>) -> Result<(), AsrError> {
        self.socket
            .send(Message::Binary(bytes.into()))
            .map_err(|err| AsrError::Provider(format!("bailian realtime send audio: {err}")))
    }

    fn try_recv_json(&mut self) -> Result<Option<Value>, AsrError> {
        match self.socket.read() {
            Ok(Message::Text(text)) => serde_json::from_str(text.as_ref())
                .map(Some)
                .map_err(|err| AsrError::Provider(format!("bailian realtime json: {err}"))),
            Ok(Message::Binary(_)) | Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => Ok(None),
            Ok(Message::Frame(_)) => Ok(None),
            Ok(Message::Close(_)) => Err(AsrError::Closed),
            Err(tungstenite::Error::Io(err)) if err.kind() == std::io::ErrorKind::WouldBlock => {
                Ok(None)
            }
            Err(err) => Err(AsrError::Provider(format!(
                "bailian realtime receive: {err}"
            ))),
        }
    }

    fn close(&mut self) -> Result<(), AsrError> {
        self.socket
            .close(None)
            .map_err(|err| AsrError::Provider(format!("bailian realtime close: {err}")))
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BailianRealtimeConfig {
    pub api_key: String,
    pub model: String,
    pub sample_rate: u32,
    pub format: String,
    pub language_hint: Option<String>,
    pub max_sentence_silence_ms: Option<u32>,
    pub heartbeat: bool,
}

impl std::fmt::Debug for BailianRealtimeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BailianRealtimeConfig")
            .field("api_key", &"<redacted>")
            .field("model", &self.model)
            .field("sample_rate", &self.sample_rate)
            .field("format", &self.format)
            .field("language_hint", &self.language_hint)
            .field("max_sentence_silence_ms", &self.max_sentence_silence_ms)
            .field("heartbeat", &self.heartbeat)
            .finish()
    }
}

impl BailianRealtimeConfig {
    pub fn from_api_key(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "fun-asr-realtime".to_string(),
            sample_rate: 16_000,
            format: "pcm".to_string(),
            language_hint: None,
            max_sentence_silence_ms: None,
            heartbeat: false,
        }
    }
}

pub struct BailianRealtimeAsrClient {
    session_id: String,
    config: BailianRealtimeConfig,
    task_id: String,
    task_started: bool,
    task_finished: bool,
    transport: Box<dyn BailianRealtimeTransport>,
    sender: Sender<AsrEvent>,
    receiver: Receiver<AsrEvent>,
}

impl BailianRealtimeAsrClient {
    pub fn connect(session_id: String, config: BailianRealtimeConfig) -> Result<Self, AsrError> {
        let transport = WebSocketBailianRealtimeTransport::connect(&config)?;
        Self::with_transport(session_id, config, Box::new(transport))
    }

    pub fn from_env(session_id: String) -> Result<Self, AsrError> {
        let api_key = std::env::var("DASHSCOPE_API_KEY")
            .map_err(|_| AsrError::Provider("missing DASHSCOPE_API_KEY".to_string()))?;
        Self::connect(session_id, BailianRealtimeConfig::from_api_key(api_key))
    }

    pub fn with_transport(
        session_id: String,
        config: BailianRealtimeConfig,
        transport: Box<dyn BailianRealtimeTransport>,
    ) -> Result<Self, AsrError> {
        if config.api_key.trim().is_empty() {
            return Err(AsrError::Provider("missing DASHSCOPE_API_KEY".to_string()));
        }
        if config.sample_rate != 16_000 {
            return Err(AsrError::Provider(
                "bailian realtime asr expects 16 kHz mono i16 frames".to_string(),
            ));
        }

        let (sender, receiver) = unbounded();
        let mut client = Self {
            session_id,
            config,
            task_id: Uuid::new_v4().to_string(),
            task_started: false,
            task_finished: false,
            transport,
            sender,
            receiver,
        };
        client.send_run_task()?;
        Ok(client)
    }

    fn send_run_task(&mut self) -> Result<(), AsrError> {
        self.transport.send_json(json!({
            "header": {
                "action": "run-task",
                "task_id": self.task_id,
                "streaming": "duplex",
            },
            "payload": {
                "task_group": "audio",
                "task": "asr",
                "function": "recognition",
                "model": self.config.model,
                "parameters": self.parameters(),
                "input": {},
            },
        }))
    }

    fn parameters(&self) -> Value {
        let mut parameters = Map::new();
        parameters.insert("format".to_string(), json!(self.config.format));
        parameters.insert("sample_rate".to_string(), json!(self.config.sample_rate));
        parameters.insert("heartbeat".to_string(), json!(self.config.heartbeat));
        if let Some(language_hint) = &self.config.language_hint {
            parameters.insert("language_hints".to_string(), json!([language_hint]));
        }
        if let Some(max_sentence_silence) = self.config.max_sentence_silence_ms {
            parameters.insert(
                "max_sentence_silence".to_string(),
                json!(max_sentence_silence),
            );
        }
        Value::Object(parameters)
    }

    fn drain_provider_events(&mut self) -> Result<(), AsrError> {
        while let Some(event) = self.transport.try_recv_json()? {
            self.handle_provider_event(event)?;
        }
        Ok(())
    }

    fn event_task_id_matches(&self, event: &Value) -> bool {
        event["header"]["task_id"]
            .as_str()
            .is_some_and(|id| id == self.task_id)
    }

    fn handle_provider_event(&mut self, event: Value) -> Result<(), AsrError> {
        match event["header"]["event"].as_str() {
            Some("task-started") => {
                if self.event_task_id_matches(&event) {
                    self.task_started = true;
                }
                Ok(())
            }
            Some("result-generated") => self.handle_result_generated(event),
            Some("task-finished") => {
                if self.event_task_id_matches(&event) {
                    self.task_finished = true;
                }
                Ok(())
            }
            Some("task-failed") => {
                let code = event["header"]["error_code"]
                    .as_str()
                    .unwrap_or("PROVIDER_ERROR");
                let message = event["header"]["error_message"]
                    .as_str()
                    .unwrap_or("provider error");
                Err(AsrError::Provider(format!(
                    "bailian realtime error {code}: {message}"
                )))
            }
            _ => Ok(()),
        }
    }

    fn handle_result_generated(&mut self, event: Value) -> Result<(), AsrError> {
        let sentence = &event["payload"]["output"]["sentence"];
        if sentence["heartbeat"].as_bool().unwrap_or(false) {
            return Ok(());
        }

        let text = sentence["text"].as_str().unwrap_or_default().to_string();
        if text.trim().is_empty() {
            return Ok(());
        }

        let started_at_ms = sentence["begin_time"].as_i64().unwrap_or(0);
        let ended_at_ms = sentence["end_time"].as_i64().unwrap_or(started_at_ms);
        let received_at_ms = now_ms();
        let event = if sentence["sentence_end"].as_bool().unwrap_or(false) {
            AsrEvent::Final {
                session_id: self.session_id.clone(),
                text,
                started_at_ms,
                ended_at_ms,
                received_at_ms,
            }
        } else {
            AsrEvent::Partial {
                session_id: self.session_id.clone(),
                text,
                started_at_ms,
                ended_at_ms,
                received_at_ms,
            }
        };

        self.sender.send(event).map_err(|_| AsrError::Closed)
    }

    fn validate_frame(&self, frame: &AudioFrame) -> Result<(), AsrError> {
        if frame.format.sample_rate != 16_000
            || frame.format.channels != 1
            || frame.format.bits_per_sample != 16
        {
            return Err(AsrError::Provider(
                "bailian realtime asr expects 16 kHz mono i16 frames".to_string(),
            ));
        }
        Ok(())
    }

    fn wait_for_task_finished(&mut self) -> Result<(), AsrError> {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.task_finished {
            if Instant::now() > deadline {
                break;
            }
            self.drain_provider_events()?;
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }
}

impl StreamingAsrClient for BailianRealtimeAsrClient {
    fn name(&self) -> &'static str {
        "bailian-realtime-asr"
    }

    fn push_frame(&mut self, frame: &AudioFrame) -> Result<(), AsrError> {
        self.validate_frame(frame)?;
        self.drain_provider_events()?;
        if !self.task_started || self.task_finished {
            return Ok(());
        }
        let bytes = frame
            .samples
            .iter()
            .copied()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        self.transport.send_binary(bytes)?;
        self.drain_provider_events()
    }

    fn events(&self) -> Receiver<AsrEvent> {
        self.receiver.clone()
    }

    fn finalize(&mut self) -> Result<(), AsrError> {
        self.drain_provider_events()?;
        if !self.task_finished {
            self.transport.send_json(json!({
                "header": {
                    "action": "finish-task",
                    "task_id": self.task_id,
                    "streaming": "duplex",
                },
                "payload": {
                    "input": {},
                },
            }))?;
            self.wait_for_task_finished()?;
        }
        self.task_id = Uuid::new_v4().to_string();
        self.task_started = false;
        self.task_finished = false;
        self.send_run_task()
    }
}

impl Drop for BailianRealtimeAsrClient {
    fn drop(&mut self) {
        let _ = self.transport.close();
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
