use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

fn default_output_device() -> OutputDevice {
    OutputDevice {
        id: "default-output".into(),
        name: "Default output device".into(),
        is_default: true,
    }
}

#[cfg(target_os = "windows")]
pub fn list_output_devices() -> Vec<OutputDevice> {
    list_output_devices_windows().unwrap_or_else(|_| vec![default_output_device()])
}

#[cfg(not(target_os = "windows"))]
pub fn list_output_devices() -> Vec<OutputDevice> {
    vec![default_output_device()]
}

#[cfg(target_os = "windows")]
fn list_output_devices_windows() -> windows::core::Result<Vec<OutputDevice>> {
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        // COM may already be initialized on this thread; ignore the result.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let default_device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let default_id = default_device
            .GetId()?
            .to_string()
            .unwrap_or_else(|_| "default-output".to_string());

        Ok(vec![OutputDevice {
            id: default_id,
            name: "Default output device".into(),
            is_default: true,
        }])
    }
}
