use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::secret_store::{
    hydrate_legacy_settings_secrets, migrate_legacy_plaintext_secrets,
    prepare_legacy_settings_for_persist,
};

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

pub fn profiles_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Resolve app data directory failed: {err}"))?;
    Ok(dir.join("provider-profiles.json"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub settings: ProviderSettings,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileStore {
    pub active_profile_id: Option<String>,
    pub profiles: Vec<ProviderProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileListItem {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    pub summary: ProviderConfigSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfilesResponse {
    pub profiles: Vec<ProviderProfileListItem>,
    pub active: ProviderConfigSummary,
}

impl ProviderProfileStore {
    pub fn active_settings(&self) -> ProviderSettings {
        self.active_profile()
            .map(|profile| profile.settings.clone())
            .unwrap_or_default()
    }

    pub fn active_profile(&self) -> Option<&ProviderProfile> {
        let active_id = self.active_profile_id.as_deref()?;
        self.profiles.iter().find(|profile| profile.id == active_id)
    }

    pub fn list_items(&self) -> Vec<ProviderProfileListItem> {
        let active_id = self.active_profile_id.as_deref();
        self.profiles
            .iter()
            .map(|profile| ProviderProfileListItem {
                id: profile.id.clone(),
                name: profile.name.clone(),
                is_active: active_id == Some(profile.id.as_str()),
                summary: profile.settings.summary(),
            })
            .collect()
    }

    pub fn response(&self) -> ProviderProfilesResponse {
        ProviderProfilesResponse {
            profiles: self.list_items(),
            active: self.active_settings().summary(),
        }
    }
}

pub fn load_profile_store(
    profiles_path: &Path,
    legacy_settings_path: &Path,
) -> Result<ProviderProfileStore, String> {
    if profiles_path.exists() {
        let text = std::fs::read_to_string(profiles_path)
            .map_err(|err| format!("Read provider profiles failed: {err}"))?;
        let mut store: ProviderProfileStore = serde_json::from_str(&text)
            .map_err(|err| format!("Parse provider profiles failed: {err}"))?;
        let migrated = crate::secret_store::migrate_store_plaintext_secrets(&mut store)?;
        crate::secret_store::hydrate_store_secrets(&mut store)?;
        if migrated {
            save_profile_store(profiles_path, &store)?;
        }
        return Ok(store);
    }

    let legacy = load_provider_settings(legacy_settings_path)?;
    if legacy.llm.is_none() && legacy.asr.is_none() {
        return Ok(ProviderProfileStore::default());
    }

    let profile = ProviderProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name: "默认".to_string(),
        settings: legacy,
        updated_at_ms: chrono::Utc::now().timestamp_millis(),
    };
    let store = ProviderProfileStore {
        active_profile_id: Some(profile.id.clone()),
        profiles: vec![profile],
    };
    save_profile_store(profiles_path, &store)?;
    Ok(store)
}

pub fn save_profile_store(path: &Path, store: &ProviderProfileStore) -> Result<(), String> {
    let mut prepared = store.clone();
    crate::secret_store::prepare_store_for_persist(&mut prepared)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Create provider profiles directory failed: {err}"))?;
    }
    let text = serde_json::to_string_pretty(&prepared)
        .map_err(|err| format!("Serialize provider profiles failed: {err}"))?;
    std::fs::write(path, text).map_err(|err| format!("Write provider profiles failed: {err}"))
}

pub fn normalize_profile_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("服务商配置名称不能为空".into());
    }
    Ok(trimmed.to_string())
}

pub fn upsert_provider_profile(
    store: &mut ProviderProfileStore,
    profile_id: Option<String>,
    name: &str,
    settings: ProviderSettings,
    merge: impl Fn(ProviderSettings, ProviderSettings) -> ProviderSettings,
) -> Result<ProviderProfile, String> {
    let name = normalize_profile_name(name)?;
    let now = chrono::Utc::now().timestamp_millis();

    if let Some(id) = profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !store.profiles.iter().any(|profile| profile.id == id) {
            return Err("未找到该服务商配置".into());
        }
        if store
            .profiles
            .iter()
            .any(|profile| profile.id != id && profile.name == name)
        {
            return Err("服务商配置名称已存在".into());
        }

        let mut updated = None;
        for profile in &mut store.profiles {
            if profile.id == id {
                profile.name = name;
                profile.settings = merge(profile.settings.clone(), settings);
                profile.updated_at_ms = now;
                updated = Some(profile.clone());
                break;
            }
        }
        let profile = updated.expect("profile exists");
        store.active_profile_id = Some(profile.id.clone());
        return Ok(profile);
    }

    if store.profiles.iter().any(|profile| profile.name == name) {
        return Err("服务商配置名称已存在".into());
    }

    let profile = ProviderProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        settings,
        updated_at_ms: now,
    };
    store.active_profile_id = Some(profile.id.clone());
    store.profiles.push(profile.clone());
    Ok(profile)
}

pub fn activate_provider_profile(
    store: &mut ProviderProfileStore,
    profile_id: &str,
) -> Result<(), String> {
    if !store
        .profiles
        .iter()
        .any(|profile| profile.id == profile_id)
    {
        return Err("未找到该服务商配置".into());
    }
    store.active_profile_id = Some(profile_id.to_string());
    Ok(())
}

pub fn delete_provider_profile(
    store: &mut ProviderProfileStore,
    profile_id: &str,
) -> Result<(), String> {
    let original_len = store.profiles.len();
    store.profiles.retain(|profile| profile.id != profile_id);
    if store.profiles.len() == original_len {
        return Err("未找到该服务商配置".into());
    }

    if store.active_profile_id.as_deref() == Some(profile_id) {
        store.active_profile_id = store.profiles.first().map(|profile| profile.id.clone());
    }
    Ok(())
}

pub fn load_provider_settings(path: &Path) -> Result<ProviderSettings, String> {
    if !path.exists() {
        return Ok(ProviderSettings::default());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("Read provider config failed: {err}"))?;
    let mut settings: ProviderSettings = serde_json::from_str(&text)
        .map_err(|err| format!("Parse provider config failed: {err}"))?;
    let migrated = migrate_legacy_plaintext_secrets(&mut settings)?;
    hydrate_legacy_settings_secrets(&mut settings)?;
    if migrated {
        save_provider_settings(path, &settings)?;
    }
    Ok(settings)
}

pub fn save_provider_settings(path: &Path, settings: &ProviderSettings) -> Result<(), String> {
    let mut prepared = settings.clone();
    prepare_legacy_settings_for_persist(&mut prepared)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Create provider config directory failed: {err}"))?;
    }
    let text = serde_json::to_string_pretty(&prepared)
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
