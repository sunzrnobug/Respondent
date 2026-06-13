use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use respondent_lib::asr::bailian_realtime::{
    BailianRealtimeAsrClient, BailianRealtimeConfig, BailianRealtimeTransport,
};
use respondent_lib::asr::client::{AsrError, AsrEvent, StreamingAsrClient};
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct RecordingHandle {
    sent_json: Arc<Mutex<Vec<Value>>>,
    sent_binary: Arc<Mutex<Vec<Vec<u8>>>>,
    recv: Arc<Mutex<VecDeque<Value>>>,
}

struct RecordingTransport {
    sent_json: Arc<Mutex<Vec<Value>>>,
    sent_binary: Arc<Mutex<Vec<Vec<u8>>>>,
    recv: Arc<Mutex<VecDeque<Value>>>,
}

impl RecordingTransport {
    fn new() -> (RecordingHandle, Self) {
        let handle = RecordingHandle::default();
        (
            handle.clone(),
            Self {
                sent_json: Arc::clone(&handle.sent_json),
                sent_binary: Arc::clone(&handle.sent_binary),
                recv: Arc::clone(&handle.recv),
            },
        )
    }
}

impl RecordingHandle {
    fn sent_json(&self) -> Vec<Value> {
        self.sent_json.lock().expect("json lock").clone()
    }

    fn sent_binary(&self) -> Vec<Vec<u8>> {
        self.sent_binary.lock().expect("binary lock").clone()
    }

    fn queue(&self, value: Value) {
        self.recv.lock().expect("recv lock").push_back(value);
    }
}

impl BailianRealtimeTransport for RecordingTransport {
    fn send_json(&mut self, value: Value) -> Result<(), AsrError> {
        if value["header"]["action"].as_str() == Some("finish-task") {
            let task_id = value["header"]["task_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            self.recv.lock().expect("recv lock").push_back(json!({
                "header": {"task_id": task_id, "event": "task-finished", "attributes": {}},
                "payload": {}
            }));
        }
        self.sent_json.lock().expect("json lock").push(value);
        Ok(())
    }

    fn send_binary(&mut self, bytes: Vec<u8>) -> Result<(), AsrError> {
        self.sent_binary.lock().expect("binary lock").push(bytes);
        Ok(())
    }

    fn try_recv_json(&mut self) -> Result<Option<Value>, AsrError> {
        Ok(self.recv.lock().expect("recv lock").pop_front())
    }

    fn close(&mut self) -> Result<(), AsrError> {
        Ok(())
    }
}

fn config() -> BailianRealtimeConfig {
    BailianRealtimeConfig {
        api_key: "test-key".to_string(),
        model: "fun-asr-realtime".to_string(),
        sample_rate: 16_000,
        format: "pcm".to_string(),
        language_hint: Some("zh".to_string()),
        max_sentence_silence_ms: Some(600),
        heartbeat: true,
    }
}

fn mono_16k_frame(samples: Vec<i16>, captured_at_ms: u64) -> AudioFrame {
    AudioFrame {
        format: PcmFormat {
            sample_rate: 16_000,
            channels: 1,
            bits_per_sample: 16,
        },
        samples,
        captured_at_ms,
    }
}

#[test]
fn new_sends_run_task_with_official_fun_asr_shape() {
    let (handle, transport) = RecordingTransport::new();

    let client =
        BailianRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");

    assert_eq!(client.name(), "bailian-realtime-asr");

    let sent = handle.sent_json();
    assert_eq!(sent.len(), 1);
    let run_task = &sent[0];
    assert_eq!(run_task["header"]["action"], "run-task");
    assert_eq!(run_task["header"]["streaming"], "duplex");
    assert!(run_task["header"]["task_id"]
        .as_str()
        .is_some_and(|id| !id.is_empty()));
    assert_eq!(run_task["payload"]["task_group"], "audio");
    assert_eq!(run_task["payload"]["task"], "asr");
    assert_eq!(run_task["payload"]["function"], "recognition");
    assert_eq!(run_task["payload"]["model"], "fun-asr-realtime");
    assert_eq!(run_task["payload"]["input"], json!({}));
    assert_eq!(run_task["payload"]["parameters"]["format"], "pcm");
    assert_eq!(run_task["payload"]["parameters"]["sample_rate"], 16000);
    assert_eq!(
        run_task["payload"]["parameters"]["language_hints"],
        json!(["zh"])
    );
    assert_eq!(
        run_task["payload"]["parameters"]["max_sentence_silence"],
        600
    );
    assert_eq!(run_task["payload"]["parameters"]["heartbeat"], true);
}

#[test]
fn push_frame_waits_for_task_started_before_sending_binary_audio() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        BailianRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");

    client
        .push_frame(&mono_16k_frame(vec![1, -2, 3], 100))
        .expect("push before start");
    assert!(handle.sent_binary().is_empty());

    let task_id = handle.sent_json()[0]["header"]["task_id"]
        .as_str()
        .expect("task id")
        .to_string();
    handle.queue(json!({
        "header": {
            "task_id": task_id,
            "event": "task-started",
            "attributes": {}
        },
        "payload": {}
    }));

    client
        .push_frame(&mono_16k_frame(vec![1, -2, 3], 120))
        .expect("push after start");

    assert_eq!(handle.sent_binary(), vec![vec![1, 0, 254, 255, 3, 0]]);
}

#[test]
fn result_generated_emits_partial_and_final_with_provider_timestamps() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        BailianRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");
    let events = client.events();
    let task_id = handle.sent_json()[0]["header"]["task_id"]
        .as_str()
        .expect("task id")
        .to_string();

    handle.queue(json!({
        "header": {"task_id": task_id, "event": "task-started", "attributes": {}},
        "payload": {}
    }));
    handle.queue(json!({
        "header": {"task_id": task_id, "event": "result-generated", "attributes": {}},
        "payload": {
            "output": {
                "sentence": {
                    "begin_time": 170,
                    "end_time": 920,
                    "text": "你好",
                    "heartbeat": false,
                    "sentence_end": false,
                    "sentence_id": 1,
                    "words": []
                }
            },
            "usage": null
        }
    }));
    handle.queue(json!({
        "header": {"task_id": task_id, "event": "result-generated", "attributes": {}},
        "payload": {
            "output": {
                "sentence": {
                    "begin_time": 170,
                    "end_time": 1100,
                    "text": "你好世界",
                    "heartbeat": false,
                    "sentence_end": true,
                    "sentence_id": 1,
                    "words": []
                }
            },
            "usage": {"duration": 2}
        }
    }));

    client
        .push_frame(&mono_16k_frame(vec![0; 320], 100))
        .expect("drain");

    let collected = events.try_iter().collect::<Vec<_>>();
    assert!(matches!(
        &collected[0],
        AsrEvent::Partial {
            text,
            started_at_ms: 170,
            ended_at_ms: 920,
            ..
        } if text == "你好"
    ));
    assert!(matches!(
        &collected[1],
        AsrEvent::Final {
            text,
            started_at_ms: 170,
            ended_at_ms: 1100,
            ..
        } if text == "你好世界"
    ));
}

#[test]
fn heartbeat_result_is_ignored() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        BailianRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");
    let events = client.events();
    let task_id = handle.sent_json()[0]["header"]["task_id"]
        .as_str()
        .expect("task id")
        .to_string();

    handle.queue(json!({
        "header": {"task_id": task_id, "event": "result-generated", "attributes": {}},
        "payload": {
            "output": {
                "sentence": {
                    "begin_time": 0,
                    "end_time": 0,
                    "text": "",
                    "heartbeat": true,
                    "sentence_end": false,
                    "sentence_id": 0,
                    "words": []
                }
            },
            "usage": null
        }
    }));

    client
        .push_frame(&mono_16k_frame(vec![0; 320], 100))
        .expect("drain");

    assert!(events.try_iter().next().is_none());
}

#[test]
fn finalize_sends_finish_task_then_starts_new_task() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        BailianRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");
    let first_task_id = handle.sent_json()[0]["header"]["task_id"]
        .as_str()
        .expect("task id")
        .to_string();

    client.finalize().expect("finish");

    let sent = handle.sent_json();
    assert_eq!(sent.len(), 3);
    assert_eq!(sent[1]["header"]["action"], "finish-task");
    assert_eq!(sent[1]["header"]["streaming"], "duplex");
    assert_eq!(sent[1]["header"]["task_id"], first_task_id);
    assert_eq!(sent[1]["payload"]["input"], json!({}));
    assert_eq!(sent[2]["header"]["action"], "run-task");
    assert_ne!(sent[2]["header"]["task_id"], first_task_id);
}

#[test]
fn second_utterance_works_after_finalize() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        BailianRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");
    let events = client.events();
    let first_task_id = handle.sent_json()[0]["header"]["task_id"]
        .as_str()
        .expect("task id")
        .to_string();

    handle.queue(json!({
        "header": {"task_id": first_task_id, "event": "task-started", "attributes": {}},
        "payload": {}
    }));
    client
        .push_frame(&mono_16k_frame(vec![1, 2], 100))
        .expect("first push");
    assert_eq!(handle.sent_binary().len(), 1);

    client.finalize().expect("finalize first utterance");

    let second_task_id = handle.sent_json()[2]["header"]["task_id"]
        .as_str()
        .expect("second task id")
        .to_string();
    handle.queue(json!({
        "header": {"task_id": second_task_id, "event": "task-started", "attributes": {}},
        "payload": {}
    }));
    handle.queue(json!({
        "header": {"task_id": second_task_id, "event": "result-generated", "attributes": {}},
        "payload": {
            "output": {
                "sentence": {
                    "begin_time": 0,
                    "end_time": 500,
                    "text": "第二轮",
                    "heartbeat": false,
                    "sentence_end": true,
                    "sentence_id": 1,
                    "words": []
                }
            },
            "usage": null
        }
    }));

    client
        .push_frame(&mono_16k_frame(vec![3, 4], 200))
        .expect("second push");
    assert_eq!(handle.sent_binary().len(), 2);

    let collected = events.try_iter().collect::<Vec<_>>();
    assert!(matches!(
        &collected[0],
        AsrEvent::Final { text, .. } if text == "第二轮"
    ));
}

#[test]
fn stale_task_finished_from_previous_task_does_not_block_new_task_audio() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        BailianRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");
    let first_task_id = handle.sent_json()[0]["header"]["task_id"]
        .as_str()
        .expect("task id")
        .to_string();

    handle.queue(json!({
        "header": {"task_id": first_task_id, "event": "task-started", "attributes": {}},
        "payload": {}
    }));
    client
        .push_frame(&mono_16k_frame(vec![1, 2], 100))
        .expect("first push");
    assert_eq!(handle.sent_binary().len(), 1);

    client.finalize().expect("finalize first utterance");

    let second_task_id = handle.sent_json()[2]["header"]["task_id"]
        .as_str()
        .expect("second task id")
        .to_string();
    handle.queue(json!({
        "header": {"task_id": first_task_id, "event": "task-finished", "attributes": {}},
        "payload": {}
    }));
    handle.queue(json!({
        "header": {"task_id": second_task_id, "event": "task-started", "attributes": {}},
        "payload": {}
    }));

    client
        .push_frame(&mono_16k_frame(vec![3, 4], 200))
        .expect("second push");
    assert_eq!(handle.sent_binary().len(), 2);
}

#[test]
fn task_failed_returns_provider_error_without_secret() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        BailianRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");
    let task_id = handle.sent_json()[0]["header"]["task_id"]
        .as_str()
        .expect("task id")
        .to_string();
    handle.queue(json!({
        "header": {
            "task_id": task_id,
            "event": "task-failed",
            "error_code": "CLIENT_ERROR",
            "error_message": "bad audio",
            "attributes": {}
        },
        "payload": {}
    }));

    let err = client
        .push_frame(&mono_16k_frame(vec![0; 320], 100))
        .expect_err("provider error");

    let message = err.to_string();
    assert!(message.contains("CLIENT_ERROR"));
    assert!(message.contains("bad audio"));
    assert!(!message.contains("test-key"));
}

#[test]
fn whitespace_api_key_is_rejected() {
    let (_, transport) = RecordingTransport::new();

    let result = BailianRealtimeAsrClient::with_transport(
        "s1".to_string(),
        BailianRealtimeConfig::from_api_key("   "),
        Box::new(transport),
    );

    match result {
        Err(AsrError::Provider(message)) => assert_eq!(message, "missing DASHSCOPE_API_KEY"),
        Err(other) => panic!("expected provider error, got {other:?}"),
        Ok(_) => panic!("blank keys should be rejected"),
    }
}
