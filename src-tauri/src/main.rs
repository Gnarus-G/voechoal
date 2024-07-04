// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use audio::ecouter::IsRecordingCtrl;

mod audio;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn record_start(state: tauri::State<'_, IsRecordingCtrl>) {
    state.start();
}

#[tauri::command]
fn record_pause(state: tauri::State<'_, IsRecordingCtrl>) {
    state.pause();
}

#[tauri::command]
fn poll_recordings() -> Result<audio::polling::RecordingsPoll, String> {
    let result = audio::polling::RecordingsPoll::poll().map_err(|err| err.to_string())?;
    eprintln!("[info] serving polled: {:?}", result);
    Ok(result)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(audio::ecouter::setup().unwrap())
        .invoke_handler(tauri::generate_handler![
            record_start,
            record_pause,
            poll_recordings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
