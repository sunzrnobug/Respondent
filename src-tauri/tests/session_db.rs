use respondent_lib::session::db::{EventInsert, SessionDb};
use respondent_lib::session::saved::{SavedSession, SessionTurn};

fn init_test_backends() {
    std::env::set_var("RESPONDENT_SECRET_BACKEND", "memory");
}

#[test]
fn creates_session_and_exports_events() {
    init_test_backends();
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
    init_test_backends();
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

#[test]
fn deletes_and_purges_old_sessions() {
    init_test_backends();
    let db = SessionDb::open_in_memory().expect("open db");
    let old_id = db
        .start_session("Old call", "default-output")
        .expect("start old session");
    db.end_session(&old_id).expect("end old session");
    db.set_session_times_for_test(&old_id, "2020-01-01T00:00:00Z", "2020-01-01T00:00:00Z")
        .expect("age old session");

    let recent_id = db
        .start_session("Recent call", "default-output")
        .expect("start recent session");
    db.end_session(&recent_id).expect("end recent session");

    let purged = db
        .purge_sessions_older_than_days(90)
        .expect("purge old sessions");
    assert_eq!(purged, 1);

    let sessions = db.list_sessions().expect("list sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, recent_id);

    db.delete_session(&recent_id).expect("delete recent session");
    assert!(db.list_sessions().expect("list sessions").is_empty());
}

#[test]
fn persists_saved_sessions_in_encrypted_database() {
    init_test_backends();
    let db = SessionDb::open_in_memory().expect("open db");
    let saved = SavedSession {
        id: "saved-1".into(),
        title: "Customer call".into(),
        date: "2026-06-15".into(),
        started_at: "2026-06-15T10:00:00Z".into(),
        ended_at: "2026-06-15T10:05:00Z".into(),
        turns: vec![SessionTurn {
            transcript: "hello".into(),
            suggestion: Some("reply".into()),
        }],
        system_messages: vec!["ready".into()],
    };

    db.upsert_saved_session(&saved).expect("upsert saved session");
    let loaded = db.list_saved_sessions().expect("list saved sessions");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, "saved-1");
    assert_eq!(loaded[0].turns[0].transcript, "hello");

    db.delete_saved_session("saved-1")
        .expect("delete saved session");
    assert!(db
        .list_saved_sessions()
        .expect("list saved sessions")
        .is_empty());
}
