pub mod appearance_settings;
pub mod asr;
pub mod audio;
pub mod commands;
pub mod docs;
pub mod llm;
pub mod provider_config;
pub mod reply_style_settings;
pub mod session;
pub mod telemetry;
pub mod window_visibility;

use tauri::Manager;

use crate::docs::store::DocumentStore;
use std::sync::{Arc, Mutex};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(commands::SessionManager::default())
        .manage(Arc::new(Mutex::new(DocumentStore::default())))
        .setup(|app| {
            #[cfg(desktop)]
            {
                window_visibility::init_global_shortcut_plugin(app.handle())?;
                setup_tray(app.handle())?;
            }

            let db = commands::PersistentSessionDb::open(app.handle())
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            let provider_config = commands::ProviderConfigStore::open(app.handle())
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            let appearance_settings = appearance_settings::AppearanceSettingsStore::open(app.handle())
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            let reply_style_settings =
                reply_style_settings::ReplyStyleSettingsStore::open(app.handle())
                    .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            app.manage(db);
            app.manage(provider_config);
            app.manage(appearance_settings);
            app.manage(Arc::new(reply_style_settings));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_audio_output_devices,
            commands::start_session,
            commands::end_session,
            commands::retry_reply,
            commands::export_session_markdown,
            commands::export_session_text,
            commands::save_markdown_file,
            commands::get_appearance_settings,
            commands::publish_appearance_settings,
            commands::get_reply_style_settings,
            commands::save_reply_style_settings,
            commands::get_provider_config,
            commands::list_provider_profiles,
            commands::save_provider_config,
            commands::save_provider_profile,
            commands::activate_provider_profile,
            commands::delete_provider_profile,
            commands::clear_provider_config,
            commands::load_document,
            commands::unload_document,
            commands::list_documents,
            window_visibility::toggle_main_window_visibility
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// System-tray icon: the app lives in the notification area instead of the
/// taskbar. Left-click shows/focuses the window; the menu toggles visibility
/// or quits.
#[cfg(desktop)]
fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let toggle_item = MenuItem::with_id(app, "toggle", "显示 / 隐藏窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("Respondent 会议助手")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                let _ = window_visibility::toggle_visibility(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = window_visibility::show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}
