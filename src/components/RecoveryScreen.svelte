<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  interface Props {
    error?: unknown;
  }
  let { error }: Props = $props();

  let busy = $state(false);
  let resetStatus = $state<"idle" | "ok" | "err">("idle");
  let errMessage = $derived(
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : error
          ? String(error)
          : "Unknown error",
  );

  async function resetAndReload() {
    if (busy) return;
    busy = true;
    resetStatus = "idle";
    try {
      try {
        localStorage.clear();
      } catch (_) {
        /* ignore */
      }
      try {
        sessionStorage.clear();
      } catch (_) {
        /* ignore */
      }
      await invoke("clear_app_state");
      resetStatus = "ok";
      window.setTimeout(() => window.location.replace("/"), 600);
    } catch (e) {
      console.error("Reset failed:", e);
      resetStatus = "err";
      busy = false;
    }
  }

  function reloadOnly() {
    window.location.replace("/");
  }
</script>

<div class="recovery">
  <div class="panel">
    <h2>Something went wrong</h2>
    <p>
      Balatro Mod Manager couldn't finish loading. This is usually caused by
      stale cached data from a previous version.
    </p>
    <details>
      <summary>Error details</summary>
      <pre>{errMessage}</pre>
    </details>
    <div class="actions">
      <button class="primary" onclick={resetAndReload} disabled={busy}>
        {busy ? "Resetting…" : "Reset cache and reload"}
      </button>
      <button class="secondary" onclick={reloadOnly} disabled={busy}>
        Reload only
      </button>
    </div>
    {#if resetStatus === "ok"}
      <p class="status ok">Cache cleared. Reloading…</p>
    {:else if resetStatus === "err"}
      <p class="status err">
        Reset failed. Try closing the app and deleting the Balatro folder under
        your config directory manually.
      </p>
    {/if}
    <p class="hint">
      Installed mods stay intact. Only cached catalog data and thumbnails are
      removed.
    </p>
  </div>
</div>

<style>
  .recovery {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(16, 18, 45, 0.96);
    z-index: 2000;
    font-family: "M6X11", sans-serif;
    color: #f4eee0;
  }

  .panel {
    max-width: 520px;
    width: calc(100% - 2rem);
    padding: 1.75rem 2rem;
    background: #1a1e3c;
    border: 2px solid rgba(244, 238, 224, 0.2);
    border-radius: 10px;
    box-shadow: 0 12px 36px rgba(0, 0, 0, 0.5);
  }

  h2 {
    margin: 0 0 0.75rem;
    font-size: 1.5rem;
  }

  p {
    margin: 0.5rem 0;
    line-height: 1.5;
    font-size: 0.95rem;
  }

  details {
    margin: 0.75rem 0;
    font-size: 0.85rem;
  }

  details pre {
    white-space: pre-wrap;
    word-break: break-word;
    background: rgba(0, 0, 0, 0.3);
    padding: 0.5rem;
    border-radius: 4px;
    margin: 0.5rem 0 0;
    max-height: 160px;
    overflow: auto;
  }

  .actions {
    display: flex;
    gap: 0.75rem;
    margin-top: 1rem;
    flex-wrap: wrap;
  }

  button {
    font-family: inherit;
    font-size: 1rem;
    padding: 0.55rem 1.1rem;
    border-radius: 6px;
    border: 2px solid transparent;
    cursor: pointer;
    transition:
      transform 0.05s ease,
      background 0.15s ease;
  }
  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  button:not(:disabled):hover {
    transform: translateY(-1px);
  }

  .primary {
    background: #fdcf51;
    color: #10122d;
  }
  .primary:not(:disabled):hover {
    background: #ea9600;
  }

  .secondary {
    background: transparent;
    color: #f4eee0;
    border-color: rgba(244, 238, 224, 0.4);
  }
  .secondary:not(:disabled):hover {
    background: rgba(244, 238, 224, 0.1);
  }

  .status {
    margin-top: 0.75rem;
    font-weight: bold;
  }
  .status.ok {
    color: #6fdc8c;
  }
  .status.err {
    color: #ff7a7a;
  }

  .hint {
    margin-top: 1rem;
    font-size: 0.8rem;
    opacity: 0.7;
  }
</style>
