import {
  writeFile,
  readFile,
  mkdir,
  exists,
  readDir,
  remove,
  stat,
} from "@tauri-apps/plugin-fs";
import { BaseDirectory } from "@tauri-apps/api/path";

// Use native fetch to reduce bundle size

const CACHE_DIR = "cache";

/**
 * Upper bound on the total bytes held by the on-disk image cache. When a
 * write pushes the cache past this limit, the oldest files (by mtime) get
 * evicted until total size is back under the trim target. 64 MB is large
 * enough to hold hundreds of thumbnails but small enough to avoid AppData
 * bloat over months of use.
 */
const MAX_CACHE_BYTES = 64 * 1024 * 1024;
const TRIM_CACHE_BYTES = 48 * 1024 * 1024;

/** How long a successful prune is trusted before we recompute size. */
const PRUNE_CHECK_INTERVAL_MS = 60_000;

let lastPruneCheckAt = 0;
let prunePromise: Promise<void> | null = null;

interface CacheEntry {
  path: string;
  size: number;
  mtimeMs: number;
}

async function listCacheEntries(): Promise<CacheEntry[]> {
  const entries: CacheEntry[] = [];
  let dir: { name: string; isFile: boolean }[];
  try {
    dir = (await readDir(CACHE_DIR, { baseDir: BaseDirectory.AppData })).map(
      (e) => ({ name: e.name ?? "", isFile: e.isFile === true }),
    );
  } catch (_) {
    return entries;
  }
  for (const item of dir) {
    if (!item.isFile || !item.name) continue;
    const p = `${CACHE_DIR}/${item.name}`;
    try {
      const info = await stat(p, { baseDir: BaseDirectory.AppData });
      // FileInfo exposes mtime as a Date | null.
      const mtimeMs = info.mtime ? info.mtime.getTime() : 0;
      entries.push({ path: p, size: info.size, mtimeMs });
    } catch (_) {
      // Best-effort enumeration; skip files we can't stat.
    }
  }
  return entries;
}

async function pruneCache(): Promise<void> {
  const entries = await listCacheEntries();
  let total = entries.reduce((n, e) => n + e.size, 0);
  if (total <= MAX_CACHE_BYTES) return;

  // Oldest first (smaller mtime = older). Tie-break by path for determinism.
  entries.sort((a, b) => a.mtimeMs - b.mtimeMs || a.path.localeCompare(b.path));

  for (const entry of entries) {
    if (total <= TRIM_CACHE_BYTES) break;
    try {
      await remove(entry.path, { baseDir: BaseDirectory.AppData });
      total -= entry.size;
    } catch (_) {
      // Skip entries we can't remove; continue trimming.
    }
  }
}

/**
 * Trigger a prune at most once per `PRUNE_CHECK_INTERVAL_MS`. Multiple
 * concurrent callers share the same in-flight prune.
 */
function schedulePruneIfDue(): Promise<void> {
  const now = Date.now();
  if (now - lastPruneCheckAt < PRUNE_CHECK_INTERVAL_MS) {
    return prunePromise ?? Promise.resolve();
  }
  lastPruneCheckAt = now;
  if (!prunePromise) {
    prunePromise = pruneCache()
      .catch((e) => {
        console.warn("Image cache prune failed:", e);
      })
      .finally(() => {
        prunePromise = null;
      });
  }
  return prunePromise;
}

async function cacheImage(imageUrl: string): Promise<void> {
  try {
    const res = await fetch(imageUrl);
    if (!res.ok) throw new Error(`Failed to download image: ${res.status}`);
    const buf = await res.arrayBuffer();
    const imageName = imageUrl.substring(imageUrl.lastIndexOf("/") + 1);
    const imagePath = `${CACHE_DIR}/${imageName}`;

    await mkdir(CACHE_DIR, {
      recursive: true,
      baseDir: BaseDirectory.AppData,
    });

    await writeFile(imagePath, new Uint8Array(buf), {
      baseDir: BaseDirectory.AppData,
    });

    // Fire-and-forget pruning so a long-running session can't grow the cache
    // beyond MAX_CACHE_BYTES.
    schedulePruneIfDue();
  } catch (error) {
    console.error("Error caching image:", error);
  }
}

export async function displayCachedImage(imageUrl: string): Promise<string> {
  const imageName = imageUrl.substring(imageUrl.lastIndexOf("/") + 1);
  const imagePath = `${CACHE_DIR}/${imageName}`;

  const imageExists = await exists(imagePath);

  if (imageExists) {
    const imageData = await readFile(imagePath, {
      baseDir: BaseDirectory.AppData,
    });
    const base64 = btoa(String.fromCharCode(...imageData));
    return `data:image/jpg;base64,${base64}`;
  } else {
    await cacheImage(imageUrl);
    return imageUrl;
  }
}

/**
 * Force an immediate cache prune. Exposed for settings / cleanup actions.
 * Resolves once the cache is back under the trim target (or fails silently).
 */
export async function pruneImageCacheNow(): Promise<void> {
  lastPruneCheckAt = Date.now();
  if (!prunePromise) {
    prunePromise = pruneCache().finally(() => {
      prunePromise = null;
    });
  }
  return prunePromise;
}
