use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TryRecvError};

use crate::asr::client::AsrEvent;

use super::client::{LlmError, ReplyEvent, ReplyGeneration, ReplyPoll, StreamingReplyClient};
use super::reply_trigger::ReplyTrigger;

const OUTPUT_CAPACITY: usize = 256;
/// Max time the worker blocks waiting for input while idle.
const IDLE_WAIT: Duration = Duration::from_millis(100);
/// Max time the worker blocks sending one event before giving up.
const SEND_TIMEOUT: Duration = Duration::from_millis(200);
/// Backoff when an in-flight generation has no token ready yet.
const PENDING_WAIT: Duration = Duration::from_millis(5);

pub struct ReplySession {
    events: Receiver<ReplyEvent>,
    retry: Sender<()>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<(), LlmError>>>,
}

impl ReplySession {
    pub fn start(
        asr_events: Receiver<AsrEvent>,
        client: Box<dyn StreamingReplyClient>,
        trigger: ReplyTrigger,
    ) -> ReplySession {
        let (out_tx, out_rx) = bounded::<ReplyEvent>(OUTPUT_CAPACITY);
        let (retry_tx, retry_rx) = bounded::<()>(8);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);

        let handle = thread::Builder::new()
            .name("llm-reply-session".into())
            .spawn(move || {
                run_session(
                    &asr_events,
                    &retry_rx,
                    client.as_ref(),
                    trigger,
                    &out_tx,
                    &thread_stop,
                )
            })
            .expect("spawn llm reply session thread");

        ReplySession {
            events: out_rx,
            retry: retry_tx,
            stop,
            handle: Some(handle),
        }
    }

    pub fn events(&self) -> Receiver<ReplyEvent> {
        self.events.clone()
    }

    pub fn request_retry(&self) -> Result<(), String> {
        self.retry
            .send(())
            .map_err(|_| "回复会话已关闭".to_string())
    }

    pub fn stop(mut self) -> Result<(), LlmError> {
        self.stop.store(true, Ordering::Release);
        self.join()
    }

    fn join(&mut self) -> Result<(), LlmError> {
        match self.handle.take() {
            Some(handle) => handle.join().unwrap_or(Err(LlmError::Closed)),
            None => Ok(()),
        }
    }
}

impl Drop for ReplySession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.join();
    }
}

fn run_session(
    asr_events: &Receiver<AsrEvent>,
    retry_rx: &Receiver<()>,
    client: &dyn StreamingReplyClient,
    mut trigger: ReplyTrigger,
    out: &Sender<ReplyEvent>,
    stop: &AtomicBool,
) -> Result<(), LlmError> {
    let mut active: Option<Box<dyn ReplyGeneration>> = None;
    let mut active_gen: Option<(String, String)> = None; // (session_id, generation_id)

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        let mut disconnected = false;
        loop {
            match asr_events.try_recv() {
                Ok(event) => {
                    if let Some(request) = trigger.observe(&event) {
                        start_generation(
                            request,
                            client,
                            out,
                            &mut active,
                            &mut active_gen,
                        )?;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        while retry_rx.try_recv().is_ok() {
            if let Some(request) = trigger.retry_last() {
                start_generation(
                    request,
                    client,
                    out,
                    &mut active,
                    &mut active_gen,
                )?;
            }
        }

        if let Some(generation) = active.as_mut() {
            match generation.poll() {
                ReplyPoll::Event(event) => {
                    out.send_timeout(event, SEND_TIMEOUT)
                        .map_err(|_| LlmError::Closed)?;
                }
                ReplyPoll::Pending => thread::sleep(PENDING_WAIT),
                ReplyPoll::Done => {
                    active = None;
                    active_gen = None;
                }
            }
            continue;
        }

        if disconnected {
            break;
        }

        match asr_events.recv_timeout(IDLE_WAIT) {
            Ok(event) => {
                if let Some(request) = trigger.observe(&event) {
                    start_generation(
                        request,
                        client,
                        out,
                        &mut active,
                        &mut active_gen,
                    )?;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}

fn start_generation(
    request: super::client::ReplyRequest,
    client: &dyn StreamingReplyClient,
    out: &Sender<ReplyEvent>,
    active: &mut Option<Box<dyn ReplyGeneration>>,
    active_gen: &mut Option<(String, String)>,
) -> Result<(), LlmError> {
    if let Some((session_id, generation_id)) = active_gen.take() {
        out.send_timeout(
            ReplyEvent::Cancelled {
                session_id,
                generation_id,
                received_at_ms: super::streaming::now_ms(),
            },
            SEND_TIMEOUT,
        )
        .map_err(|_| LlmError::Closed)?;
    }
    active_gen.replace((
        request.session_id.clone(),
        request.generation_id.clone(),
    ));
    *active = Some(client.start(request));
    Ok(())
}
