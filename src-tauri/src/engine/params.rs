use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TonePreset {
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "vox_ac30")]
    VoxAc30,
}

impl Default for TonePreset {
    fn default() -> Self {
        TonePreset::Off
    }
}

impl TonePreset {
    pub fn to_u32(self) -> u32 {
        match self {
            TonePreset::Off => 0,
            TonePreset::VoxAc30 => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelMode {
    Ch1,
    Ch2,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioEngineConfig {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub channel_mode: ChannelMode,
    pub merge_to_mono: bool,
}

impl Default for AudioEngineConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            sample_rate: 48000,
            buffer_size: 256,
            channel_mode: ChannelMode::Both,
            merge_to_mono: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStripParams {
    pub gain_db: f32,
    pub lowcut_enabled: bool,
    pub lowcut_freq_hz: f32,
    pub phase_invert: bool,
    pub reverb_enabled: bool,
    pub reverb_room_size: f32,
    pub reverb_damping: f32,
    pub reverb_wet: f32,
    #[serde(default)]
    pub tone_preset: TonePreset,
    #[serde(default = "default_tone_drive")]
    pub tone_drive: f32,
}

fn default_tone_drive() -> f32 {
    0.5
}

impl Default for ChannelStripParams {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            lowcut_enabled: false,
            lowcut_freq_hz: 80.0,
            phase_invert: false,
            reverb_enabled: false,
            reverb_room_size: 0.5,
            reverb_damping: 0.5,
            reverb_wet: 0.3,
            tone_preset: TonePreset::Off,
            tone_drive: 0.5,
        }
    }
}

pub(crate) struct AtomicChannelStrip {
    pub gain_db: AtomicU32,
    pub lowcut_enabled: AtomicBool,
    pub lowcut_freq_hz: AtomicU32,
    pub phase_invert: AtomicBool,
    pub reverb_enabled: AtomicBool,
    pub reverb_room_size: AtomicU32,
    pub reverb_damping: AtomicU32,
    pub reverb_wet: AtomicU32,
    pub tone_preset: AtomicU32,
    pub tone_drive: AtomicU32,
}

impl AtomicChannelStrip {
    pub fn new() -> Self {
        Self {
            gain_db: AtomicU32::new(0.0f32.to_bits()),
            lowcut_enabled: AtomicBool::new(false),
            lowcut_freq_hz: AtomicU32::new(80.0f32.to_bits()),
            phase_invert: AtomicBool::new(false),
            reverb_enabled: AtomicBool::new(false),
            reverb_room_size: AtomicU32::new(0.5f32.to_bits()),
            reverb_damping: AtomicU32::new(0.5f32.to_bits()),
            reverb_wet: AtomicU32::new(0.3f32.to_bits()),
            tone_preset: AtomicU32::new(0),
            tone_drive: AtomicU32::new(0.5f32.to_bits()),
        }
    }

    pub fn load_gain_db(&self) -> f32 {
        f32::from_bits(self.gain_db.load(Ordering::Relaxed))
    }

    pub fn load_lowcut_freq(&self) -> f32 {
        f32::from_bits(self.lowcut_freq_hz.load(Ordering::Relaxed))
    }
}

pub(crate) struct SharedChannelParams {
    pub strips: [AtomicChannelStrip; 2],
}

impl SharedChannelParams {
    pub fn new() -> Self {
        Self {
            strips: [AtomicChannelStrip::new(), AtomicChannelStrip::new()],
        }
    }
}
