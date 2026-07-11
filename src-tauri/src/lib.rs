pub mod engine;
pub mod project;

use engine::{AudioEngineConfig, ChannelStripParams, DeviceInfo, EngineHandle, RecordingResult};
use serde::Deserialize;
use project::{AppSettings, ProjectManifest};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

struct AppState {
    engine: Mutex<Option<EngineHandle>>,
    current_project_dir: Mutex<Option<PathBuf>>,
}

#[tauri::command]
fn list_audio_devices() -> Result<(Vec<DeviceInfo>, Vec<DeviceInfo>), String> {
    EngineHandle::list_devices().map_err(|e| e.to_string())
}

#[tauri::command]
fn start_engine(
    state: State<'_, AppState>,
    config: AudioEngineConfig,
) -> Result<(), String> {
    let mut guard = state.engine.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Err("Engine already running".into());
    }
    let handle = EngineHandle::start(config).map_err(|e| e.to_string())?;
    *guard = Some(handle);
    Ok(())
}

#[tauri::command]
fn start_recording(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<String, String> {
    let recording_path = match path {
        Some(p) => Some(p),
        None => {
            let project_dir = state.current_project_dir.lock().map_err(|e| e.to_string())?;
            project_dir.as_ref().map(|dir| {
                let recordings = project::persistence::recordings_dir(dir);
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let _ = std::fs::create_dir_all(&recordings);
                recordings
                    .join(format!("recording_{}.wav", ts))
                    .to_string_lossy()
                    .to_string()
            })
        }
    };
    let guard = state.engine.lock().map_err(|e| e.to_string())?;
    let handle = guard.as_ref().ok_or("Engine not running")?;
    handle.start_recording(recording_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn stop_recording(
    state: State<'_, AppState>,
) -> Result<Option<RecordingResult>, String> {
    let guard = state.engine.lock().map_err(|e| e.to_string())?;
    let handle = guard.as_ref().ok_or("Engine not running")?;
    handle.stop_recording().map_err(|e| e.to_string())
}

#[tauri::command]
fn update_channel_params(
    state: State<'_, AppState>,
    channel: u8,
    params: ChannelStripParams,
) -> Result<(), String> {
    let guard = state.engine.lock().map_err(|e| e.to_string())?;
    let handle = guard.as_ref().ok_or("Engine not running")?;
    handle.update_channel_params(channel, &params);
    Ok(())
}

#[tauri::command]
fn stop_engine(state: State<'_, AppState>) -> Result<Option<RecordingResult>, String> {
    let mut guard = state.engine.lock().map_err(|e| e.to_string())?;
    let handle = guard.take().ok_or("Engine not running")?;
    handle.stop().map_err(|e| e.to_string())
}

// --- Playback commands ---

#[derive(Deserialize)]
struct PlaybackTrackInput {
    wav_filename: String,
    volume_db: f32,
}

#[tauri::command]
fn start_playback(
    state: State<'_, AppState>,
    tracks: Vec<PlaybackTrackInput>,
) -> Result<(), String> {
    let project_dir = state.current_project_dir.lock().map_err(|e| e.to_string())?;
    let track_infos: Vec<engine::PlaybackTrackInfo> = tracks
        .into_iter()
        .map(|t| {
            let wav_path = if let Some(dir) = project_dir.as_ref() {
                let full = project::persistence::recordings_dir(dir).join(&t.wav_filename);
                full.to_string_lossy().to_string()
            } else {
                t.wav_filename
            };
            engine::PlaybackTrackInfo {
                wav_path,
                volume_db: t.volume_db,
            }
        })
        .collect();
    drop(project_dir);

    let guard = state.engine.lock().map_err(|e| e.to_string())?;
    let handle = guard.as_ref().ok_or("Engine not running")?;
    handle.start_playback(track_infos).map_err(|e| e.to_string())
}

#[tauri::command]
fn stop_playback(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let guard = state.engine.lock().map_err(|e| e.to_string())?;
    let handle = guard.as_ref().ok_or("Engine not running")?;
    handle.stop_playback().map_err(|e| e.to_string())
}

// --- Project commands ---

#[tauri::command]
fn load_app_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    project::persistence::load_settings(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_app_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    project::persistence::save_settings(&app, &settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_projects(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    project::persistence::list_projects(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_project(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<ProjectManifest, String> {
    let (dir, manifest) =
        project::persistence::create_project(&app, &name).map_err(|e| e.to_string())?;
    let mut project_dir = state.current_project_dir.lock().map_err(|e| e.to_string())?;
    *project_dir = Some(dir);
    Ok(manifest)
}

#[tauri::command]
fn open_project(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<ProjectManifest, String> {
    let (dir, manifest) =
        project::persistence::open_project(&app, &name).map_err(|e| e.to_string())?;
    let mut project_dir = state.current_project_dir.lock().map_err(|e| e.to_string())?;
    *project_dir = Some(dir);
    Ok(manifest)
}

#[tauri::command]
fn save_project(
    state: State<'_, AppState>,
    manifest: ProjectManifest,
) -> Result<(), String> {
    let project_dir = state.current_project_dir.lock().map_err(|e| e.to_string())?;
    let dir = project_dir.as_ref().ok_or("No project open")?;
    project::persistence::save_project(dir, &manifest).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_window_opacity(app: tauri::AppHandle, opacity: f64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use tauri::Manager;

        let window = app
            .get_webview_window("main")
            .ok_or("Main window not found")?;
        let handle = window.window_handle().map_err(|e| e.to_string())?;
        if let RawWindowHandle::Win32(h) = handle.as_raw() {
            let hwnd = h.hwnd.get() as isize;
            unsafe {
                extern "system" {
                    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
                    fn SetWindowLongPtrW(hwnd: isize, index: i32, val: isize) -> isize;
                    fn SetLayeredWindowAttributes(
                        hwnd: isize,
                        cr_key: u32,
                        alpha: u8,
                        flags: u32,
                    ) -> i32;
                }
                const GWL_EXSTYLE: i32 = -20;
                const WS_EX_LAYERED: isize = 0x80000;
                const LWA_ALPHA: u32 = 0x2;

                let alpha = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                if alpha < 255 {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED);
                } else {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style & !WS_EX_LAYERED);
                }
                SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
            }
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            engine: Mutex::new(None),
            current_project_dir: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            list_audio_devices,
            start_engine,
            start_recording,
            stop_recording,
            stop_engine,
            update_channel_params,
            start_playback,
            stop_playback,
            load_app_settings,
            save_app_settings,
            list_projects,
            create_project,
            open_project,
            save_project,
            set_window_opacity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
