pub mod asr;
pub mod audio;
pub mod commands;
pub mod docs;
pub mod llm;
pub mod provider_config;
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
            }

            let db = commands::PersistentSessionDb::open(app.handle())
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            let provider_config = commands::ProviderConfigStore::open(app.handle())
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            app.manage(db);
            app.manage(provider_config);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_audio_output_devices,
            commands::start_session,
            commands::end_session,
            commands::export_session_markdown,
            commands::export_session_text,
            commands::save_markdown_file,
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
