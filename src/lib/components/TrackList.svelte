<script lang="ts">
  import { app } from '$lib/state.svelte';
  import { defaultChannelStripParams } from '$lib/types';
  import type { Track } from '$lib/types';

  let { onmonitor }: { onmonitor: (trackId: number) => void } = $props();

  function formatDuration(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  }

  let tracks: Track[] = $derived(app.currentProject?.tracks ?? []);

  function selectTrack(track: Track) {
    app.selectedTrackId = app.selectedTrackId === track.id ? null : track.id;
  }

  function createNewTrack() {
    if (!app.currentProject) return;
    const source = app.selectedTrackId != null
      ? app.currentProject.tracks.find(t => t.id === app.selectedTrackId)
      : app.currentProject.tracks[app.currentProject.tracks.length - 1];

    const id = app.currentProject.next_track_id;
    app.currentProject.next_track_id++;
    app.currentProject.tracks = [...app.currentProject.tracks, {
      id,
      name: `Track ${id}`,
      wav_filename: null,
      duration_secs: 0,
      sample_rate: source?.sample_rate ?? 48000,
      channels: 0,
      channel_strip: source ? { ...source.channel_strip } : defaultChannelStripParams(),
      ch2_strip: source ? { ...source.ch2_strip } : defaultChannelStripParams(),
      muted: false,
      solo: false,
      volume_db: 0,
      input_device: source?.input_device ?? app.inputDevices.find(d => d.is_default)?.name ?? null,
      output_device: source?.output_device ?? app.outputDevices.find(d => d.is_default)?.name ?? null,
      buffer_size: source?.buffer_size ?? 256,
      channel_mode: source?.channel_mode ?? 'both',
      merge_to_mono: source?.merge_to_mono ?? false,
    }];
    app.selectedTrackId = id;
  }

  function deleteTrack(e: Event, track: Track) {
    e.stopPropagation();
    if (!app.currentProject) return;
    if (app.monitoringTrackId === track.id || app.recording) return;
    app.currentProject.tracks = app.currentProject.tracks.filter(t => t.id !== track.id);
    if (app.selectedTrackId === track.id) app.selectedTrackId = null;
  }

  function handleMonitor(e: Event, track: Track) {
    e.stopPropagation();
    onmonitor(track.id);
  }

  function toggleMute(e: Event, track: Track) {
    e.stopPropagation();
    if (!app.currentProject) return;
    const idx = app.currentProject.tracks.findIndex(t => t.id === track.id);
    if (idx >= 0) {
      app.currentProject.tracks[idx].muted = !app.currentProject.tracks[idx].muted;
      app.currentProject.tracks = [...app.currentProject.tracks];
    }
  }

  function toggleSolo(e: Event, track: Track) {
    e.stopPropagation();
    if (!app.currentProject) return;
    const idx = app.currentProject.tracks.findIndex(t => t.id === track.id);
    if (idx >= 0) {
      app.currentProject.tracks[idx].solo = !app.currentProject.tracks[idx].solo;
      app.currentProject.tracks = [...app.currentProject.tracks];
    }
  }
</script>

<section class="track-list">
  <div class="track-header">
    <h3>Tracks</h3>
    {#if app.currentProject}
      <button class="new-track-btn" onclick={createNewTrack}>+ New Track</button>
    {/if}
  </div>

  {#if tracks.length === 0}
    <p class="empty">No tracks yet. Add a track to get started.</p>
  {:else}
    {#each tracks as track (track.id)}
      <div
        class="track-row"
        class:selected={app.selectedTrackId === track.id}
        class:muted={track.muted}
        class:empty-track={!track.wav_filename}
        role="button"
        tabindex="0"
        onclick={() => selectTrack(track)}
        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') selectTrack(track); }}
      >
        <div class="track-info">
          <span class="track-name">{track.name}</span>
          <span class="track-meta">
            {#if track.wav_filename}
              {formatDuration(track.duration_secs)} &middot;
              {track.sample_rate / 1000}kHz &middot;
              {track.channels === 1 ? 'Mono' : 'Stereo'}
            {:else}
              Empty — select and record
            {/if}
          </span>
        </div>
        <div class="track-controls">
          <button
            class="track-btn monitor-btn"
            class:active={app.monitoringTrackId === track.id}
            title={app.monitoringTrackId === track.id ? 'Stop Monitor' : 'Monitor'}
            onclick={(e) => handleMonitor(e, track)}
          >🎧</button>
          <button
            class="track-btn"
            class:active={track.muted}
            title="Mute"
            onclick={(e) => toggleMute(e, track)}
          >M</button>
          <button
            class="track-btn"
            class:active={track.solo}
            title="Solo"
            onclick={(e) => toggleSolo(e, track)}
          >S</button>
          <button
            class="track-btn delete-btn"
            title="Delete track"
            disabled={app.monitoringTrackId === track.id || app.recording}
            onclick={(e) => deleteTrack(e, track)}
          >✕</button>
        </div>
      </div>
    {/each}
  {/if}
</section>

<style>
  .track-list {
    margin-bottom: 1.5rem;
  }

  .track-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.75rem;
  }

  .track-header h3 {
    margin: 0;
    font-size: 0.9rem;
    color: #e94560;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .new-track-btn {
    padding: 0.35rem 0.75rem;
    border: 1px solid #333;
    border-radius: 4px;
    background: #1a1a2e;
    color: #aaa;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
  }

  .new-track-btn:hover {
    border-color: #e94560;
    color: #e94560;
  }

  .empty {
    color: #666;
    font-size: 0.85rem;
    font-style: italic;
    margin: 0;
  }

  .track-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0.5rem 0.75rem;
    background: #16213e;
    border: 1px solid #333;
    border-radius: 4px;
    margin-bottom: 0.35rem;
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    color: inherit;
  }

  .track-row:hover {
    border-color: #555;
  }

  .track-row.selected {
    border-color: #e94560;
    background: #1a2a4e;
  }

  .track-row.muted {
    opacity: 0.5;
  }

  .track-row.empty-track {
    border-style: dashed;
  }

  .track-info {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .track-name {
    font-size: 0.9rem;
    color: #e0e0e0;
  }

  .track-meta {
    font-size: 0.75rem;
    color: #888;
  }

  .track-controls {
    display: flex;
    gap: 0.35rem;
  }

  .track-btn {
    width: 28px;
    height: 28px;
    border: 1px solid #333;
    border-radius: 3px;
    background: #1a1a2e;
    color: #888;
    font-size: 0.7rem;
    font-weight: 700;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .track-btn:hover:not(:disabled) {
    border-color: #555;
    color: #e0e0e0;
  }

  .track-btn:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .track-btn.active {
    background: #e94560;
    border-color: #e94560;
    color: #fff;
  }

  .monitor-btn {
    font-size: 0.85rem;
  }

  .monitor-btn.active {
    background: #f0a030;
    border-color: #f0a030;
  }

  .delete-btn:hover:not(:disabled) {
    border-color: #e94560;
    color: #e94560;
  }
</style>
