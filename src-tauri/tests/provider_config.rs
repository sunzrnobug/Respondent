use respondent_lib::provider_config::{
    activate_provider_profile, delete_provider_profile, load_profile_store,
    load_provider_settings, save_provider_settings, upsert_provider_profile,
    AsrProviderSettings, LlmProviderSettings, ProviderProfileStore, ProviderSettings,
};

fn settings_path() -> std::path::PathBuf {
    let unique = format!(
        "respondent-provider-config-test-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}

#[test]
fn summary_redacts_api_keys() {
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "siliconflow".into(),
            api_key: Some("secret-llm".into()),
            base_url: Some("https://api.siliconflow.cn/v1".into()),
            model: Some("Qwen/Qwen3-8B".into()),
        }),
        asr: Some(AsrProviderSettings {
            provider: "bailian_realtime".into(),
            api_key: Some("secret-asr".into()),
            base_url: None,
            model: Some("fun-asr-realtime".into()),
            language_hint: Some("zh".into()),
            max_sentence_silence_ms: Some(800),
            heartbeat: Some(true),
        }),
    };

    let summary = settings.summary();
    let serialized = serde_json::to_string(&summary).unwrap();

    assert!(summary.llm.unwrap().has_api_key);
    assert!(summary.asr.unwrap().has_api_key);
    assert!(!serialized.contains("secret-llm"));
    assert!(!serialized.contains("secret-asr"));
}

#[test]
fn saves_and_loads_provider_settings() {
    let path = settings_path();
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "openai".into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            model: Some("gpt-5.4-mini".into()),
        }),
        asr: None,
    };

    save_provider_settings(&path, &settings).unwrap();
    let loaded = load_provider_settings(&path).unwrap();

    assert_eq!(loaded.llm.unwrap().api_key.as_deref(), Some("sk-test"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_settings_file_loads_empty_settings() {
    let path = settings_path();
    let loaded = load_provider_settings(&path).unwrap();

    assert!(loaded.llm.is_none());
    assert!(loaded.asr.is_none());
}

#[test]
fn migrates_legacy_settings_into_a_default_profile() {
    let legacy = settings_path();
    let profiles = settings_path().with_file_name(format!(
        "provider-profiles-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "openai".into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            model: Some("gpt-5.4-mini".into()),
        }),
        asr: None,
    };

    save_provider_settings(&legacy, &settings).unwrap();
    let store = load_profile_store(&profiles, &legacy).unwrap();

    assert!(profiles.exists());
    assert_eq!(store.profiles.len(), 1);
    assert_eq!(store.profiles[0].name, "默认");
    assert!(store.active_profile_id.is_some());
    assert_eq!(
        store.active_settings().llm.unwrap().api_key.as_deref(),
        Some("sk-test")
    );

    let _ = std::fs::remove_file(legacy);
    let _ = std::fs::remove_file(profiles);
}

#[test]
fn migrated_profile_can_be_deleted_with_the_same_id() {
    let legacy = settings_path();
    let profiles = settings_path().with_file_name(format!(
        "provider-profiles-delete-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "openai".into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            model: Some("gpt-5.4-mini".into()),
        }),
        asr: None,
    };

    save_provider_settings(&legacy, &settings).unwrap();
    let listed = load_profile_store(&profiles, &legacy).unwrap();
    let profile_id = listed.profiles[0].id.clone();

    let mut store = load_profile_store(&profiles, &legacy).unwrap();
    assert_eq!(store.profiles[0].id, profile_id);
    delete_provider_profile(&mut store, &profile_id).unwrap();
    assert!(store.profiles.is_empty());

    let _ = std::fs::remove_file(legacy);
    let _ = std::fs::remove_file(profiles);
}

#[test]
fn upsert_activate_and_delete_profiles() {
    let mut store = ProviderProfileStore::default();
    let first = upsert_provider_profile(
        &mut store,
        None,
        "OpenAI",
        ProviderSettings {
            llm: Some(LlmProviderSettings {
                provider: "openai".into(),
                api_key: Some("sk-test".into()),
                base_url: None,
                model: Some("gpt-5.4-mini".into()),
            }),
            asr: None,
        },
        |_, update| update,
    )
    .unwrap();

    let second = upsert_provider_profile(
        &mut store,
        None,
        "DashScope",
        ProviderSettings::default(),
        |_, update| update,
    )
    .unwrap();

    assert_eq!(store.profiles.len(), 2);
    assert_eq!(store.active_profile_id.as_deref(), Some(second.id.as_str()));

    activate_provider_profile(&mut store, &first.id).unwrap();
    assert_eq!(store.active_profile_id.as_deref(), Some(first.id.as_str()));

    delete_provider_profile(&mut store, &second.id).unwrap();
    assert_eq!(store.profiles.len(), 1);
    assert_eq!(store.active_profile_id.as_deref(), Some(first.id.as_str()));
}
