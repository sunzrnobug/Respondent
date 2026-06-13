use respondent_lib::provider_config::{
    load_provider_settings, save_provider_settings, AsrProviderSettings, LlmProviderSettings,
    ProviderSettings,
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
