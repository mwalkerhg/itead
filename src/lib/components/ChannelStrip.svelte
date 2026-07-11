<script lang="ts">
  import type { ChannelStripParams } from '$lib/types';

  let { label, params = $bindable() }: { label: string; params: ChannelStripParams } = $props();
</script>

<div class="strip">
  <h3>{label}</h3>
  <div class="strip-control">
    <label>Gain: {params.gain_db.toFixed(1)} dB</label>
    <input type="range" min={-60} max={12} step={0.1} bind:value={params.gain_db} />
  </div>
  <div class="strip-control">
    <label class="toggle-label">
      <input type="checkbox" bind:checked={params.lowcut_enabled} />
      Low Cut
    </label>
    {#if params.lowcut_enabled}
      <label>{params.lowcut_freq_hz.toFixed(0)} Hz</label>
      <input type="range" min={20} max={500} step={1} bind:value={params.lowcut_freq_hz} />
    {/if}
  </div>
  <div class="strip-control">
    <label class="toggle-label">
      <input type="checkbox" bind:checked={params.phase_invert} />
      &#x2300; Phase Invert
    </label>
  </div>
  <div class="strip-control">
    <label>Tone</label>
    <select bind:value={params.tone_preset}>
      <option value="off">Off</option>
      <option value="vox_ac30">Vox AC30</option>
    </select>
    {#if params.tone_preset !== 'off'}
      <label>Drive: {(params.tone_drive * 100).toFixed(0)}%</label>
      <input type="range" min={0} max={1} step={0.01} bind:value={params.tone_drive} />
    {/if}
  </div>
  <div class="strip-control">
    <label class="toggle-label">
      <input type="checkbox" bind:checked={params.reverb_enabled} />
      Reverb
    </label>
    {#if params.reverb_enabled}
      <label>Room: {(params.reverb_room_size * 100).toFixed(0)}%</label>
      <input type="range" min={0} max={1} step={0.01} bind:value={params.reverb_room_size} />
      <label>Damping: {(params.reverb_damping * 100).toFixed(0)}%</label>
      <input type="range" min={0} max={1} step={0.01} bind:value={params.reverb_damping} />
      <label>Wet: {(params.reverb_wet * 100).toFixed(0)}%</label>
      <input type="range" min={0} max={1} step={0.01} bind:value={params.reverb_wet} />
    {/if}
  </div>
</div>

<style>
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

  label {
    display: block;
    margin-bottom: 0.35rem;
    font-size: 0.8rem;
    color: #aaa;
    text-transform: uppercase;
    letter-spacing: 0.05em;
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

  input[type="checkbox"] {
    accent-color: #e94560;
    width: 16px;
    height: 16px;
  }

  select {
    width: 100%;
    padding: 0.4rem;
    background: #1a1a2e;
    color: #e0e0e0;
    border: 1px solid #333;
    border-radius: 4px;
    font-size: 0.85rem;
    margin-bottom: 0.35rem;
  }
</style>
