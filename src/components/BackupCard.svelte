<script lang="ts">
  import type { Backup } from "../stores/backups";
  import {
    formatBytes,
    formatBackupDate,
    getBackupDisplayName,
  } from "../stores/backups";
  import {
    restoreBackupPopupStore,
    deleteBackupPopupStore,
  } from "../stores/modStore";
  import { Archive, RotateCcw } from "lucide-svelte";

  interface Props {
    backup: Backup;
  }

  let { backup }: Props = $props();

  const isManual = $derived(backup.trigger === "manual");

  function openRestoreConfirm() {
    restoreBackupPopupStore.set({
      visible: true,
      backupId: backup.id,
      backupName: getBackupDisplayName(backup),
    });
  }

  function openDeleteConfirm() {
    deleteBackupPopupStore.set({
      visible: true,
      backupId: backup.id,
      backupName: getBackupDisplayName(backup),
    });
  }
</script>

<div class="backup-card">
  <div class="backup-icon" class:manual={isManual}>
    {#if isManual}
      <Archive size={24} />
    {:else}
      <RotateCcw size={24} />
    {/if}
  </div>
  <div class="backup-info">
    <div class="backup-name">{getBackupDisplayName(backup)}</div>
    <div class="backup-metadata">
      <span class="date">{formatBackupDate(backup.created_at)}</span>
      <span class="separator">•</span>
      <span class="mod-count"
        >{backup.mod_count} mod{backup.mod_count !== 1 ? "s" : ""}</span
      >
      <span class="separator">•</span>
      <span class="size">{formatBytes(backup.size_bytes)}</span>
    </div>
    {#if backup.lovely_version}
      <div class="lovely-version">Lovely: {backup.lovely_version}</div>
    {/if}
  </div>
  <div class="backup-actions">
    <button class="restore-button" onclick={openRestoreConfirm}>
      Restore
    </button>
    <button class="delete-button" onclick={openDeleteConfirm}> Delete </button>
  </div>
</div>

<style>
  .backup-card {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 1rem;
    align-items: center;
    background: rgba(244, 238, 224, 0.05);
    border: 2px solid rgba(244, 238, 224, 0.3);
    border-radius: 8px;
    padding: 1rem 1.25rem;
    transition:
      background 0.2s ease,
      border-color 0.2s ease;
  }

  .backup-card:hover {
    background: rgba(244, 238, 224, 0.08);
    border-color: rgba(244, 238, 224, 0.4);
  }

  .backup-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 42px;
    height: 42px;
    border-radius: 8px;
    background: rgba(234, 150, 0, 0.2);
    color: #ea9600;
  }

  .backup-icon.manual {
    background: rgba(86, 167, 134, 0.2);
    color: #56a786;
  }

  .backup-info {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    min-width: 0;
  }

  .backup-name {
    font-family: "M6X11", sans-serif;
    font-size: 1.35rem;
    color: #fdcf51;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .backup-metadata {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    font-size: 1.1rem;
    color: rgba(244, 238, 224, 0.8);
  }

  .separator {
    opacity: 0.5;
  }

  .lovely-version {
    font-size: 1rem;
    color: rgba(244, 238, 224, 0.7);
  }

  .backup-actions {
    display: flex;
    gap: 0.5rem;
  }

  button {
    border: none;
    border-radius: 6px;
    padding: 0.7rem 1.1rem;
    font-family: "M6X11", sans-serif;
    font-size: 1.1rem;
    cursor: pointer;
    transition:
      transform 0.2s ease,
      background-color 0.2s ease;
    box-shadow: 0 3px 0 rgba(0, 0, 0, 0.2);
  }

  button:hover {
    transform: translateY(-2px);
    box-shadow: 0 5px 0 rgba(0, 0, 0, 0.2);
  }

  button:active {
    transform: translateY(0);
    box-shadow: 0 2px 0 rgba(0, 0, 0, 0.2);
  }

  .restore-button {
    background: #56a786;
    color: #f4eee0;
    border: 2px solid #459373;
  }

  .restore-button:hover {
    background: #67b897;
  }

  .delete-button {
    background: #c14139;
    color: #f4eee0;
    border: 2px solid #a13029;
  }

  .delete-button:hover {
    background: #d2524a;
  }

  @media (max-width: 768px) {
    .backup-card {
      grid-template-columns: 1fr;
      gap: 0.75rem;
      padding: 1rem;
    }

    .backup-icon {
      display: none;
    }

    .backup-metadata {
      flex-wrap: wrap;
      font-size: 0.9rem;
    }

    .backup-actions {
      width: 100%;
    }

    .backup-actions button {
      flex: 1;
      padding: 0.75rem;
    }
  }
</style>
