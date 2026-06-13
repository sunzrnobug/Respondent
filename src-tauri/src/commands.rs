use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::asr::bailian_realtime::{BailianRealtimeAsrClient, BailianRealtimeConfig};
use crate::asr::client::{AsrEvent, StreamingAsrClient};
use crate::asr::endpointer::EnergyEndpointer;
use crate::asr::mock::MockAsrClient;
use crate::asr::openai_realtime::{OpenAiRealtimeAsrClient, OpenAiRealtimeConfig};
use crate::asr::session::TranscriptionSession;
use crate::asr::siliconflow_file::{SiliconFlowFileAsrClient, SiliconFlowFileConfig};
use crate::audio::capture::LoopbackCapture;
use crate::audio::devices::{list_output_devices, OutputDevice};
use crate::llm::client::{ReplyEvent, StreamingReplyClient};
use crate::llm::mock::MockReplyClient;
use crate::llm::openai_compatible::{OpenAiCompatibleReplyClient, ProviderConfig};
use crate::llm::openai_responses::{OpenAiReplyClient, OpenAiReplyConfig};
use crate::llm::reply_trigger::ReplyTrigger;
use crate::llm::session::ReplySession;
use crate::provider_config::{
    clean_opt, load_provider_settings, save_provider_settings, settings_file_path,
    AsrProviderSettings, LlmProviderSettings, ProviderConfigSummary, ProviderSettings,
};
use crate::session::db::{EventInsert, SessionDb};
use crate::session::export::SessionExport;

pub const REALTIME_EVENT_NAME: &str = "realtime-event";
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

#[derive(Clone)]
pub struct PersistentSessionDb {
    inner: Arc<Mutex<SessionDb>>,
}

impl PersistentSessionDb {
    pub fn open(app: &tauri::AppHandle) -> Result<Self, String> {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|err| format!("Resolve app data directory failed: {err}"))?;
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("Create app data directory failed: {err}"))?;
        Self::open_path(dir.join("respondent.sqlite3"))
    }

    pub fn open_path(path: PathBuf) -> Result<Self, String> {
        let db = SessionDb::open(&path).map_err(|err| err.to_string())?;
        Ok(Self {
            inner: Arc::new(Mutex::new(db)),
        })
    }

    #[cfg(test)]
    pub fn open_in_memory_for_test() -> Result<Self, String> {
        let db = SessionDb::open_in_memory().map_err(|err| err.to_string())?;
        Ok(Self {
            inner: Arc::new(Mutex::new(db)),
        })
    }

    fn with_db<T>(&self, f: impl FnOnce(&SessionDb) -> Result<T, String>) -> Result<T, String> {
        let db = self
            .inner
            .lock()
            .map_err(|_| "Session database lock failed".to_string())?;
        f(&db)
    }
}

#[derive(Clone)]
pub struct ProviderConfigStore {
    path: PathBuf,
}

impl ProviderConfigStore {
    pub fn open(app: &tauri::AppHandle) -> Result<Self, String> {
        Ok(Self {
            path: settings_file_path(app)?,
        })
    }

    #[cfg(test)]
    pub fn open_path(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> Result<ProviderSettings, String> {
        load_provider_settings(&self.path)
    }

    fn save(&self, settings: &ProviderSettings) -> Result<(), String> {
        save_provider_settings(&self.path, settings)
    }
}

#[tauri::command]
pub fn list_audio_output_devices() -> Vec<OutputDevice> {
    list_output_devices()
}

#[tauri::command]
pub fn start_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionManager>,
    db: tauri::State<'_, PersistentSessionDb>,
    provider_config: tauri::State<'_, ProviderConfigStore>,
    title: String,
    output_device_id: String,
) -> Result<String, String> {
    validate_start_session(&title, &output_device_id)?;
    let provider_settings = provider_config.load()?;
    let session_id = new_session_id();
    db.with_db(|db| {
        db.start_session_with_id(&session_id, &title, &output_device_id)
            .map_err(|err| err.to_string())
    })?;
    let runtime_result = SessionRuntime::start(
        app.clone(),
        session_id.clone(),
        output_device_id,
        db.inner().clone(),
        provider_settings,
    );
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(err) => {
            let _ = db.with_db(|db| db.end_session(&session_id).map_err(|err| err.to_string()));
            return Err(err);
        }
    };
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
    db: tauri::State<'_, PersistentSessionDb>,
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
    db.with_db(|db| db.end_session(&session_id).map_err(|err| err.to_string()))?;
    Ok(())
}

#[tauri::command]
pub fn export_session_markdown(
    state: tauri::State<'_, PersistentSessionDb>,
    session_id: String,
) -> Result<String, String> {
    validate_session_id(&session_id)?;
    let export = state.with_db(|db| db.load_export(&session_id).map_err(|err| err.to_string()))?;
    Ok(format_session_markdown(&export))
}

#[tauri::command]
pub fn export_session_text(
    state: tauri::State<'_, PersistentSessionDb>,
    session_id: String,
) -> Result<String, String> {
    validate_session_id(&session_id)?;
    let export = state.with_db(|db| db.load_export(&session_id).map_err(|err| err.to_string()))?;
    Ok(format_session_text(&export))
}

#[tauri::command]
pub fn get_provider_config(
    state: tauri::State<'_, ProviderConfigStore>,
) -> Result<ProviderConfigSummary, String> {
    Ok(state.load()?.summary())
}

#[tauri::command]
pub fn save_provider_config(
    state: tauri::State<'_, ProviderConfigStore>,
    payload: ProviderSettings,
) -> Result<ProviderConfigSummary, String> {
    let existing = state.load()?;
    let merged = merge_provider_settings(existing, payload);
    state.save(&merged)?;
    Ok(merged.summary())
}

#[tauri::command]
pub fn clear_provider_config(
    state: tauri::State<'_, ProviderConfigStore>,
    scope: Option<String>,
) -> Result<ProviderConfigSummary, String> {
    let mut settings = state.load()?;
    match scope
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("llm") => settings.llm = None,
        Some("asr") => settings.asr = None,
        _ => settings = ProviderSettings::default(),
    }
    state.save(&settings)?;
    Ok(settings.summary())
}

pub fn merge_provider_settings(
    existing: ProviderSettings,
    update: ProviderSettings,
) -> ProviderSettings {
    ProviderSettings {
        llm: merge_llm_settings(existing.llm, update.llm),
        asr: merge_asr_settings(existing.asr, update.asr),
    }
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
        db: PersistentSessionDb,
        provider_settings: ProviderSettings,
    ) -> Result<Self, String> {
        eprintln!("[runtime] start: device={output_device_id:?}");
        let capture = LoopbackCapture::start(&output_device_id).map_err(|err| err.to_string())?;
        eprintln!("[runtime] capture started");
        let frames = capture.receiver();
        let (asr_client, using_mock_asr) = build_asr_client(&session_id, &provider_settings)?;
        eprintln!("[runtime] asr provider mock={using_mock_asr}");
        let transcription = TranscriptionSession::start(
            session_id.clone(),
            frames,
            asr_client,
            EnergyEndpointer::with_defaults(),
        );
        let asr_events = transcription.events();
        let (reply_asr_tx, reply_asr_rx) = unbounded::<AsrEvent>();
        let asr_bridge = spawn_asr_bridge(app.clone(), asr_events, reply_asr_tx, db.clone());
        let (reply_client, using_mock_llm) = build_reply_client(&provider_settings)?;
        let reply = ReplySession::start(
            reply_asr_rx,
            reply_client,
            ReplyTrigger::new(session_id.clone()),
        );
        let reply_bridge = spawn_reply_bridge(app.clone(), reply.events(), db);

        if using_mock_asr {
            emit_status(
                &app,
                SystemStatusEvent::info(
                    Some(session_id.clone()),
                    "OPENAI_API_KEY not set; using mock ASR provider",
                ),
            );
        }
        if using_mock_llm {
            emit_status(
                &app,
                SystemStatusEvent::info(
                    Some(session_id),
                    "No LLM provider configured; using mock LLM provider",
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

fn build_asr_client(
    session_id: &str,
    settings: &ProviderSettings,
) -> Result<(Box<dyn StreamingAsrClient>, bool), String> {
    resolve_asr_client_with_settings(session_id, &current_env(), settings)
}

pub fn resolve_asr_client(
    session_id: &str,
    env: &HashMap<String, String>,
) -> Result<(Box<dyn StreamingAsrClient>, bool), String> {
    let provider = env
        .get("ASR_PROVIDER")
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "openai_realtime".to_string());
    let get = |key: &str| {
        env.get(key)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };

    match provider.as_str() {
        "siliconflow_file" => match get("SILICONFLOW_API_KEY") {
            Some(api_key) => {
                let config = SiliconFlowFileConfig {
                    base_url: get("SILICONFLOW_BASE_URL")
                        .unwrap_or_else(|| "https://api.siliconflow.cn/v1".to_string()),
                    api_key,
                    model: get("SILICONFLOW_ASR_MODEL")
                        .unwrap_or_else(|| "FunAudioLLM/SenseVoiceSmall".to_string()),
                };
                let client = SiliconFlowFileAsrClient::connect(session_id.to_string(), config)
                    .map_err(|e| e.to_string())?;
                Ok((Box::new(client), false))
            }
            None => Ok((Box::new(MockAsrClient::new(session_id)), true)),
        },
        "openai_realtime" => match get("OPENAI_API_KEY") {
            Some(api_key) => {
                let mut config = OpenAiRealtimeConfig::from_api_key(api_key);
                if let Some(model) = get("OPENAI_ASR_MODEL") {
                    config.model = model;
                }
                let client = OpenAiRealtimeAsrClient::connect(session_id.to_string(), config)
                    .map_err(|e| e.to_string())?;
                Ok((Box::new(client), false))
            }
            None => Ok((Box::new(MockAsrClient::new(session_id)), true)),
        },
        "bailian_realtime" => match get("DASHSCOPE_API_KEY") {
            Some(api_key) => {
                let mut config = BailianRealtimeConfig::from_api_key(api_key);
                if let Some(model) = get("DASHSCOPE_ASR_MODEL") {
                    config.model = model;
                }
                if let Some(language_hint) = get("DASHSCOPE_ASR_LANGUAGE_HINT") {
                    config.language_hint = Some(language_hint);
                }
                if let Some(max_sentence_silence) = get("DASHSCOPE_ASR_MAX_SENTENCE_SILENCE_MS") {
                    config.max_sentence_silence_ms = max_sentence_silence.parse::<u32>().ok();
                }
                if let Some(heartbeat) = get("DASHSCOPE_ASR_HEARTBEAT") {
                    config.heartbeat = matches!(
                        heartbeat.to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    );
                }
                let client = BailianRealtimeAsrClient::connect(session_id.to_string(), config)
                    .map_err(|e| e.to_string())?;
                Ok((Box::new(client), false))
            }
            None => Ok((Box::new(MockAsrClient::new(session_id)), true)),
        },
        _ => Ok((Box::new(MockAsrClient::new(session_id)), true)),
    }
}

pub fn resolve_asr_client_with_settings(
    session_id: &str,
    env: &HashMap<String, String>,
    settings: &ProviderSettings,
) -> Result<(Box<dyn StreamingAsrClient>, bool), String> {
    match settings.asr.as_ref().and_then(asr_settings_env) {
        Some(manual_env) => resolve_asr_client(session_id, &manual_env),
        None => resolve_asr_client(session_id, env),
    }
}

pub fn resolve_asr_provider_name(session_id: &str, env: &HashMap<String, String>) -> &'static str {
    let _ = session_id;
    let provider = env
        .get("ASR_PROVIDER")
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "openai_realtime".to_string());
    let has = |key: &str| env.get(key).is_some_and(|v| !v.trim().is_empty());

    match provider.as_str() {
        "siliconflow_file" if has("SILICONFLOW_API_KEY") => "siliconflow-file-asr",
        "openai_realtime" if has("OPENAI_API_KEY") => "openai-realtime-asr",
        "bailian_realtime" if has("DASHSCOPE_API_KEY") => "bailian-realtime-asr",
        _ => "mock-asr",
    }
}

pub fn resolve_asr_provider_name_with_settings(
    session_id: &str,
    env: &HashMap<String, String>,
    settings: &ProviderSettings,
) -> &'static str {
    match settings.asr.as_ref().and_then(asr_settings_env) {
        Some(manual_env) => resolve_asr_provider_name(session_id, &manual_env),
        None => resolve_asr_provider_name(session_id, env),
    }
}

/// Build the reply client from an env-like map. Returns (client, using_mock).
pub fn resolve_reply_client(
    env: &HashMap<String, String>,
) -> Result<(Box<dyn StreamingReplyClient>, bool), String> {
    let provider = env
        .get("LLM_PROVIDER")
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "openai".to_string());

    let get = |key: &str| {
        env.get(key)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };

    let compatible = |base_default: &str,
                      key: Option<String>,
                      model_default: &str,
                      base_key: &str,
                      model_key: &str|
     -> Option<ProviderConfig> {
        let api_key = key?;
        let base_url = get(base_key).unwrap_or_else(|| base_default.to_string());
        let model = get(model_key).unwrap_or_else(|| model_default.to_string());
        Some(ProviderConfig {
            base_url,
            api_key,
            model,
        })
    };

    let cfg: Option<ProviderConfig> = match provider.as_str() {
        "openai" => {
            return match get("OPENAI_API_KEY") {
                Some(key) => {
                    let config = match get("OPENAI_LLM_MODEL") {
                        Some(model) => OpenAiReplyConfig {
                            api_key: key,
                            model,
                        },
                        None => OpenAiReplyConfig::from_api_key(key),
                    };
                    let client = OpenAiReplyClient::connect(config).map_err(|e| e.to_string())?;
                    Ok((Box::new(client), false))
                }
                None => Ok((Box::new(MockReplyClient), true)),
            };
        }
        "dashscope" => compatible(
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            get("DASHSCOPE_API_KEY"),
            "qwen-plus",
            "DASHSCOPE_BASE_URL",
            "DASHSCOPE_LLM_MODEL",
        ),
        "zhipu" => compatible(
            "https://open.bigmodel.cn/api/paas/v4",
            get("ZHIPU_API_KEY").or_else(|| get("ZAI_API_KEY")),
            "glm-4-plus",
            "ZHIPU_BASE_URL",
            "ZHIPU_LLM_MODEL",
        ),
        "siliconflow" => compatible(
            "https://api.siliconflow.cn/v1",
            get("SILICONFLOW_API_KEY"),
            "Qwen/Qwen3-8B",
            "SILICONFLOW_BASE_URL",
            "SILICONFLOW_LLM_MODEL",
        ),
        "openai_compatible" => {
            match (
                get("OPENAI_COMPATIBLE_API_KEY"),
                get("OPENAI_COMPATIBLE_BASE_URL"),
                get("OPENAI_COMPATIBLE_MODEL"),
            ) {
                (Some(api_key), Some(base_url), Some(model)) => Some(ProviderConfig {
                    base_url,
                    api_key,
                    model,
                }),
                _ => None,
            }
        }
        _ => None,
    };

    match cfg {
        Some(config) => {
            let client = OpenAiCompatibleReplyClient::connect(config).map_err(|e| e.to_string())?;
            Ok((Box::new(client), false))
        }
        None => Ok((Box::new(MockReplyClient), true)),
    }
}

pub fn resolve_reply_client_with_settings(
    env: &HashMap<String, String>,
    settings: &ProviderSettings,
) -> Result<(Box<dyn StreamingReplyClient>, bool), String> {
    match settings.llm.as_ref().and_then(llm_settings_env) {
        Some(manual_env) => resolve_reply_client(&manual_env),
        None => resolve_reply_client(env),
    }
}

pub fn resolve_reply_provider_name(env: &HashMap<String, String>) -> &'static str {
    let (client, _) = resolve_reply_client(env).expect("resolve reply client");
    client.name()
}

pub fn resolve_reply_provider_name_with_settings(
    env: &HashMap<String, String>,
    settings: &ProviderSettings,
) -> &'static str {
    let (client, _) =
        resolve_reply_client_with_settings(env, settings).expect("resolve reply client");
    client.name()
}

fn current_env() -> HashMap<String, String> {
    std::env::vars().collect()
}

fn llm_settings_env(settings: &LlmProviderSettings) -> Option<HashMap<String, String>> {
    let provider = non_empty(&settings.provider)?;
    let api_key = settings.api_key.as_deref().and_then(non_empty)?;
    let mut env = HashMap::new();
    env.insert("LLM_PROVIDER".to_string(), provider.clone());

    match provider.as_str() {
        "openai" => {
            env.insert("OPENAI_API_KEY".to_string(), api_key);
            insert_optional(&mut env, "OPENAI_LLM_MODEL", settings.model.as_deref());
        }
        "dashscope" => {
            env.insert("DASHSCOPE_API_KEY".to_string(), api_key);
            insert_optional(&mut env, "DASHSCOPE_BASE_URL", settings.base_url.as_deref());
            insert_optional(&mut env, "DASHSCOPE_LLM_MODEL", settings.model.as_deref());
        }
        "zhipu" => {
            env.insert("ZHIPU_API_KEY".to_string(), api_key);
            insert_optional(&mut env, "ZHIPU_BASE_URL", settings.base_url.as_deref());
            insert_optional(&mut env, "ZHIPU_LLM_MODEL", settings.model.as_deref());
        }
        "siliconflow" => {
            env.insert("SILICONFLOW_API_KEY".to_string(), api_key);
            insert_optional(
                &mut env,
                "SILICONFLOW_BASE_URL",
                settings.base_url.as_deref(),
            );
            insert_optional(&mut env, "SILICONFLOW_LLM_MODEL", settings.model.as_deref());
        }
        "openai_compatible" => {
            env.insert("OPENAI_COMPATIBLE_API_KEY".to_string(), api_key);
            env.insert(
                "OPENAI_COMPATIBLE_BASE_URL".to_string(),
                settings.base_url.as_deref().and_then(non_empty)?,
            );
            env.insert(
                "OPENAI_COMPATIBLE_MODEL".to_string(),
                settings.model.as_deref().and_then(non_empty)?,
            );
        }
        _ => return None,
    }

    Some(env)
}

fn asr_settings_env(settings: &AsrProviderSettings) -> Option<HashMap<String, String>> {
    let provider = non_empty(&settings.provider)?;
    let api_key = settings.api_key.as_deref().and_then(non_empty)?;
    let mut env = HashMap::new();
    env.insert("ASR_PROVIDER".to_string(), provider.clone());

    match provider.as_str() {
        "openai_realtime" => {
            env.insert("OPENAI_API_KEY".to_string(), api_key);
            insert_optional(&mut env, "OPENAI_ASR_MODEL", settings.model.as_deref());
        }
        "bailian_realtime" => {
            env.insert("DASHSCOPE_API_KEY".to_string(), api_key);
            insert_optional(&mut env, "DASHSCOPE_ASR_MODEL", settings.model.as_deref());
            insert_optional(
                &mut env,
                "DASHSCOPE_ASR_LANGUAGE_HINT",
                settings.language_hint.as_deref(),
            );
            if let Some(value) = settings.max_sentence_silence_ms {
                env.insert(
                    "DASHSCOPE_ASR_MAX_SENTENCE_SILENCE_MS".to_string(),
                    value.to_string(),
                );
            }
            if let Some(value) = settings.heartbeat {
                env.insert("DASHSCOPE_ASR_HEARTBEAT".to_string(), value.to_string());
            }
        }
        "siliconflow_file" => {
            env.insert("SILICONFLOW_API_KEY".to_string(), api_key);
            insert_optional(
                &mut env,
                "SILICONFLOW_BASE_URL",
                settings.base_url.as_deref(),
            );
            insert_optional(&mut env, "SILICONFLOW_ASR_MODEL", settings.model.as_deref());
        }
        _ => return None,
    }

    Some(env)
}

fn insert_optional(env: &mut HashMap<String, String>, key: &str, value: Option<&str>) {
    if let Some(value) = value.and_then(non_empty) {
        env.insert(key.to_string(), value);
    }
}

fn merge_llm_settings(
    existing: Option<LlmProviderSettings>,
    update: Option<LlmProviderSettings>,
) -> Option<LlmProviderSettings> {
    let update = update?;
    let old_key = existing
        .as_ref()
        .filter(|existing| same_provider(&existing.provider, &update.provider))
        .and_then(|existing| existing.api_key.clone());
    Some(LlmProviderSettings {
        provider: clean_opt(Some(&update.provider))?,
        api_key: clean_opt(update.api_key.as_deref()).or(old_key),
        base_url: clean_opt(update.base_url.as_deref()),
        model: clean_opt(update.model.as_deref()),
    })
}

fn merge_asr_settings(
    existing: Option<AsrProviderSettings>,
    update: Option<AsrProviderSettings>,
) -> Option<AsrProviderSettings> {
    let update = update?;
    let old_key = existing
        .as_ref()
        .filter(|existing| same_provider(&existing.provider, &update.provider))
        .and_then(|existing| existing.api_key.clone());
    Some(AsrProviderSettings {
        provider: clean_opt(Some(&update.provider))?,
        api_key: clean_opt(update.api_key.as_deref()).or(old_key),
        base_url: clean_opt(update.base_url.as_deref()),
        model: clean_opt(update.model.as_deref()),
        language_hint: clean_opt(update.language_hint.as_deref()),
        max_sentence_silence_ms: update.max_sentence_silence_ms,
        heartbeat: update.heartbeat,
    })
}

fn same_provider(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn build_reply_client(
    settings: &ProviderSettings,
) -> Result<(Box<dyn StreamingReplyClient>, bool), String> {
    resolve_reply_client_with_settings(&current_env(), settings)
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
    db: PersistentSessionDb,
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
                        persist_asr_event(&db, &event);
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

fn spawn_reply_bridge(
    app: tauri::AppHandle,
    events: Receiver<ReplyEvent>,
    db: PersistentSessionDb,
) -> BridgeHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = thread::Builder::new()
        .name("tauri-reply-emit-bridge".into())
        .spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match events.recv_timeout(BRIDGE_WAIT) {
                    Ok(event) => {
                        let _ = app.emit(REALTIME_EVENT_NAME, event.clone());
                        persist_reply_event(&db, &event);
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

fn persist_asr_event(db: &PersistentSessionDb, event: &AsrEvent) {
    if let AsrEvent::Final {
        session_id,
        text,
        started_at_ms,
        ended_at_ms,
        ..
    } = event
    {
        let _ = db.with_db(|db| {
            db.insert_event(EventInsert {
                session_id: session_id.clone(),
                event_type: "transcript".into(),
                text: text.clone(),
                is_final: true,
                started_at_ms: *started_at_ms,
                ended_at_ms: *ended_at_ms,
            })
            .map_err(|err| err.to_string())
        });
    }
}

fn persist_reply_event(db: &PersistentSessionDb, event: &ReplyEvent) {
    if let ReplyEvent::Final {
        session_id,
        text,
        received_at_ms,
        ..
    } = event
    {
        let _ = db.with_db(|db| {
            db.insert_event(EventInsert {
                session_id: session_id.clone(),
                event_type: "suggestion".into(),
                text: text.clone(),
                is_final: true,
                started_at_ms: *received_at_ms,
                ended_at_ms: *received_at_ms,
            })
            .map_err(|err| err.to_string())
        });
    }
}

fn format_session_markdown(export: &SessionExport) -> String {
    let ended_at = export.ended_at.as_deref().unwrap_or("In progress");
    let mut lines = vec![
        format!("## {}", export.title),
        String::new(),
        format!("- Started: {}", export.started_at),
        format!("- Ended: {ended_at}"),
        String::new(),
        "### Timeline".to_string(),
        String::new(),
    ];
    lines.extend(export.events.iter().map(|event| {
        format!(
            "- [{}] {}: {}",
            format_timestamp(event.ended_at_ms),
            event_label(&event.event_type),
            event.text
        )
    }));
    lines.push(String::new());
    lines.join("\n")
}

fn format_session_text(export: &SessionExport) -> String {
    let ended_at = export.ended_at.as_deref().unwrap_or("In progress");
    let mut lines = vec![
        export.title.clone(),
        format!("Started: {}", export.started_at),
        format!("Ended: {ended_at}"),
        String::new(),
    ];
    lines.extend(export.events.iter().map(|event| {
        format!(
            "[{}] {}: {}",
            format_timestamp(event.ended_at_ms),
            event_label(&event.event_type),
            event.text
        )
    }));
    lines.push(String::new());
    lines.join("\n")
}

fn event_label(event_type: &str) -> &'static str {
    match event_type {
        "transcript" => "Transcript",
        "suggestion" => "Suggestion",
        _ => "System",
    }
}

fn format_timestamp(ms: i64) -> String {
    let ms = ms.max(0);
    let minutes = ms / 60_000;
    let seconds = (ms % 60_000) / 1_000;
    let milliseconds = ms % 1_000;
    format!("{minutes:02}:{seconds:02}.{milliseconds:03}")
}

pub fn export_session_markdown_for_test(export: &SessionExport) -> String {
    format_session_markdown(export)
}

pub fn export_session_text_for_test(export: &SessionExport) -> String {
    format_session_text(export)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
