pub mod audio;
pub mod background;
pub mod sharedref;

use audio::AudioCtrls;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn record_start(state: tauri::State<'_, AudioCtrls>) {
    let id = cuid2::cuid();
    state
        .ecouter
        .trigger(audio::StreamControlCommand::Play(id.clone()));

    state
        .sttlistener
        .trigger(audio::StreamControlCommand::Play(id));
}

#[tauri::command]
fn record_pause(state: tauri::State<'_, AudioCtrls>) {
    state
        .ecouter
        .trigger(audio::StreamControlCommand::Pause(None));

    state
        .sttlistener
        .trigger(audio::StreamControlCommand::Pause(None));
}

#[tauri::command]
fn poll_recordings(
    state: tauri::State<'_, AudioCtrls>,
) -> Result<audio::polling::RecordingsPoll, String> {
    let result = audio::polling::RecordingsPoll::poll(&state.db.lock().unwrap())
        .map_err(|err| err.to_string())?;
    // eprintln!("[info] serving polled: {:?}", result);
    Ok(result)
}

#[tauri::command]
fn player_start(state: tauri::State<'_, AudioCtrls>, id: String) {
    state.player.trigger(audio::StreamControlCommand::Play(id));
}

#[tauri::command]
fn player_pause(state: tauri::State<'_, AudioCtrls>, id: String) {
    state
        .player
        .trigger(audio::StreamControlCommand::Pause(Some(id)));
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(audio::setup().unwrap())
        .invoke_handler(tauri::generate_handler![
            record_start,
            record_pause,
            poll_recordings,
            player_start,
            player_pause,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
