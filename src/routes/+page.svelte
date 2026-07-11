<script lang="ts">
  import { onMount } from 'svelte';
  import * as api from '$lib/api';
  import { app, applyProject, buildProjectManifest, getPlayableTracks } from '$lib/state.svelte';
  import ChannelStrip from '$lib/components/ChannelStrip.svelte';
  import ProjectBar from '$lib/components/ProjectBar.svelte';
  import TrackList from '$lib/components/TrackList.svelte';

  const STANDARD_RATES = [44100, 48000, 88200, 96000, 176400, 192000];

  let transparent = $state(false);

  async function toggleTransparency() {
    transparent = !transparent;
    await api.setWindowOpacity(transparent ? 0.75 : 1.0);
  }

  let selectedTrackIndex = $derived(
    app.currentProject?.tracks.findIndex(t => t.id === app.selectedTrackId) ?? -1
  );

  let selectedTrack = $derived(
    selectedTrackIndex >= 0 && app.currentProject
      ? app.currentProject.tracks[selectedTrackIndex]
      : null
  );

  let availableSampleRates: number[] = $derived.by(() => {
    if (!selectedTrack) return STANDARD_RATES;
    const inputDev = app.inputDevices.find(d => d.name === selectedTrack.input_device);
    const outputDev = app.outputDevices.find(d => d.name === selectedTrack.output_device);
    if (!inputDev || !outputDev) return STANDARD_RATES;

    return STANDARD_RATES.filter(rate =>
      inputDev.configs.some(c => rate >= c.min_sample_rate && rate <= c.max_sample_rate) &&
      outputDev.configs.some(c => rate >= c.min_sample_rate && rate <= c.max_sample_rate)
    );
  });

  let hasPlayableTracks = $derived(getPlayableTracks().length > 0);
  let canRecord = $derived(app.selectedTrackId != null && !app.recording && !app.isPlaying);
  let canPlay = $derived(hasPlayableTracks && !app.recording && !app.isPlaying);
  let canStop = $derived(app.recording || app.isPlaying);

  onMount(async () => {
    try {
      const [inputs, outputs] = await api.listAudioDevices();
      app.inputDevices = inputs;
      app.outputDevices = outputs;

      const settings = await api.loadAppSettings();
      app.projectList = await api.listProjects();

      if (settings.last_project) {
        try {
          const manifest = await api.openProject(settings.last_project);
          applyProject(manifest);
        } catch {
          // Project may have been deleted
        }
      }
    } catch (e) {
      app.error = `Failed to initialize: ${e}`;
    }
  });

  let paramTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    if (app.engineRunning && app.monitoringTrackId != null && app.currentProject) {
      const track = app.currentProject.tracks.find(t => t.id === app.monitoringTrackId);
      if (track) {
        const ch1 = { ...track.channel_strip };
        const ch2 = { ...track.ch2_strip };
        const mode = track.channel_mode;
        if (paramTimer) clearTimeout(paramTimer);
        paramTimer = setTimeout(() => {
          if (mode === 'ch1') {
            api.updateChannelParams(0, ch1).catch(() => {});
          } else if (mode === 'ch2') {
            api.updateChannelParams(1, ch2).catch(() => {});
          } else {
            api.updateChannelParams(0, ch1).catch(() => {});
            api.updateChannelParams(1, ch2).catch(() => {});
          }
        }, 16);
      }
    }
  });

  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  function debouncedProjectSave() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      const manifest = buildProjectManifest();
      if (manifest) {
        try {
          await api.saveProject(manifest);
        } catch {
          // Silent save failure
        }
      }
    }, 500);
  }

  $effect(() => {
    if (app.currentProject) {
      void JSON.stringify(app.currentProject.tracks);
      debouncedProjectSave();
    }
  });

  function getTrackById(id: number) {
    return app.currentProject?.tracks.find(t => t.id === id) ?? null;
  }

  async function startEngineForTrack(trackId: number) {
    const track = getTrackById(trackId);
    if (!track) return;
    await api.startEngine({
      input_device: track.input_device,
      output_device: track.output_device,
      sample_rate: track.sample_rate,
      buffer_size: track.buffer_size,
      channel_mode: track.channel_mode,
      merge_to_mono: track.channel_mode === 'both' && track.merge_to_mono,
    });
    app.engineRunning = true;
    app.monitoringTrackId = trackId;

    if (track.channel_mode === 'ch1') {
      api.updateChannelParams(0, { ...track.channel_strip }).catch(() => {});
    } else if (track.channel_mode === 'ch2') {
      api.updateChannelParams(1, { ...track.ch2_strip }).catch(() => {});
    } else {
      api.updateChannelParams(0, { ...track.channel_strip }).catch(() => {});
      api.updateChannelParams(1, { ...track.ch2_strip }).catch(() => {});
    }

    await api.saveAppSettings({
      last_project: app.currentProject?.name ?? null,
      input_device: track.input_device,
      output_device: track.output_device,
      sample_rate: track.sample_rate,
      buffer_size: track.buffer_size,
    });
  }

  async function stopCurrentEngine() {
    if (app.recording) {
      await api.stopRecording();
      app.recording = false;
    }
    if (app.isPlaying) {
      await api.stopPlayback();
      app.isPlaying = false;
    }
    await api.stopEngine();
    app.engineRunning = false;
    app.monitoringTrackId = null;
  }

  async function toggleMonitor(trackId: number) {
    try {
      app.error = '';
      if (app.monitoringTrackId === trackId) {
        await stopCurrentEngine();
        app.status = 'Stopped';
      } else {
        if (app.engineRunning) {
          await stopCurrentEngine();
        }
        await startEngineForTrack(trackId);
        app.selectedTrackId = trackId;
        app.status = 'Monitoring';
      }
    } catch (e) {
      app.error = `${e}`;
    }
  }

  async function startRecording() {
    if (!canRecord || app.selectedTrackId == null) return;
    try {
      app.error = '';
      if (!app.engineRunning) {
        await startEngineForTrack(app.selectedTrackId);
      } else if (app.monitoringTrackId !== app.selectedTrackId) {
        await stopCurrentEngine();
        await startEngineForTrack(app.selectedTrackId);
      }
      const playable = getPlayableTracks(app.selectedTrackId);
      if (playable.length > 0) {
        await api.startPlayback(playable);
        app.isPlaying = true;
      }
      const path = await api.startRecording();
      app.recording = true;
      app.status = `Recording`;
    } catch (e) {
      app.error = `${e}`;
    }
  }

  async function startPlayback() {
    if (!canPlay) return;
    try {
      app.error = '';
      if (!app.engineRunning && app.selectedTrackId != null) {
        await startEngineForTrack(app.selectedTrackId);
      }
      const playable = getPlayableTracks();
      await api.startPlayback(playable);
      app.isPlaying = true;
      app.status = 'Playing';
    } catch (e) {
      app.error = `${e}`;
    }
  }

  async function handleStop() {
    try {
      app.error = '';
      if (app.isPlaying) {
        await api.stopPlayback();
        app.isPlaying = false;
      }
      if (app.recording) {
        const result = await api.stopRecording();
        app.recording = false;
        app.lastResult = result ?? null;
        if (result && app.currentProject && app.selectedTrackId != null) {
          const idx = app.currentProject.tracks.findIndex(t => t.id === app.selectedTrackId);
          if (idx >= 0) {
            const filename = result.path.split(/[/\\]/).pop() ?? result.path;
            app.currentProject.tracks[idx].wav_filename = filename;
            app.currentProject.tracks[idx].duration_secs = result.duration_secs;
            app.currentProject.tracks[idx].sample_rate = result.sample_rate;
            app.currentProject.tracks[idx].channels = result.channels;
            app.currentProject.tracks = [...app.currentProject.tracks];
          }
          app.status = `Recorded (${result.duration_secs.toFixed(1)}s)`;
        } else {
          app.status = app.engineRunning ? 'Monitoring' : 'Stopped';
        }
      } else {
        app.status = app.engineRunning ? 'Monitoring' : 'Stopped';
      }
    } catch (e) {
      app.error = `${e}`;
    }
  }
</script>

<main>
  <div class="header">
    <div>
      <h1>ITEAD</h1>
      <p class="subtitle">Audio Engine</p>
    </div>
    <button
      class="btn transparency-btn"
      class:active={transparent}
      onclick={toggleTransparency}
      title={transparent ? 'Disable transparency' : 'Enable transparency'}
    >
      {transparent ? '◉' : '◎'}
    </button>
  </div>

  <ProjectBar />

  <section class="transport">
    <button
      class="btn record"
      class:active={app.recording}
      disabled={!canRecord && !app.recording}
      onclick={app.recording ? handleStop : startRecording}
      title={app.recording ? 'Stop Recording' : (app.selectedTrackId == null ? 'Select a track first' : 'Record')}
    >●</button>
    <button
      class="btn play"
      class:active={app.isPlaying && !app.recording}
      disabled={!canPlay && !app.isPlaying}
      onclick={app.isPlaying && !app.recording ? handleStop : startPlayback}
    >▶</button>
    <button
      class="btn stop-btn"
      disabled={!canStop}
      onclick={handleStop}
    >■</button>
    <span class="status-text">{app.status}</span>
    {#if app.error}
      <span class="error-text">{app.error}</span>
    {/if}
  </section>

  <TrackList onmonitor={toggleMonitor} />

  {#if selectedTrack && app.currentProject}
    <section class="selected-track-strip">
      <h3>{selectedTrack.name}</h3>

      <div class="track-config">
        <div class="field">
          <label for="track-input">Input</label>
          <select id="track-input" bind:value={selectedTrack.input_device} disabled={app.monitoringTrackId === selectedTrack.id}>
            <option value={null}>None</option>
            {#each app.inputDevices as device}
              <option value={device.name}>
                {device.name}{device.is_default ? ' (default)' : ''}
              </option>
            {/each}
          </select>
        </div>

        <div class="field">
          <label for="track-output">Output</label>
          <select id="track-output" bind:value={selectedTrack.output_device} disabled={app.monitoringTrackId === selectedTrack.id}>
            <option value={null}>None</option>
            {#each app.outputDevices as device}
              <option value={device.name}>
                {device.name}{device.is_default ? ' (default)' : ''}
              </option>
            {/each}
          </select>
        </div>

        <div class="field">
          <label for="track-rate">Sample Rate</label>
          <select id="track-rate" bind:value={selectedTrack.sample_rate} disabled={app.monitoringTrackId === selectedTrack.id}>
            {#each availableSampleRates as rate}
              <option value={rate}>{rate} Hz</option>
            {/each}
          </select>
        </div>

        <div class="field">
          <label for="track-buffer">Buffer</label>
          <select id="track-buffer" bind:value={selectedTrack.buffer_size} disabled={app.monitoringTrackId === selectedTrack.id}>
            <option value={64}>64</option>
            <option value={128}>128</option>
            <option value={256}>256</option>
            <option value={512}>512</option>
            <option value={1024}>1024</option>
          </select>
        </div>

        <div class="field">
          <label>Channels</label>
          <div class="channel-toggle">
            <button
              class="ch-btn" class:active={selectedTrack.channel_mode === 'ch1'}
              disabled={app.monitoringTrackId === selectedTrack.id}
              onclick={() => { selectedTrack.channel_mode = 'ch1'; }}
            >Ch 1</button>
            <button
              class="ch-btn" class:active={selectedTrack.channel_mode === 'both'}
              disabled={app.monitoringTrackId === selectedTrack.id}
              onclick={() => { selectedTrack.channel_mode = 'both'; }}
            >Both</button>
            <button
              class="ch-btn" class:active={selectedTrack.channel_mode === 'ch2'}
              disabled={app.monitoringTrackId === selectedTrack.id}
              onclick={() => { selectedTrack.channel_mode = 'ch2'; }}
            >Ch 2</button>
          </div>
        </div>

        {#if selectedTrack.channel_mode === 'both'}
          <div class="field checkbox">
            <label>
              <input type="checkbox" bind:checked={selectedTrack.merge_to_mono} disabled={app.monitoringTrackId === selectedTrack.id} />
              Merge to mono
            </label>
          </div>
        {/if}
      </div>

      <div class="track-volume">
        <label for="track-volume">Volume</label>
        <input
          id="track-volume"
          type="range"
          min="-60"
          max="12"
          step="0.1"
          bind:value={selectedTrack.volume_db}
        />
        <span class="volume-label">{selectedTrack.volume_db.toFixed(1)} dB</span>
      </div>

      <div class="channel-strips">
        {#if selectedTrack.channel_mode === 'both'}
          <ChannelStrip label="Ch 1" bind:params={selectedTrack.channel_strip} />
          <ChannelStrip label="Ch 2" bind:params={selectedTrack.ch2_strip} />
        {:else if selectedTrack.channel_mode === 'ch1'}
          <ChannelStrip label="Ch 1" bind:params={selectedTrack.channel_strip} />
        {:else}
          <ChannelStrip label="Ch 2" bind:params={selectedTrack.ch2_strip} />
        {/if}
      </div>
    </section>
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    background: #1a1a2e;
    color: #e0e0e0;
    font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
  }

  main {
    max-width: 640px;
    margin: 0 auto;
    padding: 2rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 2rem;
  }

  h1 {
    margin: 0;
    font-size: 2rem;
    color: #e94560;
    letter-spacing: 0.1em;
  }

  .subtitle {
    margin: 0.25rem 0 0;
    color: #888;
    font-size: 0.85rem;
  }

  .transparency-btn {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: #16213e;
    color: #888;
    font-size: 1.2rem;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 1px solid #333;
  }
  .transparency-btn:hover {
    background: #1a2744;
    color: #e0e0e0;
  }
  .transparency-btn.active {
    background: #e94560;
    color: #fff;
    border-color: #e94560;
  }

  section {
    margin-bottom: 1.5rem;
  }

  .transport {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 1.5rem;
  }

  .status-text {
    margin-left: 0.5rem;
    font-size: 0.85rem;
    color: #888;
  }

  .error-text {
    margin-left: 0.5rem;
    font-size: 0.85rem;
    color: #e94560;
  }

  .btn {
    border: none;
    border-radius: 4px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn.record {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: #444;
    color: #e94560;
    font-size: 1.2rem;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  .btn.record:hover:not(:disabled) {
    background: #555;
  }
  .btn.record.active {
    background: #e94560;
    color: #fff;
    animation: pulse 1.5s ease-in-out infinite;
  }
  .btn.record:disabled {
    opacity: 0.3;
    cursor: default;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.6; }
  }

  .btn.play {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: #444;
    color: #4caf50;
    font-size: 1.1rem;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 0 0 2px;
  }
  .btn.play:hover:not(:disabled) {
    background: #555;
  }
  .btn.play.active {
    background: #4caf50;
    color: #fff;
  }
  .btn.play:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .btn.stop-btn {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: #444;
    color: #e0e0e0;
    font-size: 1rem;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  .btn.stop-btn:hover:not(:disabled) {
    background: #555;
  }
  .btn.stop-btn:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .track-config {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }

  .field {
    flex: 1;
    min-width: 140px;
  }

  label {
    display: block;
    margin-bottom: 0.35rem;
    font-size: 0.8rem;
    color: #aaa;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  select {
    width: 100%;
    padding: 0.5rem;
    background: #16213e;
    color: #e0e0e0;
    border: 1px solid #333;
    border-radius: 4px;
    font-size: 0.9rem;
    box-sizing: border-box;
  }

  select:disabled {
    opacity: 0.5;
  }

  .channel-toggle {
    display: flex;
    gap: 0;
    border-radius: 4px;
    overflow: hidden;
    border: 1px solid #333;
  }

  .ch-btn {
    flex: 1;
    padding: 0.5rem 0;
    background: #16213e;
    color: #888;
    border: none;
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .ch-btn:not(:last-child) {
    border-right: 1px solid #333;
  }

  .ch-btn.active {
    background: #e94560;
    color: #fff;
  }

  .ch-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .checkbox label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.9rem;
    color: #e0e0e0;
    text-transform: none;
    cursor: pointer;
    padding-top: 0.5rem;
  }

  input[type="checkbox"] {
    accent-color: #e94560;
    width: 16px;
    height: 16px;
  }

  .selected-track-strip {
    border: 1px solid #e94560;
    border-radius: 4px;
    padding: 0.75rem;
  }

  .selected-track-strip h3 {
    margin: 0 0 0.75rem;
    font-size: 0.9rem;
    color: #e94560;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .track-volume {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }

  .track-volume label {
    margin: 0;
    min-width: 50px;
  }

  .track-volume input[type="range"] {
    flex: 1;
    accent-color: #e94560;
  }

  .volume-label {
    font-size: 0.8rem;
    color: #aaa;
    min-width: 60px;
    text-align: right;
  }

  .channel-strips {
    display: flex;
    gap: 1rem;
  }
</style>
