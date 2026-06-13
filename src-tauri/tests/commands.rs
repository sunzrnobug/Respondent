use respondent_lib::commands::{end_session_for_test, start_session_for_test, SystemStatusEvent};

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
