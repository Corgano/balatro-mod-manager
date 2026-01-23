<script lang="ts">
  import { onMount } from "svelte";
  import { backupsStore, formatBytes } from "../stores/backups";
  import { invoke } from "@tauri-apps/api/core";
  import BackupCard from "./BackupCard.svelte";
  import { addMessage } from "$lib/stores";
  import { Save, FolderOpen } from "lucide-svelte";
  import { createBackupPopupStore } from "../stores/modStore";

  onMount(() => {
    backupsStore.load();
  });

  async function handleOpenFolder() {
    try {
      const dir = await backupsStore.getBackupsDirectory();
      if (dir) {
        await invoke("open_directory", { path: dir });
      } else {
        addMessage("Failed to get backups directory", "error");
      }
    } catch (e) {
      addMessage(
        `Failed to open backups folder: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    }
  }

  function handleCreateClick() {
    createBackupPopupStore.set({ visible: true });
  }
</script>

<div class="backups-container">
  <h2>Backups</h2>

  <div class="actions-row">
    <button class="action-button primary" onclick={handleCreateClick}>
      <Save size={18} />
      Create Backup
    </button>
    <button class="action-button secondary" onclick={handleOpenFolder}>
      <FolderOpen size={18} />
      Open Backups Folder
    </button>
  </div>

  {#if $backupsStore.totalSize > 0}
    <p class="size-info">Total backup size: <span class="highlight">{formatBytes($backupsStore.totalSize)}</span></p>
  {/if}

  {#if $backupsStore.loading && $backupsStore.backups.length === 0}
    <div class="status-message">Loading backups...</div>
  {:else if $backupsStore.error && $backupsStore.backups.length === 0}
    <div class="status-message error">{$backupsStore.error}</div>
  {:else if $backupsStore.backups.length === 0}
    <div class="empty-state">
      <p>No backups yet.</p>
      <p class="hint">Create a backup to save your current mods configuration.</p>
    </div>
  {:else}
    <div class="backups-list">
      {#each $backupsStore.backups as backup (backup.id)}
        <BackupCard {backup} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .backups-container {
    padding: 1.5rem;
  }

  h2 {
    color: #fdcf51;
    font-size: 2.8rem;
    margin: 0 0 2rem 0;
    font-family: "M6X11", sans-serif;
    text-shadow:
      -2px -2px 0 #000,
      2px -2px 0 #000,
      -2px 2px 0 #000,
      2px 2px 0 #000;
  }

  .actions-row {
    display: flex;
    gap: 1rem;
    margin-bottom: 1.5rem;
    flex-wrap: wrap;
  }

  .action-button {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1.25rem;
    border-radius: 6px;
    font-family: "M6X11", sans-serif;
    font-size: 1.1rem;
    cursor: pointer;
    transition: transform 0.15s ease, background-color 0.15s ease;
    box-shadow: 0 3px 0 rgba(0, 0, 0, 0.3);
  }

  .action-button:hover {
    transform: translateY(-2px);
    box-shadow: 0 5px 0 rgba(0, 0, 0, 0.3);
  }

  .action-button:active {
    transform: translateY(0);
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.3);
  }

  .action-button.primary {
    background: #56a786;
    color: #f4eee0;
    border: 2px solid #459373;
  }

  .action-button.primary:hover {
    background: #67b897;
  }

  .action-button.secondary {
    background: #ea9600;
    color: #f4eee0;
    border: 2px solid #cc8400;
  }

  .action-button.secondary:hover {
    background: #fca800;
  }

  .size-info {
    color: #f4eee0;
    font-family: "M6X11", sans-serif;
    font-size: 1.15rem;
    margin-bottom: 1rem;
  }

  .highlight {
    color: #fdcf51;
  }

  .status-message {
    color: #f4eee0;
    font-family: "M6X11", sans-serif;
    font-size: 1.1rem;
    padding: 2rem;
    text-align: center;
    opacity: 0.8;
  }

  .status-message.error {
    color: #c14139;
  }

  .empty-state {
    text-align: center;
    padding: 3rem 1rem;
    color: #f4eee0;
    font-family: "M6X11", sans-serif;
  }

  .empty-state p {
    margin: 0.5rem 0;
    font-size: 1.3rem;
  }

  .empty-state .hint {
    font-size: 1.15rem;
    opacity: 0.7;
  }

  .backups-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  @media (max-width: 768px) {
    .backups-container {
      padding: 1rem;
    }

    h2 {
      font-size: 1.8rem;
    }

    .actions-row {
      flex-direction: column;
    }

    .action-button {
      width: 100%;
      justify-content: center;
    }
  }
</style>
