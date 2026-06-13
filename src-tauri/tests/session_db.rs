use respondent_lib::session::db::{EventInsert, SessionDb};

#[test]
fn creates_session_and_exports_events() {
    let db = SessionDb::open_in_memory().expect("open db");
    let session_id = db
        .start_session("Customer call", "default-output")
        .expect("start session");

    db.insert_event(EventInsert {
        session_id: session_id.clone(),
        event_type: "transcript".into(),
        text: "What is the timeline?".into(),
        is_final: true,
        started_at_ms: 0,
        ended_at_ms: 1200,
    })
    .expect("insert transcript");

    db.insert_event(EventInsert {
        session_id: session_id.clone(),
        event_type: "suggestion".into(),
        text: "We can deliver the first draft by Friday.".into(),
        is_final: true,
        started_at_ms: 1500,
        ended_at_ms: 2400,
    })
    .expect("insert suggestion");

    db.end_session(&session_id).expect("end session");
    let export = db.load_export(&session_id).expect("load export");

    assert_eq!(export.title, "Customer call");
    assert_eq!(export.events.len(), 2);
    assert_eq!(export.events[0].text, "What is the timeline?");
}

#[test]
fn start_session_with_supplied_id_persists_events() {
    let db = SessionDb::open_in_memory().expect("open db");
    db.start_session_with_id("session-1", "Meeting", "default-output")
        .expect("start session with id");

    db.insert_event(EventInsert {
        session_id: "session-1".into(),
        event_type: "transcript".into(),
        text: "hello".into(),
        is_final: true,
        started_at_ms: 0,
        ended_at_ms: 300,
    })
    .expect("insert transcript");

    let export = db.load_export("session-1").expect("load export");

    assert_eq!(export.id, "session-1");
    assert_eq!(export.title, "Meeting");
    assert_eq!(export.events[0].event_type, "transcript");
    assert_eq!(export.events[0].text, "hello");
}
