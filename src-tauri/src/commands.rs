use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use serde::Serialize;
use tauri::Emitter;

use crate::asr::client::{AsrEvent, StreamingAsrClient};
use crate::asr::endpointer::EnergyEndpointer;
use crate::asr::mock::MockAsrClient;
use crate::asr::openai_realtime::{OpenAiRealtimeAsrClient, OpenAiRealtimeConfig};
use crate::asr::session::TranscriptionSession;
use crate::audio::capture::LoopbackCapture;
use crate::audio::devices::{list_output_devices, OutputDevice};
use crate::llm::client::ReplyEvent;
use crate::llm::mock::MockReplyClient;
use crate::llm::reply_trigger::ReplyTrigger;
use crate::llm::session::ReplySession;

pub const REALTIME_EVENT_NAME: &str = "realtime.event";
const BRIDGE_WAIT: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatusEvent {
    #[serde(rename = "type")]
    event_type: &'static str,
    session_id: Option<String>,
    level: &'static str,
    message: String,
    received_at_ms: i64,
}

impl SystemStatusEvent {
    pub fn info(session_id: Option<String>, message: impl Into<String>) -> Self {
        Self::new(session_id, "info", message)
    }

    pub fn error(session_id: Option<String>, message: impl Into<String>) -> Self {
        Self::new(session_id, "error", message)
    }

    fn new(session_id: Option<String>, level: &'static str, message: impl Into<String>) -> Self {
        Self {
            event_type: "system.status",
            session_id,
            level,
            message: message.into(),
            received_at_ms: now_ms(),
        }
    }
}

#[derive(Default)]
pub struct SessionManager {
    sessions: Mutex<HashMap<String, SessionRuntime>>,
}

#[tauri::command]
pub fn list_audio_output_devices() -> Vec<OutputDevice> {
    list_output_devices()
}

#[tauri::command]
pub fn start_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionManager>,
    title: String,
    output_device_id: String,
) -> Result<String, String> {
    validate_start_session(&title, &output_device_id)?;
    let session_id = new_session_id();
    let runtime = SessionRuntime::start(app.clone(), session_id.clone(), output_device_id)?;
    {
        let mut sessions = state
            .inner()
            .sessions
            .lock()
            .map_err(|_| "Session manager lock failed".to_string())?;
        sessions.insert(session_id.clone(), runtime);
    }
    emit_status(
        &app,
        SystemStatusEvent::info(Some(session_id.clone()), "Native realtime session started"),
    );
    Ok(session_id)
}

#[tauri::command]
pub fn end_session(
    state: tauri::State<'_, SessionManager>,
    session_id: String,
) -> Result<(), String> {
    validate_session_id(&session_id)?;
    let runtime = {
        let mut sessions = state
            .inner()
            .sessions
            .lock()
            .map_err(|_| "Session manager lock failed".to_string())?;
        sessions.remove(&session_id)
    };
    if let Some(runtime) = runtime {
        runtime.stop();
    }
    Ok(())
}

pub fn start_session_for_test(title: String, output_device_id: String) -> Result<String, String> {
    validate_start_session(&title, &output_device_id)?;
    Ok(new_session_id())
}

pub fn end_session_for_test(session_id: String) -> Result<(), String> {
    validate_session_id(&session_id)
}

fn validate_start_session(title: &str, output_device_id: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("Session title cannot be empty".into());
    }
    if output_device_id.trim().is_empty() {
        return Err("Output device id cannot be empty".into());
    }
    Ok(())
}

fn validate_session_id(session_id: &str) -> Result<(), String> {
    if session_id.trim().is_empty() {
        return Err("Session id cannot be empty".into());
    }
    Ok(())
}

fn new_session_id() -> String {
    format!("session-{}", chrono::Utc::now().timestamp_millis())
}

struct SessionRuntime {
    capture: LoopbackCapture,
    transcription: TranscriptionSession,
    reply: ReplySession,
    asr_bridge: BridgeHandle,
    reply_bridge: BridgeHandle,
}

impl SessionRuntime {
    fn start(
        app: tauri::AppHandle,
        session_id: String,
        output_device_id: String,
    ) -> Result<Self, String> {
        let capture = LoopbackCapture::start(&output_device_id).map_err(|err| err.to_string())?;
        let frames = capture.receiver();
        let (asr_client, using_mock_asr) = build_asr_client(&session_id)?;
        let transcription = TranscriptionSession::start(
            session_id.clone(),
            frames,
            asr_client,
            EnergyEndpointer::with_defaults(),
        );
        let asr_events = transcription.events();
        let (reply_asr_tx, reply_asr_rx) = unbounded::<AsrEvent>();
        let asr_bridge = spawn_asr_bridge(app.clone(), asr_events, reply_asr_tx);
        let reply = ReplySession::start(
            reply_asr_rx,
            Box::new(MockReplyClient),
            ReplyTrigger::new(session_id.clone()),
        );
        let reply_bridge = spawn_reply_bridge(app.clone(), reply.events());

        if using_mock_asr {
            emit_status(
                &app,
                SystemStatusEvent::info(
                    Some(session_id),
                    "OPENAI_API_KEY not set; using mock ASR provider",
                ),
            );
        }

        Ok(Self {
            capture,
            transcription,
            reply,
            asr_bridge,
            reply_bridge,
        })
    }

    fn stop(self) {
        let _ = self.capture.stop();
        let _ = self.transcription.stop();
        let _ = self.reply.stop();
        self.asr_bridge.stop();
        self.reply_bridge.stop();
    }
}

fn build_asr_client(session_id: &str) -> Result<(Box<dyn StreamingAsrClient>, bool), String> {
    match std::env::var("OPENAI_API_KEY") {
        Ok(api_key) if !api_key.trim().is_empty() => {
            let client = OpenAiRealtimeAsrClient::connect(
                session_id.to_string(),
                OpenAiRealtimeConfig::from_api_key(api_key),
            )
            .map_err(|err| err.to_string())?;
            Ok((Box::new(client), false))
        }
        _ => Ok((Box::new(MockAsrClient::new(session_id)), true)),
    }
}

struct BridgeHandle {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl BridgeHandle {
    fn stop(self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.handle.join();
    }
}

fn spawn_asr_bridge(
    app: tauri::AppHandle,
    events: Receiver<AsrEvent>,
    reply_tx: Sender<AsrEvent>,
) -> BridgeHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = thread::Builder::new()
        .name("tauri-asr-emit-bridge".into())
        .spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match events.recv_timeout(BRIDGE_WAIT) {
                    Ok(event) => {
                        let _ = app.emit(REALTIME_EVENT_NAME, event.clone());
                        let _ = reply_tx.send(event);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("spawn tauri asr emit bridge");
    BridgeHandle { stop, handle }
}

fn spawn_reply_bridge(app: tauri::AppHandle, events: Receiver<ReplyEvent>) -> BridgeHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = thread::Builder::new()
        .name("tauri-reply-emit-bridge".into())
        .spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match events.recv_timeout(BRIDGE_WAIT) {
                    Ok(event) => {
                        let _ = app.emit(REALTIME_EVENT_NAME, event);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("spawn tauri reply emit bridge");
    BridgeHandle { stop, handle }
}

fn emit_status(app: &tauri::AppHandle, event: SystemStatusEvent) {
    let _ = app.emit(REALTIME_EVENT_NAME, event);
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
