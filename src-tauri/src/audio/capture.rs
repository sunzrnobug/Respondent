use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use thiserror::Error;

use super::frame::{AudioFrame, PcmFormat};

#[cfg(target_os = "windows")]
use super::convert::CapturePipeline;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("unsupported capture configuration: {0}")]
    Unsupported(String),
    #[error("capture thread join failed: {0}")]
    ThreadJoin(String),
    #[cfg(target_os = "windows")]
    #[error("windows audio error: {0}")]
    Com(#[from] windows::core::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    Float32,
    Pcm16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasapiFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub sample_format: SampleFormat,
}

impl WasapiFormat {
    pub fn new(
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
        sample_format: SampleFormat,
    ) -> Result<Self, CaptureError> {
        if sample_rate == 0 {
            return Err(CaptureError::Unsupported(
                "unsupported WASAPI format: sample_rate=0".into(),
            ));
        }
        if channels == 0 {
            return Err(CaptureError::Unsupported(
                "unsupported WASAPI format: channels=0".into(),
            ));
        }

        match (sample_format, bits_per_sample) {
            (SampleFormat::Float32, 32) | (SampleFormat::Pcm16, 16) => Ok(Self {
                sample_rate,
                channels,
                bits_per_sample,
                sample_format,
            }),
            _ => Err(CaptureError::Unsupported(format!(
                "unsupported WASAPI format: sample_format={sample_format:?}, bits_per_sample={bits_per_sample}",
            ))),
        }
    }
}

pub struct LoopbackCapture {
    sender: Sender<AudioFrame>,
    receiver: Receiver<AudioFrame>,
    dropped_frames: Arc<AtomicU64>,
    stop_flag: Option<Arc<AtomicBool>>,
    thread_handle: Option<JoinHandle<Result<(), CaptureError>>>,
    #[cfg(target_os = "windows")]
    wake_event: Option<OwnedHandle>,
}

impl LoopbackCapture {
    pub fn new_for_device(_device_id: &str) -> Self {
        Self::new_for_test_with_capacity(128)
    }

    pub fn new_for_test_with_capacity(capacity: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        Self {
            sender,
            receiver,
            dropped_frames: Arc::new(AtomicU64::new(0)),
            stop_flag: None,
            thread_handle: None,
            #[cfg(target_os = "windows")]
            wake_event: None,
        }
    }

    pub fn start(device_id: &str) -> Result<Self, CaptureError> {
        start_platform_capture(device_id)
    }

    pub fn receiver(&self) -> Receiver<AudioFrame> {
        self.receiver.clone()
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames.load(Ordering::Relaxed)
    }

    pub fn stop(mut self) -> Result<(), CaptureError> {
        self.signal_stop()?;
        self.join_thread()
    }

    pub fn push_test_frame(&self, captured_at_ms: u64) {
        send_or_drop_newest(
            &self.sender,
            &self.dropped_frames,
            build_test_frame(captured_at_ms),
        );
    }

    fn join_thread(&mut self) -> Result<(), CaptureError> {
        let Some(thread_handle) = self.thread_handle.take() else {
            return Ok(());
        };

        match thread_handle.join() {
            Ok(result) => result,
            Err(payload) => Err(CaptureError::ThreadJoin(panic_payload_to_string(payload))),
        }
    }

    fn signal_stop(&self) -> Result<(), CaptureError> {
        if let Some(stop_flag) = &self.stop_flag {
            stop_flag.store(true, Ordering::Release);
        }

        #[cfg(target_os = "windows")]
        if let Some(wake_event) = &self.wake_event {
            wake_event.signal()?;
        }

        Ok(())
    }
}

impl Drop for LoopbackCapture {
    fn drop(&mut self) {
        let _ = self.signal_stop();
        let _ = self.join_thread();
    }
}

fn build_test_frame(captured_at_ms: u64) -> AudioFrame {
    AudioFrame {
        format: PcmFormat {
            sample_rate: 16_000,
            channels: 1,
            bits_per_sample: 16,
        },
        samples: vec![0; 320],
        captured_at_ms,
    }
}

fn send_or_drop_newest(
    sender: &Sender<AudioFrame>,
    dropped_frames: &Arc<AtomicU64>,
    frame: AudioFrame,
) {
    match sender.try_send(frame) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
            dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "capture thread panicked".to_string()
    }
}

#[cfg(not(target_os = "windows"))]
fn start_platform_capture(_device_id: &str) -> Result<LoopbackCapture, CaptureError> {
    Err(CaptureError::Unsupported(
        "event-driven loopback requires Windows 10 or later".into(),
    ))
}

#[cfg(target_os = "windows")]
fn start_platform_capture(device_id: &str) -> Result<LoopbackCapture, CaptureError> {
    use crossbeam_channel::bounded as bounded_once;

    let (sender, receiver) = bounded(128);
    let dropped_frames = Arc::new(AtomicU64::new(0));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let wake_event = OwnedHandle::create_auto_reset()?;
    let raw_wake_event = wake_event.raw().0 as isize;
    let device_id = device_id.to_string();

    let thread_sender = sender.clone();
    let thread_dropped_frames = Arc::clone(&dropped_frames);
    let thread_stop_flag = Arc::clone(&stop_flag);
    let (ready_tx, ready_rx) = bounded_once(1);

    let thread_handle = thread::Builder::new()
        .name("wasapi-loopback-capture".into())
        .spawn(move || {
            run_capture_thread(
                &device_id,
                windows::Win32::Foundation::HANDLE(raw_wake_event as *mut _),
                &thread_sender,
                &thread_dropped_frames,
                &thread_stop_flag,
                ready_tx,
            )
        })
        .map_err(|error| CaptureError::ThreadJoin(error.to_string()))?;

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(LoopbackCapture {
            sender,
            receiver,
            dropped_frames,
            stop_flag: Some(stop_flag),
            thread_handle: Some(thread_handle),
            wake_event: Some(wake_event),
        }),
        Ok(Err(_)) | Err(_) => match thread_handle.join() {
            Ok(result) => match result {
                Ok(()) => Err(CaptureError::ThreadJoin(
                    "capture thread exited before signalling startup".into(),
                )),
                Err(error) => Err(error),
            },
            Err(payload) => Err(CaptureError::ThreadJoin(panic_payload_to_string(payload))),
        },
    }
}

#[cfg(target_os = "windows")]
fn run_capture_thread(
    device_id: &str,
    wake_event: windows::Win32::Foundation::HANDLE,
    sender: &Sender<AudioFrame>,
    dropped_frames: &Arc<AtomicU64>,
    stop_flag: &Arc<AtomicBool>,
    ready_tx: crossbeam_channel::Sender<Result<(), String>>,
) -> Result<(), CaptureError> {
    use std::time::Instant;

    use windows::core::{Error, HRESULT};
    use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::Media::Audio::{
        AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::WaitForSingleObject;

    const WAIT_TIMEOUT_MS: u32 = 200;

    struct ComGuard;

    impl ComGuard {
        fn initialize() -> Result<Self, CaptureError> {
            let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if result == HRESULT(0) || result == S_FALSE {
                Ok(Self)
            } else if result == RPC_E_CHANGED_MODE {
                Err(CaptureError::Unsupported(
                    "COM apartment already initialized with incompatible mode".into(),
                ))
            } else {
                Err(Error::from(result).into())
            }
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    struct MixFormatPtr(*mut WAVEFORMATEX);

    impl MixFormatPtr {
        fn as_ptr(&self) -> *const WAVEFORMATEX {
            self.0
        }
    }

    impl Drop for MixFormatPtr {
        fn drop(&mut self) {
            unsafe {
                CoTaskMemFree(Some(self.0.cast()));
            }
        }
    }

    let thread_result = (|| -> Result<(), CaptureError> {
        let _com_guard = ComGuard::initialize()?;
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
        let device = select_render_device(&enumerator, device_id)?;
        let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }?;
        let mix_format = MixFormatPtr(unsafe { audio_client.GetMixFormat()? });
        let wasapi_format = parse_wave_format(mix_format.as_ptr())?;

        unsafe {
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                0,
                0,
                mix_format.as_ptr(),
                None,
            )?;
            audio_client.SetEventHandle(wake_event)?;
        }

        let capture_client: IAudioCaptureClient = unsafe { audio_client.GetService()? };
        let mut pipeline = CapturePipeline::new(wasapi_format.sample_rate, wasapi_format.channels);
        let started_at = Instant::now();

        unsafe {
            audio_client.Start()?;
        }
        let _ = ready_tx.send(Ok(()));

        loop {
            if stop_flag.load(Ordering::Acquire) {
                break;
            }

            let wait_result = unsafe { WaitForSingleObject(wake_event, WAIT_TIMEOUT_MS) };
            if wait_result == WAIT_TIMEOUT {
                continue;
            }
            if wait_result == WAIT_FAILED {
                let stop_result = unsafe { audio_client.Stop() };
                return match stop_result {
                    Ok(()) => Err(Error::from_win32().into()),
                    Err(error) => Err(error.into()),
                };
            }
            if wait_result != WAIT_OBJECT_0 {
                unsafe {
                    audio_client.Stop()?;
                }
                return Err(CaptureError::ThreadJoin(format!(
                    "unexpected wait result: {}",
                    wait_result.0
                )));
            }
            if stop_flag.load(Ordering::Acquire) {
                break;
            }

            loop {
                let packet_frames = unsafe { capture_client.GetNextPacketSize()? };
                if packet_frames == 0 {
                    break;
                }

                let mut packet_data = std::ptr::null_mut();
                let mut frames_available = 0u32;
                let mut flags = 0u32;
                unsafe {
                    capture_client.GetBuffer(
                        &mut packet_data,
                        &mut frames_available,
                        &mut flags,
                        None,
                        None,
                    )?;
                }

                let conversion_result = (|| -> Result<(), CaptureError> {
                    let captured_at_ms = started_at.elapsed().as_millis() as u64;
                    let interleaved = if (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 {
                        silent_packet(frames_available, wasapi_format.channels)?
                    } else {
                        packet_to_f32(packet_data, frames_available, wasapi_format)?
                    };

                    for frame in pipeline.push_interleaved_f32(&interleaved, captured_at_ms) {
                        send_or_drop_newest(sender, dropped_frames, frame);
                    }

                    Ok(())
                })();

                let release_result = unsafe { capture_client.ReleaseBuffer(frames_available) };
                if let Err(error) = release_result {
                    let _ = unsafe { audio_client.Stop() };
                    return Err(error.into());
                }
                conversion_result?;
            }
        }

        unsafe {
            audio_client.Stop()?;
        }
        Ok(())
    })();

    if let Err(error) = &thread_result {
        let _ = ready_tx.send(Err(error.to_string()));
    }

    thread_result
}

#[cfg(target_os = "windows")]
const WAVE_FORMAT_PCM_TAG: u16 = 0x0001;

#[cfg(target_os = "windows")]
const WAVE_FORMAT_IEEE_FLOAT_TAG: u16 = 0x0003;

#[cfg(target_os = "windows")]
const IEEE_FLOAT_SUBTYPE: windows::core::GUID =
    windows::core::GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

#[cfg(target_os = "windows")]
fn select_render_device(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
    device_id: &str,
) -> Result<windows::Win32::Media::Audio::IMMDevice, CaptureError> {
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::{eConsole, eRender};

    if device_id.is_empty() || device_id == "default-output" {
        return Ok(unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? });
    }

    let wide_device_id = wide_null(device_id);
    let requested = unsafe { enumerator.GetDevice(PCWSTR(wide_device_id.as_ptr())) };
    match requested {
        Ok(device) => Ok(device),
        Err(_) => Ok(unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? }),
    }
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn parse_wave_format(
    format_ptr: *const windows::Win32::Media::Audio::WAVEFORMATEX,
) -> Result<WasapiFormat, CaptureError> {
    use std::ptr;
    use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;
    use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};

    let format = unsafe { ptr::read_unaligned(format_ptr) };
    match format.wFormatTag as u32 {
        tag if tag == WAVE_FORMAT_PCM_TAG as u32 => WasapiFormat::new(
            format.nSamplesPerSec,
            format.nChannels,
            format.wBitsPerSample,
            SampleFormat::Pcm16,
        ),
        tag if tag == WAVE_FORMAT_IEEE_FLOAT_TAG as u32 => WasapiFormat::new(
            format.nSamplesPerSec,
            format.nChannels,
            format.wBitsPerSample,
            SampleFormat::Float32,
        ),
        WAVE_FORMAT_EXTENSIBLE => {
            let extensible = unsafe { ptr::read_unaligned(format_ptr.cast::<WAVEFORMATEXTENSIBLE>()) };
            let cb_size = format.cbSize;
            let sub_format = extensible.SubFormat;
            if cb_size < 22 {
                return Err(CaptureError::Unsupported(format!(
                    "unsupported WASAPI extensible format: cbSize={}",
                    cb_size
                )));
            }

            if sub_format == IEEE_FLOAT_SUBTYPE {
                WasapiFormat::new(
                    format.nSamplesPerSec,
                    format.nChannels,
                    format.wBitsPerSample,
                    SampleFormat::Float32,
                )
            } else if sub_format == KSDATAFORMAT_SUBTYPE_PCM {
                WasapiFormat::new(
                    format.nSamplesPerSec,
                    format.nChannels,
                    format.wBitsPerSample,
                    SampleFormat::Pcm16,
                )
            } else {
                Err(CaptureError::Unsupported(format!(
                    "unsupported WASAPI extensible subformat: {:?}",
                    sub_format
                )))
            }
        }
        other => Err(CaptureError::Unsupported(format!(
            "unsupported WASAPI format tag: {other}"
        ))),
    }
}

#[cfg(target_os = "windows")]
fn silent_packet(frames: u32, channels: u16) -> Result<Vec<f32>, CaptureError> {
    let samples = sample_len(frames, channels)?;
    Ok(vec![0.0; samples])
}

#[cfg(target_os = "windows")]
fn packet_to_f32(
    packet_data: *mut u8,
    frames: u32,
    format: WasapiFormat,
) -> Result<Vec<f32>, CaptureError> {
    use std::slice;

    let sample_count = sample_len(frames, format.channels)?;

    match format.sample_format {
        SampleFormat::Float32 => {
            let data = unsafe { slice::from_raw_parts(packet_data.cast::<f32>(), sample_count) };
            Ok(data.to_vec())
        }
        SampleFormat::Pcm16 => {
            let data = unsafe { slice::from_raw_parts(packet_data.cast::<i16>(), sample_count) };
            Ok(data
                .iter()
                .map(|sample| (*sample as f32 / i16::MAX as f32).clamp(-1.0, 1.0))
                .collect())
        }
    }
}

#[cfg(target_os = "windows")]
fn sample_len(frames: u32, channels: u16) -> Result<usize, CaptureError> {
    let frames = usize::try_from(frames)
        .map_err(|_| CaptureError::Unsupported("frame count does not fit in usize".into()))?;
    frames
        .checked_mul(channels as usize)
        .ok_or_else(|| CaptureError::Unsupported("sample count overflow".into()))
}

#[cfg(target_os = "windows")]
struct OwnedHandle(windows::Win32::Foundation::HANDLE);

// Windows HANDLE values are process-wide and may be signaled or closed from a
// different thread. OwnedHandle owns exactly one HANDLE and only closes it on
// drop, so moving that ownership into Tauri-managed session state is safe.
#[cfg(target_os = "windows")]
unsafe impl Send for OwnedHandle {}

#[cfg(target_os = "windows")]
impl OwnedHandle {
    fn create_auto_reset() -> Result<Self, CaptureError> {
        use windows::core::PCWSTR;
        use windows::Win32::System::Threading::CreateEventW;

        let handle = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }?;
        Ok(Self(handle))
    }

    fn raw(&self) -> windows::Win32::Foundation::HANDLE {
        self.0
    }

    fn signal(&self) -> Result<(), CaptureError> {
        use windows::Win32::System::Threading::SetEvent;

        unsafe {
            SetEvent(self.0)?;
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
        }
    }
}
