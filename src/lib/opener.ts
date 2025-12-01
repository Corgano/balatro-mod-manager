import { browser } from "$app/environment";
import { invoke } from "@tauri-apps/api/core";

/**
 * Open an external URL using the Tauri opener plugin with a browser fallback.
 */
export async function openExternal(url: string): Promise<void> {
  if (!url) return;

  try {
    await invoke("plugin:opener|open_url", { url });
  } catch (error) {
    // Fallback for web/dev builds where the plugin isn't available.
    if (browser) {
      window.open(url, "_blank", "noopener,noreferrer");
    } else {
      console.error("Failed to open external URL", error);
    }
  }
}
