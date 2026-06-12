pub mod engine;

use engine::{AudioEngineConfig, DeviceInfo, EngineHandle, RecordingResult};
use std::sync::Mutex;
use tauri::State;

struct EngineState(Mutex<Option<EngineHandle>>);

#[tauri::command]
fn list_audio_devices() -> Result<(Vec<DeviceInfo>, Vec<DeviceInfo>), String> {
    EngineHandle::list_devices().map_err(|e| e.to_string())
}

#[tauri::command]
fn start_engine(
    state: State<'_, EngineState>,
    config: AudioEngineConfig,
) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Err("Engine already running".into());
    }
    let handle = EngineHandle::start(config).map_err(|e| e.to_string())?;
    *guard = Some(handle);
    Ok(())
}

#[tauri::command]
fn start_recording(
    state: State<'_, EngineState>,
    path: Option<String>,
) -> Result<String, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let handle = guard.as_ref().ok_or("Engine not running")?;
    handle.start_recording(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn stop_engine(state: State<'_, EngineState>) -> Result<Option<RecordingResult>, String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let handle = guard.take().ok_or("Engine not running")?;
    handle.stop().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(EngineState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            list_audio_devices,
            start_engine,
            start_recording,
            stop_engine,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
