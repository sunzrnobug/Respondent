use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::provider_config::{ProviderProfileStore, ProviderSettings};

const SERVICE: &str = "com.respondent.desktop";
pub const LEGACY_PROFILE_ID: &str = "legacy-default";
const MEMORY_BACKEND_ENV: &str = "RESPONDENT_SECRET_BACKEND";

fn use_memory_backend() -> bool {
    std::env::var(MEMORY_BACKEND_ENV)
        .ok()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("memory"))
}

fn memory_secret_store() -> &'static Mutex<HashMap<String, String>> {
    static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn store_secret_memory(key: &str, value: &str) -> Result<(), String> {
    let mut store = memory_secret_store()
        .lock()
        .map_err(|_| "Memory secret store lock failed".to_string())?;
    store.insert(key.to_string(), value.to_string());
    Ok(())
}

fn load_secret_memory(key: &str) -> Result<Option<String>, String> {
    let store = memory_secret_store()
        .lock()
        .map_err(|_| "Memory secret store lock failed".to_string())?;
    Ok(store.get(key).cloned())
}

fn delete_secret_memory(key: &str) -> Result<(), String> {
    let mut store = memory_secret_store()
        .lock()
        .map_err(|_| "Memory secret store lock failed".to_string())?;
    store.remove(key);
    Ok(())
}

fn store_secret_keyring(key: &str, value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, key)
        .map_err(|err| format!("Open credential entry failed: {err}"))?;
    entry
        .set_password(value)
        .map_err(|err| format!("Store credential failed: {err}"))
}

fn load_secret_keyring(key: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE, key)
        .map_err(|err| format!("Open credential entry failed: {err}"))?;
    match entry.get_password() {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(format!("Load credential failed: {err}")),
    }
}

fn delete_secret_keyring(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, key)
        .map_err(|err| format!("Open credential entry failed: {err}"))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(format!("Delete credential failed: {err}")),
    }
}

const KEYRING_PROBE_KEY: &str = "__respondent/keyring-probe";

/// Verifies the OS credential backend can round-trip a secret.
/// Refuses plaintext-key migration when this fails (e.g. mock keystore).
pub fn verify_keyring_backend() -> Result<(), String> {
    if use_memory_backend() {
        return Ok(());
    }
    let probe_value = format!("probe-{}", uuid::Uuid::new_v4());
    store_secret_keyring(KEYRING_PROBE_KEY, &probe_value)?;
    let loaded = load_secret_keyring(KEYRING_PROBE_KEY)?;
    delete_secret_keyring(KEYRING_PROBE_KEY)?;
    if loaded.as_deref() != Some(probe_value.as_str()) {
        return Err(
            "OS credential store did not round-trip a probe secret; refusing to migrate plaintext API keys"
                .into(),
        );
    }
    Ok(())
}

#[doc(hidden)]
pub fn roundtrip_system_keyring_for_integration_test(key: &str, value: &str) -> Result<(), String> {
    if use_memory_backend() {
        return Err(
            "RESPONDENT_SECRET_BACKEND=memory is set; unset it to exercise the OS credential store"
                .into(),
        );
    }
    store_secret_keyring(key, value)?;
    let loaded = load_secret_keyring(key)?;
    if loaded.as_deref() != Some(value) {
        delete_secret_keyring(key)?;
        return Err("OS credential store did not return the stored secret".into());
    }
    delete_secret_keyring(key)?;
    Ok(())
}

const DB_MASTER_KEY: &str = "respondent/db-master-key";

pub fn load_or_create_db_master_key() -> Result<String, String> {
    if let Some(existing) = load_secret(DB_MASTER_KEY)? {
        return Ok(existing);
    }
    let key = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    store_secret(DB_MASTER_KEY, &key)?;
    Ok(key)
}

fn llm_secret_key(profile_id: &str) -> String {
    format!("{profile_id}/llm-api-key")
}

fn asr_secret_key(profile_id: &str) -> String {
    format!("{profile_id}/asr-api-key")
}

fn store_secret(key: &str, value: &str) -> Result<(), String> {
    if use_memory_backend() {
        store_secret_memory(key, value)
    } else {
        store_secret_keyring(key, value)
    }
}

fn load_secret(key: &str) -> Result<Option<String>, String> {
    if use_memory_backend() {
        load_secret_memory(key)
    } else {
        load_secret_keyring(key)
    }
}

pub fn delete_profile_secrets(profile_id: &str) -> Result<(), String> {
    for key in [llm_secret_key(profile_id), asr_secret_key(profile_id)] {
        if use_memory_backend() {
            delete_secret_memory(&key)?;
        } else {
            delete_secret_keyring(&key)?;
        }
    }
    Ok(())
}

pub fn prepare_settings_for_persist(
    profile_id: &str,
    settings: &mut ProviderSettings,
) -> Result<(), String> {
    if let Some(llm) = settings.llm.as_mut() {
        if let Some(api_key) = llm.api_key.as_deref().filter(|value| !value.trim().is_empty()) {
            store_secret(&llm_secret_key(profile_id), api_key)?;
        }
        llm.api_key = None;
    }
    if let Some(asr) = settings.asr.as_mut() {
        if let Some(api_key) = asr.api_key.as_deref().filter(|value| !value.trim().is_empty()) {
            store_secret(&asr_secret_key(profile_id), api_key)?;
        }
        asr.api_key = None;
    }
    Ok(())
}

pub fn hydrate_settings_secrets(
    profile_id: &str,
    settings: &mut ProviderSettings,
) -> Result<(), String> {
    if let Some(llm) = settings.llm.as_mut() {
        if llm.api_key.as_deref().is_none_or(|value| value.trim().is_empty()) {
            llm.api_key = load_secret(&llm_secret_key(profile_id))?;
        }
    }
    if let Some(asr) = settings.asr.as_mut() {
        if asr.api_key.as_deref().is_none_or(|value| value.trim().is_empty()) {
            asr.api_key = load_secret(&asr_secret_key(profile_id))?;
        }
    }
    Ok(())
}

fn migrate_plaintext_settings(profile_id: &str, settings: &mut ProviderSettings) -> Result<bool, String> {
    let mut migrated = false;
    if let Some(llm) = settings.llm.as_mut() {
        if llm.api_key.as_deref().is_some_and(|value| !value.trim().is_empty()) {
            migrated = true;
        }
    }
    if let Some(asr) = settings.asr.as_mut() {
        if asr.api_key.as_deref().is_some_and(|value| !value.trim().is_empty()) {
            migrated = true;
        }
    }
    if migrated {
        verify_keyring_backend()?;
        prepare_settings_for_persist(profile_id, settings)?;
    }
    Ok(migrated)
}

pub fn prepare_store_for_persist(store: &mut ProviderProfileStore) -> Result<(), String> {
    for profile in &mut store.profiles {
        prepare_settings_for_persist(&profile.id, &mut profile.settings)?;
    }
    Ok(())
}

pub fn hydrate_store_secrets(store: &mut ProviderProfileStore) -> Result<(), String> {
    for profile in &mut store.profiles {
        hydrate_settings_secrets(&profile.id, &mut profile.settings)?;
    }
    Ok(())
}

pub fn migrate_store_plaintext_secrets(store: &mut ProviderProfileStore) -> Result<bool, String> {
    let mut migrated = false;
    for profile in &mut store.profiles {
        migrated |= migrate_plaintext_settings(&profile.id, &mut profile.settings)?;
    }
    Ok(migrated)
}

pub fn prepare_legacy_settings_for_persist(settings: &mut ProviderSettings) -> Result<(), String> {
    prepare_settings_for_persist(LEGACY_PROFILE_ID, settings)
}

pub fn hydrate_legacy_settings_secrets(settings: &mut ProviderSettings) -> Result<(), String> {
    hydrate_settings_secrets(LEGACY_PROFILE_ID, settings)
}

pub fn migrate_legacy_plaintext_secrets(settings: &mut ProviderSettings) -> Result<bool, String> {
    migrate_plaintext_settings(LEGACY_PROFILE_ID, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_config::{AsrProviderSettings, LlmProviderSettings};

    fn unique_profile_id() -> String {
        format!(
            "test-profile-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[test]
    fn stores_and_hydrates_provider_secrets_in_memory_backend() {
        std::env::set_var(MEMORY_BACKEND_ENV, "memory");
        let profile_id = unique_profile_id();
        let mut settings = ProviderSettings {
            llm: Some(LlmProviderSettings {
                provider: "openai".into(),
                api_key: Some("sk-test".into()),
                base_url: None,
                model: None,
            }),
            asr: Some(AsrProviderSettings {
                provider: "openai_realtime".into(),
                api_key: Some("asr-test".into()),
                base_url: None,
                model: None,
                language_hint: None,
                max_sentence_silence_ms: None,
                heartbeat: None,
            }),
        };

        prepare_settings_for_persist(&profile_id, &mut settings).expect("persist secrets");
        assert!(settings.llm.as_ref().unwrap().api_key.is_none());
        assert!(settings.asr.as_ref().unwrap().api_key.is_none());

        hydrate_settings_secrets(&profile_id, &mut settings).expect("hydrate secrets");
        assert_eq!(
            settings.llm.as_ref().unwrap().api_key.as_deref(),
            Some("sk-test")
        );
        assert_eq!(
            settings.asr.as_ref().unwrap().api_key.as_deref(),
            Some("asr-test")
        );

        delete_profile_secrets(&profile_id).expect("delete secrets");
        std::env::remove_var(MEMORY_BACKEND_ENV);
    }
}
