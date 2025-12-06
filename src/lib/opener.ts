import { browser } from "$app/environment";
import { invoke } from "@tauri-apps/api/core";
import { addMessage } from "$lib/stores";

/**
 * Open an external URL using the Tauri opener plugin with a browser fallback.
 */
export async function openExternal(url: string): Promise<void> {
  if (!url) return;

  try {
    // Prefer our internal command (works in Flatpak and respects capability perms).
    await invoke("open_external_url", { url });
    return;
  } catch (error) {
    // fall through to plugin/window fallback
    console.warn(
      "open_external_url failed, falling back to opener plugin",
      error,
    );
  }

  try {
    await invoke("plugin:opener|open_url", { url });
  } catch (error) {
    // Fallback for web/dev builds where the plugin isn't available.
    if (browser) {
      window.open(url, "_blank", "noopener,noreferrer");
      return;
    }
    console.error("Failed to open external URL", error);
    addMessage(
      "Failed to open link. Please check your browser configuration.",
      "error",
    );
  }
}
