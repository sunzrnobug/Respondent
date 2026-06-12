use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};

use crate::audio::frame::AudioFrame;

use super::client::{AsrError, AsrEvent, StreamingAsrClient};
use super::endpointer::{EndpointSignal, EnergyEndpointer};

const OUTPUT_CAPACITY: usize = 256;
/// Max time the worker blocks waiting for the consumer to observe the stop flag.
const FRAME_WAIT: Duration = Duration::from_millis(100);
/// Max time the worker blocks sending one event before giving up (avoids a
/// deadlock if a stopped consumer never drains the output channel).
const SEND_TIMEOUT: Duration = Duration::from_millis(200);

pub struct TranscriptionSession {
    events: Receiver<AsrEvent>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<(), AsrError>>>,
}

impl TranscriptionSession {
    pub fn start(
        session_id: String,
        frames: Receiver<AudioFrame>,
        mut client: Box<dyn StreamingAsrClient>,
        mut endpointer: EnergyEndpointer,
    ) -> TranscriptionSession {
        let (out_tx, out_rx) = bounded::<AsrEvent>(OUTPUT_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);

        let handle = thread::Builder::new()
            .name("asr-transcription".into())
            .spawn(move || {
                run_session(
                    &session_id,
                    &frames,
                    client.as_mut(),
                    &mut endpointer,
                    &out_tx,
                    &thread_stop,
                )
            })
            .expect("spawn asr transcription thread");

        TranscriptionSession {
            events: out_rx,
            stop,
            handle: Some(handle),
        }
    }

    pub fn events(&self) -> Receiver<AsrEvent> {
        self.events.clone()
    }

    pub fn stop(mut self) -> Result<(), AsrError> {
        self.stop.store(true, Ordering::Release);
        self.join()
    }

    fn join(&mut self) -> Result<(), AsrError> {
        match self.handle.take() {
            Some(handle) => handle.join().unwrap_or(Err(AsrError::Closed)),
            None => Ok(()),
        }
    }
}

impl Drop for TranscriptionSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.join();
    }
}

fn run_session(
    session_id: &str,
    frames: &Receiver<AudioFrame>,
    client: &mut dyn StreamingAsrClient,
    endpointer: &mut EnergyEndpointer,
    out: &Sender<AsrEvent>,
    stop: &AtomicBool,
) -> Result<(), AsrError> {
    let client_events = client.events();
    let mut saw_speech = false;

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        let frame = match frames.recv_timeout(FRAME_WAIT) {
            Ok(frame) => frame,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        let signal = endpointer.observe(&frame);
        if matches!(signal, Some(EndpointSignal::StartOfSpeech)) {
            saw_speech = true;
        }

        client.push_frame(&frame)?;
        forward_available(&client_events, out)?;

        if matches!(signal, Some(EndpointSignal::EndOfSpeech)) {
            out.send_timeout(
                AsrEvent::Endpoint {
                    session_id: session_id.to_string(),
                    silence_ms: endpointer.silence_window_ms() as i64,
                    detected_at_ms: frame.captured_at_ms as i64,
                },
                SEND_TIMEOUT,
            )
            .map_err(|_| AsrError::Closed)?;
            client.finalize()?;
            forward_available(&client_events, out)?;
            saw_speech = false;
        }
    }

    if saw_speech {
        client.finalize()?;
        forward_available(&client_events, out)?;
    }
    Ok(())
}

fn forward_available(
    client_events: &Receiver<AsrEvent>,
    out: &Sender<AsrEvent>,
) -> Result<(), AsrError> {
    while let Ok(event) = client_events.try_recv() {
        out.send_timeout(event, SEND_TIMEOUT)
            .map_err(|_| AsrError::Closed)?;
    }
    Ok(())
}
