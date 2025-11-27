import { writable } from "svelte/store";

function createPersistentBoolean(key: string, fallback: boolean) {
  const isBrowser = typeof window !== "undefined";
  let initial = fallback;
  if (isBrowser) {
    try {
      const raw = localStorage.getItem(key);
      if (raw != null) initial = raw === "true";
    } catch (err) {
      console.warn("Failed to read update prompt setting:", err);
    }
  }
  const store = writable<boolean>(initial);
  if (isBrowser) {
    store.subscribe((val) => {
      try {
        localStorage.setItem(key, val ? "true" : "false");
      } catch (err) {
        console.warn("Failed to persist update prompt setting:", err);
      }
    });
  }
  return store;
}

// If true, never show the update-available popup
export const updatePromptDisabled = createPersistentBoolean(
  "ui.updatePromptDisabled",
  false,
);
