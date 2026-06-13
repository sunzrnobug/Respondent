use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub llm: Option<LlmProviderSettings>,
    pub asr: Option<AsrProviderSettings>,
}

impl ProviderSettings {
    pub fn summary(&self) -> ProviderConfigSummary {
        ProviderConfigSummary {
            llm: self.llm.as_ref().map(LlmProviderSettings::summary),
            asr: self.asr.as_ref().map(AsrProviderSettings::summary),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderSettings {
    pub provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

impl LlmProviderSettings {
    fn summary(&self) -> LlmProviderSummary {
        LlmProviderSummary {
            provider: self.provider.clone(),
            has_api_key: has_secret(&self.api_key),
            base_url: clean_opt(self.base_url.as_deref()),
            model: clean_opt(self.model.as_deref()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrProviderSettings {
    pub provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub language_hint: Option<String>,
    pub max_sentence_silence_ms: Option<u32>,
    pub heartbeat: Option<bool>,
}

impl AsrProviderSettings {
    fn summary(&self) -> AsrProviderSummary {
        AsrProviderSummary {
            provider: self.provider.clone(),
            has_api_key: has_secret(&self.api_key),
            base_url: clean_opt(self.base_url.as_deref()),
            model: clean_opt(self.model.as_deref()),
            language_hint: clean_opt(self.language_hint.as_deref()),
            max_sentence_silence_ms: self.max_sentence_silence_ms,
            heartbeat: self.heartbeat,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigSummary {
    pub llm: Option<LlmProviderSummary>,
    pub asr: Option<AsrProviderSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderSummary {
    pub provider: String,
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AsrProviderSummary {
    pub provider: String,
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub language_hint: Option<String>,
    pub max_sentence_silence_ms: Option<u32>,
    pub heartbeat: Option<bool>,
}

pub fn settings_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Resolve app data directory failed: {err}"))?;
    Ok(dir.join("provider-config.json"))
}

pub fn load_provider_settings(path: &Path) -> Result<ProviderSettings, String> {
    if !path.exists() {
        return Ok(ProviderSettings::default());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("Read provider config failed: {err}"))?;
    serde_json::from_str(&text).map_err(|err| format!("Parse provider config failed: {err}"))
}

pub fn save_provider_settings(path: &Path, settings: &ProviderSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Create provider config directory failed: {err}"))?;
    }
    let text = serde_json::to_string_pretty(settings)
        .map_err(|err| format!("Serialize provider config failed: {err}"))?;
    std::fs::write(path, text).map_err(|err| format!("Write provider config failed: {err}"))
}

pub fn clean_opt(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn has_secret(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
}
