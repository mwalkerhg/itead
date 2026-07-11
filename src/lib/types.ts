export interface DeviceConfigInfo {
  channels: number;
  min_sample_rate: number;
  max_sample_rate: number;
  sample_format: string;
}

export interface DeviceInfo {
  name: string;
  is_default: boolean;
  configs: DeviceConfigInfo[];
}

export interface RecordingResult {
  path: string;
  duration_secs: number;
  total_samples: number;
  sample_rate: number;
  channels: number;
}

export type TonePreset = 'off' | 'vox_ac30';

export interface ChannelStripParams {
  gain_db: number;
  lowcut_enabled: boolean;
  lowcut_freq_hz: number;
  phase_invert: boolean;
  reverb_enabled: boolean;
  reverb_room_size: number;
  reverb_damping: number;
  reverb_wet: number;
  tone_preset: TonePreset;
  tone_drive: number;
}

export interface AppSettings {
  last_project: string | null;
  input_device: string | null;
  output_device: string | null;
  sample_rate: number;
  buffer_size: number;
}

export interface Track {
  id: number;
  name: string;
  wav_filename: string | null;
  duration_secs: number;
  sample_rate: number;
  channels: number;
  channel_strip: ChannelStripParams;
  ch2_strip: ChannelStripParams;
  muted: boolean;
  solo: boolean;
  volume_db: number;
  input_device: string | null;
  output_device: string | null;
  buffer_size: number;
  channel_mode: string;
  merge_to_mono: boolean;
}

export interface ProjectManifest {
  name: string;
  created_at: string;
  modified_at: string;
  tracks: Track[];
  next_track_id: number;
}

export interface PlaybackTrackInput {
  wav_filename: string;
  volume_db: number;
}

export function defaultChannelStripParams(): ChannelStripParams {
  return {
    gain_db: 0,
    lowcut_enabled: false,
    lowcut_freq_hz: 80,
    phase_invert: false,
    reverb_enabled: false,
    reverb_room_size: 0.5,
    reverb_damping: 0.5,
    reverb_wet: 0.3,
    tone_preset: 'off',
    tone_drive: 0.5,
  };
}
