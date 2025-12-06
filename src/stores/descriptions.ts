import { writable } from "svelte/store";

export const descriptionsStore = writable<Record<string, string>>({});

export function setDescription(title: string, description: string) {
  descriptionsStore.update((map) => ({
    ...map,
    [title]: description,
  }));
}

export function setDescriptions(batch: Record<string, string>) {
  if (!batch || Object.keys(batch).length === 0) return;
  descriptionsStore.update((map) => ({
    ...map,
    ...batch,
  }));
}

const CACHE_KEY = "mods-descriptions-cache";
let persistTimer: number | null = null;

// Hydrate from localStorage (best-effort)
if (typeof window !== "undefined") {
  try {
    const raw = window.localStorage.getItem(CACHE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === "object") {
        descriptionsStore.set(parsed);
      }
    }
  } catch {
    // ignore malformed cache
  }

  // Persist with a debounce to avoid thrashing during hydration
  descriptionsStore.subscribe((val) => {
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = window.setTimeout(() => {
      try {
        window.localStorage.setItem(CACHE_KEY, JSON.stringify(val));
      } catch {
        // ignore quota errors
      }
    }, 300);
  });
}
