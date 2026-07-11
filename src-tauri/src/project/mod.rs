pub mod persistence;

use crate::engine::{ChannelMode, ChannelStripParams};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub last_project: Option<String>,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub sample_rate: u32,
    pub buffer_size: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_project: None,
            input_device: None,
            output_device: None,
            sample_rate: 48000,
            buffer_size: 256,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub name: String,
    pub created_at: String,
    pub modified_at: String,
    pub tracks: Vec<Track>,
    pub next_track_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: u32,
    pub name: String,
    pub wav_filename: Option<String>,
    pub duration_secs: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub channel_strip: ChannelStripParams,
    #[serde(default)]
    pub ch2_strip: ChannelStripParams,
    pub muted: bool,
    pub solo: bool,
    pub volume_db: f32,
    #[serde(default)]
    pub input_device: Option<String>,
    #[serde(default)]
    pub output_device: Option<String>,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: u32,
    #[serde(default = "default_channel_mode")]
    pub channel_mode: ChannelMode,
    #[serde(default)]
    pub merge_to_mono: bool,
}

fn default_buffer_size() -> u32 {
    256
}

fn default_channel_mode() -> ChannelMode {
    ChannelMode::Both
}

impl ProjectManifest {
    pub fn new(name: String) -> Self {
        let now = format_timestamp();
        Self {
            name,
            created_at: now.clone(),
            modified_at: now,
            tracks: Vec::new(),
            next_track_id: 1,
        }
    }
}

fn format_timestamp() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap();
    let secs = duration.as_secs();
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Days since epoch to Y-M-D (simplified calculation)
    let mut y = 1970i64;
    let mut remaining_days = days as i64;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if remaining_days < year_days {
            break;
        }
        remaining_days -= year_days;
        y += 1;
    }
    let month_days: [i64; 12] = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining_days < md {
            m = i;
            break;
        }
        remaining_days -= md;
    }
    let d = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m + 1,
        d,
        hours,
        minutes,
        seconds
    )
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
