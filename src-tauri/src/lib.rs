mod commands;
mod db;
mod dictation;
mod hotkey;
mod inject;
mod overlay;
mod state;
mod tray;

use std::sync::{Arc, Mutex};

use state::AppState;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .format_timestamp_secs()
    .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::list_mics,
            commands::get_mic,
            commands::set_mic,
            commands::models_status,
            commands::ensure_models,
            commands::start_recording,
            commands::stop_recording,
            commands::cancel_recording,
            commands::is_recording,
            commands::get_settings,
            commands::set_settings,
            commands::complete_onboarding,
            commands::get_history,
            commands::delete_history,
            commands::clear_history,
            commands::get_dictionary,
            commands::add_dictionary_word,
            commands::remove_dictionary_word,
            commands::paste_clipboard,
            commands::show_overlay,
        ])
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("app data dir must exist");
            std::fs::create_dir_all(&data_dir)?;

            let conn = db::open(&data_dir.join("opendictate.db"))
                .map_err(|e| std::io::Error::other(format!("failed to open db: {e}")))?;
            let settings = db::load_settings(&conn);

            let state = AppState {
                recorder: Arc::new(opendictate_core::audio::capture::AudioRecorder::new()),
                test_mode: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                db: Arc::new(Mutex::new(conn)),
                settings: Arc::new(Mutex::new(settings.clone())),
                hotkey: Arc::new(Mutex::new(None)),
            };
            app.manage(state);

            let handle = app.handle();
            tray::build(handle)?;
            let _ = hotkey::register(handle, &handle.state::<AppState>(), &settings.hotkey);
            overlay::hide(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}