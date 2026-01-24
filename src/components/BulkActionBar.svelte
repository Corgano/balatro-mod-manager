<script lang="ts">
  import {
    Check,
    X,
    Power,
    PowerOff,
    Trash2,
    ChevronDown,
  } from "lucide-svelte";
  import { fly } from "svelte/transition";
  import { onMount, onDestroy } from "svelte";
  import {
    selectedModsStore,
    clearSelection,
    selectAllMods,
    modEnabledStore,
    batchUpdateModEnabled,
    installationStatus,
  } from "../stores/modStore";
  import { darkMode } from "../stores/ui";
  import { invoke } from "@tauri-apps/api/core";
  import { addMessage } from "$lib/stores";

  interface Props {
    allSelectableIds: string[];
    catalogModIds: string[];
    localModIds: string[];
    localModPaths: Map<string, string>; // Maps mod path to path for local mods
    localModNames: Map<string, string>; // Maps mod path to name for store updates
    onBulkUninstall: (ids: string[]) => void;
    onRefresh: () => void;
  }

  let {
    allSelectableIds,
    catalogModIds,
    localModIds,
    localModPaths,
    localModNames,
    onBulkUninstall,
    onRefresh,
  }: Props = $props();

  let isBusy = $state(false);
  let selectDropdownOpen = $state(false);

  let selectedCount = $derived($selectedModsStore.size);
  let hasSelection = $derived(selectedCount > 0);

  // Check if all selected mods are already enabled or disabled
  let allSelectedEnabled = $derived(() => {
    if ($selectedModsStore.size === 0) return false;
    for (const id of $selectedModsStore) {
      // For local mods, look up by name; for catalog mods, use id directly
      const storeKey = localModNames.get(id) || id;
      // If modEnabledStore[storeKey] is undefined or false, it's not enabled
      if ($modEnabledStore[storeKey] !== true) return false;
    }
    return true;
  });

  let allSelectedDisabled = $derived(() => {
    if ($selectedModsStore.size === 0) return false;
    for (const id of $selectedModsStore) {
      // For local mods, look up by name; for catalog mods, use id directly
      const storeKey = localModNames.get(id) || id;
      // If modEnabledStore[storeKey] is undefined, treat as enabled (default state)
      if ($modEnabledStore[storeKey] !== false) return false;
    }
    return true;
  });

  function handleSelectAll() {
    selectAllMods(allSelectableIds);
    selectDropdownOpen = false;
  }

  function handleSelectCatalog() {
    selectAllMods(catalogModIds);
    selectDropdownOpen = false;
  }

  function handleSelectLocal() {
    selectAllMods(localModIds);
    selectDropdownOpen = false;
  }

  function handleClear() {
    clearSelection();
  }

  function toggleSelectDropdown(e: Event) {
    e.stopPropagation();
    selectDropdownOpen = !selectDropdownOpen;
  }

  function closeDropdown() {
    selectDropdownOpen = false;
  }

  // Close dropdown when clicking outside
  function handleClickOutside(event: MouseEvent) {
    if (selectDropdownOpen) {
      const target = event.target as HTMLElement;
      const dropdown = document.querySelector(".select-dropdown-wrapper");
      if (dropdown && !dropdown.contains(target)) {
        selectDropdownOpen = false;
      }
    }
  }

  onMount(() => {
    document.addEventListener("click", handleClickOutside);
  });

  onDestroy(() => {
    document.removeEventListener("click", handleClickOutside);
  });

  async function handleEnableSelected() {
    if (isBusy) return;
    isBusy = true;

    const selectedIds = Array.from($selectedModsStore);

    // Run all toggle operations in parallel
    const results = await Promise.allSettled(
      selectedIds.map(async (id) => {
        const localPath = localModPaths.get(id);
        if (localPath) {
          await invoke("toggle_mod_enabled_by_path", {
            modPath: localPath,
            enabled: true,
          });
          // Return the mod name for store update (local mods use name as key)
          return localModNames.get(id) || id;
        } else {
          await invoke("toggle_mod_enabled", {
            modName: id,
            enabled: true,
          });
          return id;
        }
      }),
    );

    // Batch update store once after all operations complete
    const updates: Record<string, boolean> = {};
    let successCount = 0;
    let failCount = 0;

    results.forEach((result) => {
      if (result.status === "fulfilled") {
        updates[result.value] = true;
        successCount++;
      } else {
        console.error(`Failed to enable mod:`, result.reason);
        failCount++;
      }
    });

    batchUpdateModEnabled(updates);

    if (failCount > 0) {
      addMessage(
        `Enabled ${successCount} mods, ${failCount} failed`,
        "warning",
      );
    } else {
      addMessage(`Enabled ${successCount} mods`, "success");
    }

    clearSelection();
    onRefresh();
    isBusy = false;
  }

  async function handleDisableSelected() {
    if (isBusy) return;
    isBusy = true;

    const selectedIds = Array.from($selectedModsStore);

    // Run all toggle operations in parallel
    const results = await Promise.allSettled(
      selectedIds.map(async (id) => {
        const localPath = localModPaths.get(id);
        if (localPath) {
          await invoke("toggle_mod_enabled_by_path", {
            modPath: localPath,
            enabled: false,
          });
          // Return the mod name for store update (local mods use name as key)
          return localModNames.get(id) || id;
        } else {
          await invoke("toggle_mod_enabled", {
            modName: id,
            enabled: false,
          });
          return id;
        }
      }),
    );

    // Batch update store once after all operations complete
    const updates: Record<string, boolean> = {};
    let successCount = 0;
    let failCount = 0;

    results.forEach((result) => {
      if (result.status === "fulfilled") {
        updates[result.value] = false;
        successCount++;
      } else {
        console.error(`Failed to disable mod:`, result.reason);
        failCount++;
      }
    });

    batchUpdateModEnabled(updates);

    if (failCount > 0) {
      addMessage(
        `Disabled ${successCount} mods, ${failCount} failed`,
        "warning",
      );
    } else {
      addMessage(`Disabled ${successCount} mods`, "success");
    }

    clearSelection();
    onRefresh();
    isBusy = false;
  }

  function handleUninstallSelected() {
    if (isBusy) return;
    const selectedIds = Array.from($selectedModsStore);
    onBulkUninstall(selectedIds);
  }
</script>

{#if hasSelection}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="bulk-action-bar"
    class:light={!$darkMode}
    transition:fly={{ y: 20, duration: 150 }}
    onclick={closeDropdown}
  >
    <span class="count"
      >{selectedCount} mod{selectedCount !== 1 ? "s" : ""} selected</span
    >

    <div class="select-dropdown-wrapper">
      <button
        class="action-btn select-all"
        onclick={toggleSelectDropdown}
        disabled={isBusy}
        title="Select options"
      >
        <Check size={16} />
        Select
        <ChevronDown size={14} />
      </button>
      {#if selectDropdownOpen}
        <div
          class="select-dropdown"
          class:light={!$darkMode}
          transition:fly={{ y: -5, duration: 100 }}
        >
          <button class="dropdown-item" onclick={handleSelectAll}>
            All ({allSelectableIds.length})
          </button>
          {#if catalogModIds.length > 0}
            <button class="dropdown-item" onclick={handleSelectCatalog}>
              Catalog Mods ({catalogModIds.length})
            </button>
          {/if}
          {#if localModIds.length > 0}
            <button class="dropdown-item" onclick={handleSelectLocal}>
              Local Mods ({localModIds.length})
            </button>
          {/if}
        </div>
      {/if}
    </div>
    <button
      class="action-btn clear"
      onclick={handleClear}
      disabled={isBusy}
      title="Clear selection"
    >
      <X size={16} />
      Clear
    </button>

    <div class="divider"></div>

    <button
      class="action-btn enable"
      onclick={handleEnableSelected}
      disabled={isBusy || allSelectedEnabled()}
      title={allSelectedEnabled()
        ? "All selected mods are already enabled"
        : "Enable selected mods"}
    >
      <Power size={16} />
      Enable
    </button>
    <button
      class="action-btn disable"
      onclick={handleDisableSelected}
      disabled={isBusy || allSelectedDisabled()}
      title={allSelectedDisabled()
        ? "All selected mods are already disabled"
        : "Disable selected mods"}
    >
      <PowerOff size={16} />
      Disable
    </button>
    <button
      class="action-btn uninstall"
      onclick={handleUninstallSelected}
      disabled={isBusy}
      title="Uninstall selected mods"
    >
      <Trash2 size={16} />
      Uninstall
    </button>
  </div>
{/if}

<style>
  .bulk-action-bar {
    position: fixed;
    bottom: 0.5rem;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem;
    background: var(--ui-panel-bg);
    border: 2px solid var(--ui-mod-panel-border);
    border-radius: 8px;
    z-index: 1000;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
  }

  .count {
    font-family: "M6X11", sans-serif;
    font-size: 1rem;
    color: var(--ui-warning);
    white-space: nowrap;
    padding-right: 0.5rem;
  }

  .divider {
    width: 1px;
    height: 24px;
    background: var(--ui-glass-border);
    margin: 0 0.25rem;
  }

  .action-btn {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.4rem 0.6rem;
    border: none;
    border-radius: 4px;
    font-family: "M6X11", sans-serif;
    font-size: 0.85rem;
    cursor: pointer;
    transition: all 0.15s ease;
    color: var(--ui-text);
    outline: none;
  }

  .action-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .action-btn.select-all,
  .action-btn.clear {
    background: var(--ui-neutral);
    border: 1px solid var(--ui-neutral-outline);
  }

  .action-btn.select-all:hover:not(:disabled),
  .action-btn.clear:hover:not(:disabled) {
    background: var(--ui-neutral-hover);
  }

  .action-btn.enable {
    background: var(--ui-success);
    border: 1px solid var(--ui-button-green-border);
  }

  .action-btn.enable:hover:not(:disabled) {
    background: var(--ui-success-hover);
  }

  .action-btn.disable {
    background: #b8860b;
    border: 1px solid #8b6508;
  }

  .action-btn.disable:hover:not(:disabled) {
    background: #9a7209;
  }

  .action-btn.uninstall {
    background: var(--ui-danger);
    border: 1px solid var(--ui-danger-outline);
  }

  .action-btn.uninstall:hover:not(:disabled) {
    background: var(--ui-danger-hover);
  }

  .action-btn:active:not(:disabled) {
    transform: translateY(1px);
  }

  .select-dropdown-wrapper {
    position: relative;
  }

  .select-dropdown {
    position: absolute;
    bottom: 100%;
    left: 0;
    margin-bottom: 0.5rem;
    background: #1a1a1a;
    border: 2px solid white;
    border-radius: 6px;
    min-width: 160px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.6);
    z-index: 1001;
    overflow: hidden;
  }

  .dropdown-item {
    display: block;
    width: 100%;
    padding: 0.6rem 0.85rem;
    border: none;
    background: transparent;
    color: white;
    font-family: "M6X11", sans-serif;
    font-size: 0.9rem;
    text-align: left;
    cursor: pointer;
    transition: background 0.1s ease;
  }

  .dropdown-item:hover {
    background: var(--ui-success);
  }

  .dropdown-item:not(:last-child) {
    border-bottom: 1px solid rgba(255, 255, 255, 0.2);
  }

  /* Light mode styles - Balatro red theme */
  .select-dropdown.light {
    background: #a33d48;
    border-color: white;
  }

  .select-dropdown.light .dropdown-item {
    color: white;
  }

  .select-dropdown.light .dropdown-item:hover {
    background: #8a2a35;
  }

  .select-dropdown.light .dropdown-item:not(:last-child) {
    border-bottom: 1px solid rgba(255, 255, 255, 0.25);
  }

  /* Light mode disable button - slightly brighter */
  .bulk-action-bar.light .action-btn.disable {
    background: #cc9a0c;
    border-color: #a67d08;
  }

  .bulk-action-bar.light .action-btn.disable:hover:not(:disabled) {
    background: #b8860b;
  }

  @media (max-width: 768px) {
    .bulk-action-bar {
      flex-wrap: wrap;
      justify-content: center;
      max-width: 90vw;
    }

    .divider {
      display: none;
    }

    .action-btn {
      padding: 0.35rem 0.5rem;
      font-size: 0.8rem;
    }
  }
</style>
