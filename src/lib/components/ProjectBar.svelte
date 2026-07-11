<script lang="ts">
  import { app, applyProject, buildProjectManifest } from '$lib/state.svelte';
  import * as api from '$lib/api';

  let showDropdown = $state(false);
  let newProjectName = $state('');
  let showNewInput = $state(false);

  async function refreshProjects() {
    app.projectList = await api.listProjects();
  }

  async function handleCreate() {
    if (!newProjectName.trim()) return;
    try {
      app.error = '';
      const manifest = await api.createProject(newProjectName.trim());
      applyProject(manifest);
      await api.saveAppSettings({
        last_project: manifest.name,
        input_device: null,
        output_device: null,
        sample_rate: 48000,
        buffer_size: 256,
      });
      newProjectName = '';
      showNewInput = false;
      showDropdown = false;
      await refreshProjects();
    } catch (e) {
      app.error = `${e}`;
    }
  }

  async function handleOpen(name: string) {
    if (app.engineRunning) return;
    try {
      app.error = '';
      const manifest = await api.openProject(name);
      applyProject(manifest);
      await api.saveAppSettings({
        last_project: name,
        input_device: null,
        output_device: null,
        sample_rate: 48000,
        buffer_size: 256,
      });
      showDropdown = false;
    } catch (e) {
      app.error = `${e}`;
    }
  }

  async function handleSave() {
    const manifest = buildProjectManifest();
    if (!manifest) return;
    try {
      app.error = '';
      await api.saveProject(manifest);
      app.status = 'Project saved';
    } catch (e) {
      app.error = `${e}`;
    }
  }

  function toggleDropdown() {
    showDropdown = !showDropdown;
    if (showDropdown) {
      refreshProjects();
      showNewInput = false;
    }
  }
</script>

<div class="project-bar">
  <div class="project-info">
    <span class="project-label">Project:</span>
    <button class="project-name" onclick={toggleDropdown}>
      {app.currentProject?.name ?? 'No Project'}
      <span class="arrow">{showDropdown ? '▲' : '▼'}</span>
    </button>
    {#if app.currentProject}
      <button class="save-btn" onclick={handleSave}>Save</button>
    {/if}
  </div>

  {#if showDropdown}
    <div class="dropdown">
      {#each app.projectList as name}
        <button
          class="dropdown-item"
          class:active={name === app.currentProject?.name}
          disabled={app.engineRunning}
          onclick={() => handleOpen(name)}
        >{name}</button>
      {/each}

      {#if showNewInput}
        <div class="new-project-row">
          <input
            type="text"
            placeholder="Project name..."
            bind:value={newProjectName}
            onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && handleCreate()}
          />
          <button class="create-btn" onclick={handleCreate}>Create</button>
        </div>
      {:else}
        <button class="dropdown-item new" onclick={() => (showNewInput = true)}>
          + New Project
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .project-bar {
    position: relative;
    margin-bottom: 1.5rem;
  }

  .project-info {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .project-label {
    font-size: 0.8rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .project-name {
    background: #16213e;
    border: 1px solid #333;
    border-radius: 4px;
    color: #e0e0e0;
    padding: 0.4rem 0.75rem;
    font-size: 0.9rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .project-name:hover {
    border-color: #555;
  }

  .arrow {
    font-size: 0.6rem;
    color: #888;
  }

  .save-btn {
    background: #0f3460;
    border: none;
    border-radius: 4px;
    color: #e0e0e0;
    padding: 0.4rem 0.75rem;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .save-btn:hover {
    background: #1a4a7a;
  }

  .dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    margin-top: 0.25rem;
    background: #16213e;
    border: 1px solid #333;
    border-radius: 4px;
    min-width: 220px;
    z-index: 10;
    overflow: hidden;
  }

  .dropdown-item {
    display: block;
    width: 100%;
    padding: 0.5rem 0.75rem;
    background: none;
    border: none;
    color: #e0e0e0;
    font-size: 0.9rem;
    text-align: left;
    cursor: pointer;
  }

  .dropdown-item:hover {
    background: #1a2744;
  }

  .dropdown-item.active {
    color: #e94560;
  }

  .dropdown-item:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .dropdown-item.new {
    color: #e94560;
    border-top: 1px solid #333;
  }

  .new-project-row {
    display: flex;
    padding: 0.5rem;
    gap: 0.5rem;
    border-top: 1px solid #333;
  }

  .new-project-row input {
    flex: 1;
    padding: 0.35rem 0.5rem;
    background: #1a1a2e;
    border: 1px solid #333;
    border-radius: 3px;
    color: #e0e0e0;
    font-size: 0.85rem;
  }

  .create-btn {
    background: #e94560;
    border: none;
    border-radius: 3px;
    color: #fff;
    padding: 0.35rem 0.6rem;
    font-size: 0.8rem;
    cursor: pointer;
  }
</style>
