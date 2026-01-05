// stores/modCache.ts
import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { InstalledMod } from "./modStore";
import { installationStatus, modsStore } from "./modStore";

declare global {
  // eslint-disable-next-line no-unused-vars
  interface Window {
    __bmmInstalledModsListenerAttached?: boolean;
    __bmmInstalledModsUnlisten?: () => void;
  }
}

// Create a self-contained cache system
const createModCache = () => {
  // Private variables inside closure
  const cache = writable<InstalledMod[]>([]);
  let lastFetchTime = 0;
  const CACHE_TIMEOUT = 15000; // 15 seconds to avoid chatty IPC
  let inFlight: Promise<InstalledMod[]> | null = null; // coalesce concurrent calls

  // Core cache function that handles all operations
  async function getModsFromCache(
    forceRefresh = false,
  ): Promise<InstalledMod[]> {
    const now = Date.now();

    if (
      forceRefresh ||
      lastFetchTime === 0 ||
      now - lastFetchTime > CACHE_TIMEOUT
    ) {
      try {
        // Deduplicate concurrent requests
        if (!inFlight) {
          inFlight = (async () => {
            const installed: InstalledMod[] = await invoke(
              "get_installed_mods_from_db",
            );
            const formattedMods = installed.map((mod) => ({
              name: mod.name,
              path: mod.path,
            }));

            cache.set(formattedMods);
            lastFetchTime = Date.now();
            return formattedMods;
          })();
        }

        const result = await inFlight;
        inFlight = null;
        return result;
      } catch (error) {
        console.error("Failed to get installed mods:", error);
        inFlight = null;
        return [];
      }
    }

    // Return current value from store
    return get(cache);
  }

  // Public interface
  return {
    // Exported store for reactive access
    installedModsCache: cache,

    // Get mods with optional force refresh
    fetchCachedMods: async (forceRefresh = false) => {
      return getModsFromCache(forceRefresh);
    },

    // Check if a specific mod is in the cache
    checkModInCache: async (modTitle: string) => {
      if (!modTitle) return false;
      const mods = await getModsFromCache();
      return mods.some((m) => m.name.toLowerCase() === modTitle.toLowerCase());
    },

    // Force refresh the cache
    forceRefreshCache: async () => {
      return getModsFromCache(true);
    },
  };
};

// Create a single instance of the cache system
const modCache = createModCache();

// Listen for backend notifications that installed mods have changed,
// and refresh the cache immediately to update the UI in real-time.
// Guard against duplicate listeners during Vite HMR by stashing a flag on window.
try {
  if (typeof window !== "undefined") {
    if (!window.__bmmInstalledModsListenerAttached) {
      window.__bmmInstalledModsListenerAttached = true;
      listen("installed-mods-changed", async () => {
        try {
          const installed = await modCache.forceRefreshCache();
          const installedSet = new Set(
            installed.map((mod) => mod.name.toLowerCase()),
          );
          const mods = get(modsStore);
          installationStatus.set(
            Object.fromEntries(
              mods.map((mod) => [
                mod.title,
                installedSet.has(mod.title.toLowerCase()),
              ]),
            ),
          );
        } catch {
          // ignore
        }
      })
        .then((un) => {
          window.__bmmInstalledModsUnlisten = un;
          if (import.meta?.hot) {
            import.meta.hot.dispose(() => {
              try {
                window.__bmmInstalledModsUnlisten?.();
              } catch (err) {
                console.warn("Failed to dispose installed-mods listener:", err);
              }
              window.__bmmInstalledModsListenerAttached = false;
            });
          }
        })
        .catch((err) => {
          console.warn("Failed to attach installed-mods listener:", err);
        });
    }
  }
} catch {
  // ignore if listen fails outside Tauri context
}

// Export the public interface
export const {
  installedModsCache,
  fetchCachedMods,
  checkModInCache,
  forceRefreshCache,
} = modCache;
