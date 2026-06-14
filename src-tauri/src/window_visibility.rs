use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

#[cfg(desktop)]
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};

const SHOW_DEBOUNCE_MS: u64 = 300;

static LAST_HIDE_MS: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn mark_windows_hidden() {
    LAST_HIDE_MS.store(now_ms(), Ordering::SeqCst);
}

fn should_ignore_show_request() -> bool {
    now_ms().saturating_sub(LAST_HIDE_MS.load(Ordering::SeqCst)) < SHOW_DEBOUNCE_MS
}

#[cfg(desktop)]
fn numpad_enter_shortcut() -> Shortcut {
    Shortcut::new(None, Code::NumpadEnter)
}

#[cfg(desktop)]
fn enable_wake_shortcut(app: &AppHandle) -> Result<(), String> {
    let shortcut = numpad_enter_shortcut();
    if app.global_shortcut().is_registered(shortcut.clone()) {
        return Ok(());
    }
    app.global_shortcut()
        .register(shortcut)
        .map_err(|err| err.to_string())
}

#[cfg(desktop)]
fn disable_wake_shortcut(app: &AppHandle) -> Result<(), String> {
    let shortcut = numpad_enter_shortcut();
    if !app.global_shortcut().is_registered(shortcut.clone()) {
        return Ok(());
    }
    app.global_shortcut()
        .unregister(shortcut)
        .map_err(|err| err.to_string())
}

#[cfg(desktop)]
fn schedule_enable_wake_shortcut(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let _ = enable_wake_shortcut(&app);
        });
    });
}

#[cfg(desktop)]
fn schedule_disable_wake_shortcut(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let _ = disable_wake_shortcut(&app);
        });
    });
}

fn hide_all_windows(app: &AppHandle) -> Result<(), String> {
    for (_, window) in app.webview_windows() {
        window.hide().map_err(|err| err.to_string())?;
    }
    mark_windows_hidden();
    Ok(())
}

pub fn show_main_window(app: &AppHandle) -> Result<(), String> {
    if should_ignore_show_request() {
        return Ok(());
    }

    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口".to_string())?;

    if main.is_visible().map_err(|err| err.to_string())? {
        return Ok(());
    }

    main.show().map_err(|err| err.to_string())?;
    main.set_focus().map_err(|err| err.to_string())?;

    #[cfg(desktop)]
    schedule_disable_wake_shortcut(app);

    Ok(())
}

pub fn handle_global_numpad_enter(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = show_main_window(&handle);
    });
}

#[cfg(desktop)]
pub fn init_global_shortcut_plugin(app: &tauri::AppHandle) -> Result<(), String> {
    let shortcut = numpad_enter_shortcut();
    let shortcut_for_handler = shortcut.clone();
    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, pressed, event| {
                if pressed != &shortcut_for_handler || event.state() != ShortcutState::Pressed {
                    return;
                }
                handle_global_numpad_enter(app);
            })
            .build(),
    )
    .map_err(|err| err.to_string())
}

#[cfg(not(desktop))]
pub fn init_global_shortcut_plugin(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

/// Toggle main-window visibility. Returns the new visibility (`true` = shown).
/// Shared by the IPC command and the tray icon.
pub fn toggle_visibility(app: &AppHandle) -> Result<bool, String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口".to_string())?;
    let visible = main.is_visible().map_err(|err| err.to_string())?;

    if visible {
        hide_all_windows(app)?;
        #[cfg(desktop)]
        schedule_enable_wake_shortcut(app);
        Ok(false)
    } else {
        show_main_window(app)?;
        Ok(true)
    }
}

#[tauri::command]
pub fn toggle_main_window_visibility(app: tauri::AppHandle) -> Result<bool, String> {
    toggle_visibility(&app)
}
