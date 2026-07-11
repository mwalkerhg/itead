import { invoke } from '@tauri-apps/api/core';
import type {
  AppSettings,
  ChannelStripParams,
  DeviceInfo,
  PlaybackTrackInput,
  ProjectManifest,
  RecordingResult,
} from './types';

export async function listAudioDevices(): Promise<[DeviceInfo[], DeviceInfo[]]> {
  return invoke<[DeviceInfo[], DeviceInfo[]]>('list_audio_devices');
}

export async function startEngine(config: {
  input_device: string | null;
  output_device: string | null;
  sample_rate: number;
  buffer_size: number;
  channel_mode: string;
  merge_to_mono: boolean;
}): Promise<void> {
  return invoke('start_engine', { config });
}

export async function startRecording(path?: string): Promise<string> {
  return invoke<string>('start_recording', { path: path ?? null });
}

export async function stopRecording(): Promise<RecordingResult | null> {
  return invoke<RecordingResult | null>('stop_recording');
}

export async function stopEngine(): Promise<RecordingResult | null> {
  return invoke<RecordingResult | null>('stop_engine');
}

export async function updateChannelParams(
  channel: number,
  params: ChannelStripParams
): Promise<void> {
  return invoke('update_channel_params', { channel, params });
}

export async function startPlayback(tracks: PlaybackTrackInput[]): Promise<void> {
  return invoke('start_playback', { tracks });
}

export async function stopPlayback(): Promise<void> {
  return invoke('stop_playback');
}

export async function loadAppSettings(): Promise<AppSettings> {
  return invoke<AppSettings>('load_app_settings');
}

export async function saveAppSettings(settings: AppSettings): Promise<void> {
  return invoke('save_app_settings', { settings });
}

export async function listProjects(): Promise<string[]> {
  return invoke<string[]>('list_projects');
}

export async function createProject(name: string): Promise<ProjectManifest> {
  return invoke<ProjectManifest>('create_project', { name });
}

export async function openProject(name: string): Promise<ProjectManifest> {
  return invoke<ProjectManifest>('open_project', { name });
}

export async function saveProject(manifest: ProjectManifest): Promise<void> {
  return invoke('save_project', { manifest });
}

export async function setWindowOpacity(opacity: number): Promise<void> {
  return invoke('set_window_opacity', { opacity });
}
