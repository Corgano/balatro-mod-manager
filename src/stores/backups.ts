import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";

export type BackupTrigger =
  | "auto_update"
  | "auto_uninstall"
  | "auto_bulk"
  | "manual";

export interface Backup {
  id: string;
  created_at: string;
  trigger: BackupTrigger;
  name: string | null;
  mod_count: number;
  size_bytes: number;
  lovely_version: string | null;
}

export interface BackupsState {
  backups: Backup[];
  loading: boolean;
  totalSize: number;
  error: string | null;
}

function createBackupsStore() {
  const { subscribe, set, update } = writable<BackupsState>({
    backups: [],
    loading: false,
    totalSize: 0,
    error: null,
  });

  return {
    subscribe,

    async load() {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const [backups, totalSize] = await Promise.all([
          invoke<Backup[]>("list_backups"),
          invoke<number>("get_backups_total_size"),
        ]);
        update((s) => ({
          ...s,
          backups,
          totalSize,
          loading: false,
        }));
      } catch (e) {
        update((s) => ({
          ...s,
          loading: false,
          error: e instanceof Error ? e.message : String(e),
        }));
      }
    },

    async createBackup(name?: string): Promise<Backup | null> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const backup = await invoke<Backup>("create_backup", {
          trigger: "manual",
          name: name || null,
          context: null,
        });
        await this.load();
        return backup;
      } catch (e) {
        update((s) => ({
          ...s,
          loading: false,
          error: e instanceof Error ? e.message : String(e),
        }));
        return null;
      }
    },

    async restoreBackup(backupId: string): Promise<boolean> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        await invoke("restore_backup", { backupId });
        update((s) => ({ ...s, loading: false }));
        return true;
      } catch (e) {
        update((s) => ({
          ...s,
          loading: false,
          error: e instanceof Error ? e.message : String(e),
        }));
        return false;
      }
    },

    async deleteBackup(backupId: string): Promise<boolean> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        await invoke("delete_backup", { backupId });
        await this.load();
        return true;
      } catch (e) {
        update((s) => ({
          ...s,
          loading: false,
          error: e instanceof Error ? e.message : String(e),
        }));
        return false;
      }
    },

    async getBackupsDirectory(): Promise<string | null> {
      try {
        return await invoke<string>("get_backups_directory");
      } catch {
        return null;
      }
    },

    async checkInterruptedRestore(): Promise<string | null> {
      try {
        return await invoke<string | null>("check_interrupted_restore");
      } catch {
        return null;
      }
    },

    async clearInterruptedRestore(): Promise<void> {
      try {
        await invoke("clear_interrupted_restore");
      } catch {
        // Ignore errors
      }
    },

    clearError() {
      update((s) => ({ ...s, error: null }));
    },
  };
}

export const backupsStore = createBackupsStore();

// Helper to create auto-backup before operations
export async function createAutoBackup(
  trigger: "auto_update" | "auto_uninstall" | "auto_bulk",
  context?: string,
): Promise<void> {
  try {
    await invoke("create_backup", {
      trigger,
      name: null,
      context: context || null,
    });
  } catch (e) {
    console.warn("Failed to create auto-backup:", e);
  }
}

// Format bytes to human-readable size
export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

// Format date to localized string
export function formatBackupDate(isoDate: string): string {
  const date = new Date(isoDate);
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

// Get display name for backup
export function getBackupDisplayName(backup: Backup): string {
  if (backup.name) {
    return backup.name;
  }
  switch (backup.trigger) {
    case "auto_update":
      return "Before updating mod";
    case "auto_uninstall":
      return "Before uninstalling mod";
    case "auto_bulk":
      return "Before bulk operation";
    case "manual":
      return "Manual backup";
    default:
      return "Backup";
  }
}
