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

  const STANDARD_RATES = [44100, 48000, 88200, 96000, 176400, 192000];

  let inputDevices: DeviceInfo[] = $state([]);
  let outputDevices: DeviceInfo[] = $state([]);
  let selectedInput: string = $state('');
  let selectedOutput: string = $state('');
  let sampleRate: number = $state(48000);
  let bufferSize: number = $state(256);
  let mono: boolean = $state(false);

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

  async function startEngine() {
    try {
      error = '';
      await invoke('start_engine', {
        config: {
          input_device: selectedInput || null,
          output_device: selectedOutput || null,
          sample_rate: sampleRate,
          buffer_size: bufferSize,
          mono
        }
      });
      engineRunning = true;
      status = 'Passthrough active';
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

    <div class="field checkbox">
      <label>
        <input type="checkbox" bind:checked={mono} disabled={engineRunning} />
        Mono (input 1 → both L+R)
      </label>
    </div>
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

  .checkbox label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.9rem;
    color: #e0e0e0;
    text-transform: none;
    cursor: pointer;
    padding-top: 1.1rem;
  }

  input[type="checkbox"] {
    accent-color: #e94560;
    width: 16px;
    height: 16px;
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
