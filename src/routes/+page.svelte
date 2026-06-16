<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';

  interface DeviceConfigInfo {
    channels: number;
    min_sample_rate: number;
    max_sample_rate: number;
    sample_format: string;
  }

  interface DeviceInfo {
    name: string;
    is_default: boolean;
    configs: DeviceConfigInfo[];
  }

  interface RecordingResult {
    path: string;
    duration_secs: number;
    total_samples: number;
  }

  interface ChannelStripParams {
    gain_db: number;
    lowcut_enabled: boolean;
    lowcut_freq_hz: number;
    phase_invert: boolean;
  }

  const STANDARD_RATES = [44100, 48000, 88200, 96000, 176400, 192000];

  let inputDevices: DeviceInfo[] = $state([]);
  let outputDevices: DeviceInfo[] = $state([]);
  let selectedInput: string = $state('');
  let selectedOutput: string = $state('');
  let sampleRate: number = $state(48000);
  let bufferSize: number = $state(256);
  let channelMode: string = $state('both');
  let mergeToMono: boolean = $state(false);

  let ch1Params: ChannelStripParams = $state({
    gain_db: 0, lowcut_enabled: false, lowcut_freq_hz: 80, phase_invert: false
  });
  let ch2Params: ChannelStripParams = $state({
    gain_db: 0, lowcut_enabled: false, lowcut_freq_hz: 80, phase_invert: false
  });

  let engineRunning: boolean = $state(false);
  let recording: boolean = $state(false);
  let status: string = $state('Stopped');
  let error: string = $state('');
  let lastResult: RecordingResult | null = $state(null);

  let availableSampleRates: number[] = $derived.by(() => {
    const inputDev = inputDevices.find(d => d.name === selectedInput);
    const outputDev = outputDevices.find(d => d.name === selectedOutput);
    if (!inputDev || !outputDev) return STANDARD_RATES;

    return STANDARD_RATES.filter(rate =>
      inputDev.configs.some(c => rate >= c.min_sample_rate && rate <= c.max_sample_rate) &&
      outputDev.configs.some(c => rate >= c.min_sample_rate && rate <= c.max_sample_rate)
    );
  });

  $effect(() => {
    if (availableSampleRates.length > 0 && !availableSampleRates.includes(sampleRate)) {
      sampleRate = availableSampleRates.includes(48000) ? 48000 : availableSampleRates[0];
    }
  });

  onMount(async () => {
    try {
      const [inputs, outputs] = await invoke<[DeviceInfo[], DeviceInfo[]]>('list_audio_devices');
      inputDevices = inputs;
      outputDevices = outputs;
      selectedInput = inputs.find(d => d.is_default)?.name ?? inputs[0]?.name ?? '';
      selectedOutput = outputs.find(d => d.is_default)?.name ?? outputs[0]?.name ?? '';
    } catch (e) {
      error = `Failed to list devices: ${e}`;
    }
  });

  let ch1Timer: ReturnType<typeof setTimeout> | null = null;
  let ch2Timer: ReturnType<typeof setTimeout> | null = null;

  function sendChannelParams(channel: number, params: ChannelStripParams) {
    invoke('update_channel_params', { channel, params }).catch(e => {
      console.error(`Failed to update ch${channel + 1} params:`, e);
    });
  }

  $effect(() => {
    if (engineRunning) {
      const p = { ...ch1Params };
      if (ch1Timer) clearTimeout(ch1Timer);
      ch1Timer = setTimeout(() => sendChannelParams(0, p), 16);
    }
  });

  $effect(() => {
    if (engineRunning) {
      const p = { ...ch2Params };
      if (ch2Timer) clearTimeout(ch2Timer);
      ch2Timer = setTimeout(() => sendChannelParams(1, p), 16);
    }
  });

  async function startEngine() {
    try {
      error = '';
      await invoke('start_engine', {
        config: {
          input_device: selectedInput || null,
          output_device: selectedOutput || null,
          sample_rate: sampleRate,
          buffer_size: bufferSize,
          channel_mode: channelMode,
          merge_to_mono: channelMode === 'both' && mergeToMono
        }
      });
      engineRunning = true;
      status = 'Passthrough active';
      sendChannelParams(0, { ...ch1Params });
      sendChannelParams(1, { ...ch2Params });
    } catch (e) {
      error = `${e}`;
    }
  }

  async function startRecording() {
    try {
      error = '';
      const path = await invoke<string>('start_recording', { path: null });
      recording = true;
      status = `Recording to ${path}`;
    } catch (e) {
      error = `${e}`;
    }
  }

  async function stopEngine() {
    try {
      error = '';
      const result = await invoke<RecordingResult | null>('stop_engine');
      engineRunning = false;
      recording = false;
      lastResult = result ?? null;
      status = result
        ? `Saved ${result.path} (${result.duration_secs.toFixed(1)}s)`
        : 'Stopped';
    } catch (e) {
      error = `${e}`;
    }
  }
</script>

<main>
  <h1>ITEAD</h1>
  <p class="subtitle">Audio Engine</p>

  <section class="devices">
    <div class="field">
      <label for="input-device">Input Device</label>
      <select id="input-device" bind:value={selectedInput} disabled={engineRunning}>
        {#each inputDevices as device}
          <option value={device.name}>
            {device.name}{device.is_default ? ' (default)' : ''}
          </option>
        {/each}
      </select>
    </div>

    <div class="field">
      <label for="output-device">Output Device</label>
      <select id="output-device" bind:value={selectedOutput} disabled={engineRunning}>
        {#each outputDevices as device}
          <option value={device.name}>
            {device.name}{device.is_default ? ' (default)' : ''}
          </option>
        {/each}
      </select>
    </div>
  </section>

  <section class="config">
    <div class="field">
      <label for="sample-rate">Sample Rate</label>
      <select id="sample-rate" bind:value={sampleRate} disabled={engineRunning}>
        {#each availableSampleRates as rate}
          <option value={rate}>{rate} Hz</option>
        {/each}
      </select>
    </div>

    <div class="field">
      <label for="buffer-size">Buffer Size</label>
      <select id="buffer-size" bind:value={bufferSize} disabled={engineRunning}>
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
          class="ch-btn" class:active={channelMode === 'ch1'}
          disabled={engineRunning}
          onclick={() => channelMode = 'ch1'}
        >Ch 1</button>
        <button
          class="ch-btn" class:active={channelMode === 'both'}
          disabled={engineRunning}
          onclick={() => channelMode = 'both'}
        >Both</button>
        <button
          class="ch-btn" class:active={channelMode === 'ch2'}
          disabled={engineRunning}
          onclick={() => channelMode = 'ch2'}
        >Ch 2</button>
      </div>
    </div>

    {#if channelMode === 'both'}
      <div class="field checkbox">
        <label>
          <input type="checkbox" bind:checked={mergeToMono} disabled={engineRunning} />
          Merge to mono (both channels in both ears)
        </label>
      </div>
    {/if}
  </section>

  <section class="channel-strips">
    {#if channelMode !== 'ch2'}
      <div class="strip">
        <h3>Ch 1</h3>
        <div class="strip-control">
          <label>Gain: {ch1Params.gain_db.toFixed(1)} dB</label>
          <input type="range" min={-60} max={12} step={0.1} bind:value={ch1Params.gain_db} />
        </div>
        <div class="strip-control">
          <label class="toggle-label">
            <input type="checkbox" bind:checked={ch1Params.lowcut_enabled} />
            Low Cut
          </label>
          {#if ch1Params.lowcut_enabled}
            <label>{ch1Params.lowcut_freq_hz.toFixed(0)} Hz</label>
            <input type="range" min={20} max={500} step={1} bind:value={ch1Params.lowcut_freq_hz} />
          {/if}
        </div>
        <div class="strip-control">
          <label class="toggle-label">
            <input type="checkbox" bind:checked={ch1Params.phase_invert} />
            &#x2300; Phase Invert
          </label>
        </div>
      </div>
    {/if}

    {#if channelMode !== 'ch1'}
      <div class="strip">
        <h3>Ch 2</h3>
        <div class="strip-control">
          <label>Gain: {ch2Params.gain_db.toFixed(1)} dB</label>
          <input type="range" min={-60} max={12} step={0.1} bind:value={ch2Params.gain_db} />
        </div>
        <div class="strip-control">
          <label class="toggle-label">
            <input type="checkbox" bind:checked={ch2Params.lowcut_enabled} />
            Low Cut
          </label>
          {#if ch2Params.lowcut_enabled}
            <label>{ch2Params.lowcut_freq_hz.toFixed(0)} Hz</label>
            <input type="range" min={20} max={500} step={1} bind:value={ch2Params.lowcut_freq_hz} />
          {/if}
        </div>
        <div class="strip-control">
          <label class="toggle-label">
            <input type="checkbox" bind:checked={ch2Params.phase_invert} />
            &#x2300; Phase Invert
          </label>
        </div>
      </div>
    {/if}
  </section>

  <section class="transport">
    {#if !engineRunning}
      <button class="btn start" onclick={startEngine}>Start Engine</button>
    {:else}
      {#if !recording}
        <button class="btn record" onclick={startRecording}>Record</button>
      {/if}
      <button class="btn stop" onclick={stopEngine}>Stop</button>
    {/if}
  </section>

  <section class="status">
    <p class="status-text">{status}</p>
    {#if error}
      <p class="error">{error}</p>
    {/if}
    {#if lastResult}
      <p class="result">
        {lastResult.path} — {lastResult.duration_secs.toFixed(1)}s,
        {lastResult.total_samples.toLocaleString()} samples
      </p>
    {/if}
  </section>
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

  h1 {
    margin: 0;
    font-size: 2rem;
    color: #e94560;
    letter-spacing: 0.1em;
  }

  .subtitle {
    margin: 0.25rem 0 2rem;
    color: #888;
    font-size: 0.85rem;
  }

  section {
    margin-bottom: 1.5rem;
  }

  .devices, .config {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }

  .field {
    flex: 1;
    min-width: 200px;
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

  .channel-strips {
    display: flex;
    gap: 1rem;
    margin-bottom: 1.5rem;
  }

  .strip {
    flex: 1;
    background: #16213e;
    border: 1px solid #333;
    border-radius: 6px;
    padding: 1rem;
  }

  .strip h3 {
    margin: 0 0 0.75rem;
    font-size: 0.9rem;
    color: #e94560;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .strip-control {
    margin-bottom: 0.75rem;
  }

  .strip-control:last-child {
    margin-bottom: 0;
  }

  .strip-control input[type="range"] {
    width: 100%;
    accent-color: #e94560;
    margin-top: 0.25rem;
  }

  .strip-control .toggle-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.9rem;
    color: #e0e0e0;
    text-transform: none;
    cursor: pointer;
  }

  .transport {
    display: flex;
    gap: 0.75rem;
  }

  .btn {
    padding: 0.65rem 1.5rem;
    border: none;
    border-radius: 4px;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn.start {
    background: #0f3460;
    color: #e0e0e0;
  }
  .btn.start:hover {
    background: #1a4a7a;
  }

  .btn.record {
    background: #e94560;
    color: #fff;
  }
  .btn.record:hover {
    background: #ff6b81;
  }

  .btn.stop {
    background: #333;
    color: #e0e0e0;
  }
  .btn.stop:hover {
    background: #444;
  }

  .status-text {
    font-size: 0.95rem;
    color: #aaa;
  }

  .error {
    color: #e94560;
    font-size: 0.85rem;
  }

  .result {
    color: #6c9;
    font-size: 0.85rem;
  }
</style>
