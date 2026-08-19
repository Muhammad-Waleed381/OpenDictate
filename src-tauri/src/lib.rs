mod commands;
mod autostart;
mod db;
mod dictation;
mod dock;
mod hotkey;
mod inject;
mod notify;
mod state;
mod tray;
pub mod tray_icon;

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
            commands::models_catalog,
            commands::ensure_model,
            commands::remove_model,
            commands::start_recording,
            commands::stop_recording,
            commands::cancel_recording,
            commands::is_recording,
            commands::get_settings,
            commands::set_settings,
            commands::complete_onboarding,
            commands::get_history,
            commands::delete_history,
            commands::update_history,
            commands::clear_history,
            commands::get_dictionary,
            commands::add_dictionary_word,
            commands::remove_dictionary_word,
            commands::paste_clipboard,
            commands::copy_text,
            commands::undo_last_insert,
            commands::warmup_model,
            commands::word_stats,
            commands::reset_word_stats,
            commands::export_history,
        ])
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("app data dir must exist");
            std::fs::create_dir_all(&data_dir)?;

            let socket_path = data_dir.join("toggle.sock");
            if hotkey::is_another_instance(&socket_path) {
                log::info!("another instance is running; exiting");
                std::process::exit(0);
            }

            let conn = db::open(&data_dir.join("opendictate.db"))
                .map_err(|e| std::io::Error::other(format!("failed to open db: {e}")))?;
            let settings = db::load_settings(&conn);

            let state = AppState {
                recorder: Arc::new(opendictate_core::audio::capture::AudioRecorder::new()),
                test_mode: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                db: Arc::new(Mutex::new(conn)),
                settings: Arc::new(Mutex::new(settings.clone())),
                hotkey: Arc::new(Mutex::new(None)),
                continuous: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                stream: Arc::new(Mutex::new(None)),
                stream_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                last_inserted: Arc::new(Mutex::new(None)),
                stt_engine: Arc::new(Mutex::new(None)),
                streaming_engine: Arc::new(Mutex::new(None)),
                vad: Arc::new(Mutex::new(None)),
            };
            app.manage(state);

            let handle = app.handle();
            tray::build(handle)?;
            if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png")) {
                for window in app.webview_windows().values() {
                    let _ = window.set_icon(icon.clone());
                }
            }
            let _ = hotkey::register(handle, &handle.state::<AppState>(), &settings.hotkey);
            hotkey::install_socket_toggle(handle.clone(), socket_path);
            dock::init(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            match window.label() {
                "main" => {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window.hide();
                        let app = window.app_handle();
                        dock::ensure_on_main(app);
                    }
                }
                "dock" => match event {
                    WindowEvent::CloseRequested { api, .. } => api.prevent_close(),
                    WindowEvent::Moved(_)
                    | WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. }
                    | WindowEvent::Focused(_) => {
                        let app = window.app_handle();
                        dock::ensure_on_main(app);
                    }
                    _ => {}
                },
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
