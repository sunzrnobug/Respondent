pub mod asr;
pub mod audio;
pub mod commands;
pub mod llm;
pub mod session;
pub mod telemetry;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(commands::SessionManager::default())
        .setup(|app| {
            let db = commands::PersistentSessionDb::open(app.handle())
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_audio_output_devices,
            commands::start_session,
            commands::end_session,
            commands::export_session_markdown,
            commands::export_session_text
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
