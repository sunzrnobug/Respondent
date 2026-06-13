use respondent_lib::commands::{
    end_session_for_test, export_session_markdown_for_test, export_session_text_for_test,
    merge_provider_settings, resolve_asr_provider_name, resolve_asr_provider_name_with_settings,
    resolve_reply_provider_name, resolve_reply_provider_name_with_settings, start_session_for_test,
    SystemStatusEvent,
};
use respondent_lib::provider_config::{AsrProviderSettings, LlmProviderSettings, ProviderSettings};
use respondent_lib::session::export::{SessionExport, SessionExportEvent};

fn env(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn start_session_rejects_empty_title() {
    assert!(start_session_for_test(String::new(), "default-output".into()).is_err());
}

#[test]
fn start_session_rejects_empty_output_device() {
    assert!(start_session_for_test("Customer call".into(), String::new()).is_err());
}

#[test]
fn start_session_accepts_valid_input() {
    let id = start_session_for_test("Customer call".into(), "default-output".into())
        .expect("valid session start");
    assert!(id.starts_with("session-"));
}

#[test]
fn end_session_rejects_empty_id() {
    assert!(end_session_for_test(String::new()).is_err());
}

#[test]
fn end_session_accepts_non_empty_id() {
    assert!(end_session_for_test("session-123".into()).is_ok());
}

#[test]
fn system_status_serializes_to_frontend_contract() {
    let event = SystemStatusEvent::info(Some("s1".to_string()), "ready");
    let value = serde_json::to_value(&event).expect("serialize");
    assert_eq!(value["type"], "system.status");
    assert_eq!(value["sessionId"], "s1");
    assert_eq!(value["level"], "info");
    assert_eq!(value["message"], "ready");
    assert!(value["receivedAtMs"].as_i64().unwrap() > 0);
}

#[test]
fn provider_defaults_to_mock_without_keys() {
    assert_eq!(resolve_reply_provider_name(&env(&[])), "mock-llm");
}

#[test]
fn provider_openai_with_key() {
    assert_eq!(
        resolve_reply_provider_name(&env(&[("OPENAI_API_KEY", "k")])),
        "openai-responses-llm"
    );
}

#[test]
fn provider_dashscope_with_key() {
    assert_eq!(
        resolve_reply_provider_name(&env(&[
            ("LLM_PROVIDER", "dashscope"),
            ("DASHSCOPE_API_KEY", "k")
        ])),
        "openai-compatible-llm"
    );
}

#[test]
fn provider_compatible_missing_config_falls_back_to_mock() {
    assert_eq!(
        resolve_reply_provider_name(&env(&[("LLM_PROVIDER", "siliconflow")])),
        "mock-llm"
    );
}

#[test]
fn export_helpers_format_markdown_and_plain_text() {
    let export = SessionExport {
        id: "session-1".into(),
        title: "Meeting".into(),
        started_at: "2026-06-13T00:00:00Z".into(),
        ended_at: Some("2026-06-13T00:01:00Z".into()),
        events: vec![
            SessionExportEvent {
                event_type: "transcript".into(),
                text: "hello".into(),
                is_final: true,
                started_at_ms: 0,
                ended_at_ms: 300,
            },
            SessionExportEvent {
                event_type: "suggestion".into(),
                text: "ask about timing".into(),
                is_final: true,
                started_at_ms: 300,
                ended_at_ms: 300,
            },
        ],
    };

    let markdown = export_session_markdown_for_test(&export);
    let text = export_session_text_for_test(&export);

    assert!(markdown.contains("## Meeting"));
    assert!(markdown.contains("[00:00.300] Suggestion: ask about timing"));
    assert!(text.contains("Transcript: hello"));
}

#[test]
fn provider_zhipu_accepts_zai_api_key() {
    assert_eq!(
        resolve_reply_provider_name(&env(&[("LLM_PROVIDER", "zhipu"), ("ZAI_API_KEY", "k")])),
        "openai-compatible-llm"
    );
}

#[test]
fn llm_manual_settings_override_env() {
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "siliconflow".into(),
            api_key: Some("manual-key".into()),
            base_url: None,
            model: None,
        }),
        asr: None,
    };

    assert_eq!(
        resolve_reply_provider_name_with_settings(
            &env(&[("LLM_PROVIDER", "openai"), ("OPENAI_API_KEY", "env-key")]),
            &settings,
        ),
        "openai-compatible-llm"
    );
}

#[test]
fn llm_incomplete_manual_settings_fall_back_to_env() {
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "siliconflow".into(),
            api_key: None,
            base_url: None,
            model: None,
        }),
        asr: None,
    };

    assert_eq!(
        resolve_reply_provider_name_with_settings(
            &env(&[("LLM_PROVIDER", "openai"), ("OPENAI_API_KEY", "env-key")]),
            &settings,
        ),
        "openai-responses-llm"
    );
}

#[test]
fn asr_defaults_to_mock_without_keys() {
    assert_eq!(resolve_asr_provider_name("s1", &env(&[])), "mock-asr");
}

#[test]
fn asr_siliconflow_file_with_key() {
    assert_eq!(
        resolve_asr_provider_name(
            "s1",
            &env(&[
                ("ASR_PROVIDER", "siliconflow_file"),
                ("SILICONFLOW_API_KEY", "k")
            ])
        ),
        "siliconflow-file-asr"
    );
}

#[test]
fn asr_siliconflow_file_missing_key_falls_back_to_mock() {
    assert_eq!(
        resolve_asr_provider_name("s1", &env(&[("ASR_PROVIDER", "siliconflow_file")])),
        "mock-asr"
    );
}

#[test]
fn asr_bailian_realtime_with_key() {
    assert_eq!(
        resolve_asr_provider_name(
            "s1",
            &env(&[
                ("ASR_PROVIDER", "bailian_realtime"),
                ("DASHSCOPE_API_KEY", "k")
            ])
        ),
        "bailian-realtime-asr"
    );
}

#[test]
fn asr_bailian_realtime_missing_key_falls_back_to_mock() {
    assert_eq!(
        resolve_asr_provider_name("s1", &env(&[("ASR_PROVIDER", "bailian_realtime")])),
        "mock-asr"
    );
}

#[test]
fn asr_manual_settings_override_env() {
    let settings = ProviderSettings {
        llm: None,
        asr: Some(AsrProviderSettings {
            provider: "bailian_realtime".into(),
            api_key: Some("manual-key".into()),
            base_url: None,
            model: None,
            language_hint: None,
            max_sentence_silence_ms: None,
            heartbeat: None,
        }),
    };

    assert_eq!(
        resolve_asr_provider_name_with_settings(
            "s1",
            &env(&[
                ("ASR_PROVIDER", "openai_realtime"),
                ("OPENAI_API_KEY", "env-key")
            ]),
            &settings,
        ),
        "bailian-realtime-asr"
    );
}

#[test]
fn provider_config_update_without_api_key_preserves_existing_key() {
    let existing = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "openai".into(),
            api_key: Some("old-key".into()),
            base_url: None,
            model: Some("old-model".into()),
        }),
        asr: None,
    };
    let update = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "openai".into(),
            api_key: None,
            base_url: None,
            model: Some("new-model".into()),
        }),
        asr: None,
    };

    let merged = merge_provider_settings(existing, update);

    assert_eq!(merged.llm.unwrap().api_key.as_deref(), Some("old-key"));
}

#[test]
fn provider_config_update_does_not_reuse_key_for_different_provider() {
    let existing = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "openai".into(),
            api_key: Some("old-key".into()),
            base_url: None,
            model: None,
        }),
        asr: None,
    };
    let update = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "siliconflow".into(),
            api_key: None,
            base_url: None,
            model: None,
        }),
        asr: None,
    };

    let merged = merge_provider_settings(existing, update);

    assert!(merged.llm.unwrap().api_key.is_none());
}
