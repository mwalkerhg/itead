import type {
  DeviceInfo,
  PlaybackTrackInput,
  RecordingResult,
  ProjectManifest,
} from './types';

export const app = $state({
  inputDevices: [] as DeviceInfo[],
  outputDevices: [] as DeviceInfo[],

  engineRunning: false,
  monitoringTrackId: null as number | null,
  recording: false,
  status: 'Stopped',
  error: '',
  lastResult: null as RecordingResult | null,

  currentProject: null as ProjectManifest | null,
  projectList: [] as string[],
  isPlaying: false,
  selectedTrackId: null as number | null,
});

export function applyProject(manifest: ProjectManifest) {
  app.currentProject = manifest;
  app.selectedTrackId = null;
  app.monitoringTrackId = null;
}

export function buildProjectManifest(): ProjectManifest | null {
  return app.currentProject;
}

export function getPlayableTracks(excludeId?: number | null): PlaybackTrackInput[] {
  const tracks = app.currentProject?.tracks ?? [];
  const hasSolo = tracks.some(t => t.solo);
  return tracks
    .filter(t => t.wav_filename && t.id !== excludeId && (hasSolo ? t.solo : !t.muted))
    .map(t => ({ wav_filename: t.wav_filename!, volume_db: t.volume_db }));
}
