/**
 * Shared singletons for `<LazyImage>` instances.
 *
 * Each card used to create its own LRU caches, its own
 * IntersectionObserver, and its own concurrency limiter. With 500+ cards
 * on a populous tab (e.g. Miscellaneous on Linux/webkit2gtk) the per-card
 * state added up to a multi-hundred-MB RSS spike that looked like a leak.
 * Hoisting everything into module scope makes the cost O(1) regardless of
 * how many cards are mounted.
 */
import { LRUCache } from "./lru-cache";

/** Caches the asset:// URL we computed for a given mod title's thumbnail. */
export const cacheUrlMemo = new LRUCache<string, string | null>(500);

/** Caches the raw cached-thumbnail path returned by Tauri for a title. */
export const thumbMemo = new LRUCache<string, string>(500);

/**
 * Global concurrent-load gate. `MAX_CONCURRENT_LOADS` is read once at
 * module load via `setMaxConcurrentLoads` from the consumer; default keeps
 * legacy behaviour for non-Linux.
 */
let MAX_CONCURRENT_LOADS = 6;
let inflight = 0;
const waiters: Array<() => void> = [];

export function setMaxConcurrentLoads(n: number): void {
  const next = Math.max(1, Math.floor(n));
  if (next === MAX_CONCURRENT_LOADS) return;
  MAX_CONCURRENT_LOADS = next;
  // Wake any pending waiters whose slot is now available.
  while (inflight < MAX_CONCURRENT_LOADS && waiters.length > 0) {
    const next = waiters.shift();
    next?.();
  }
}

/**
 * Acquire one load slot. Resolves with a release function the caller MUST
 * invoke when their load finishes (success, failure, or component
 * teardown).
 */
export function acquireLoadSlot(): Promise<() => void> {
  return new Promise((resolve) => {
    const grant = () => {
      inflight = Math.max(0, inflight) + 1;
      let released = false;
      resolve(() => {
        if (released) return;
        released = true;
        inflight = Math.max(0, inflight - 1);
        const next = waiters.shift();
        next?.();
      });
    };
    if (inflight < MAX_CONCURRENT_LOADS) {
      grant();
    } else {
      waiters.push(grant);
    }
  });
}

/**
 * Shared IntersectionObserver. One instance per (root, rootMargin,
 * threshold) tuple covers every card on the page. Replaces the
 * previous "one observer per `<LazyImage>`" pattern that retained DOM
 * targets in webkit2gtk for the lifetime of the observer.
 */
type IntersectCallback = (entry: IntersectionObserverEntry) => void;
const callbacks = new WeakMap<Element, IntersectCallback>();
const observerPool = new Map<string, IntersectionObserver>();

function poolKey(rootMargin: string, threshold: number): string {
  return `${rootMargin}|${threshold}`;
}

function getPooledObserver(
  rootMargin: string,
  threshold: number,
): IntersectionObserver | null {
  if (typeof IntersectionObserver === "undefined") return null;
  const key = poolKey(rootMargin, threshold);
  let observer = observerPool.get(key);
  if (!observer) {
    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const cb = callbacks.get(entry.target);
          if (cb) cb(entry);
        }
      },
      { root: null, rootMargin, threshold },
    );
    observerPool.set(key, observer);
  }
  return observer;
}

/**
 * Observe `target` and invoke `cb` whenever its intersection state
 * changes. Returns an unobserve function the caller MUST call on
 * component teardown to avoid leaking the DOM reference.
 */
export function observeIntersection(
  target: Element,
  cb: IntersectCallback,
  options: { rootMargin?: string; threshold?: number } = {},
): () => void {
  const rootMargin = options.rootMargin ?? "0px";
  const threshold = options.threshold ?? 0.01;
  const observer = getPooledObserver(rootMargin, threshold);
  if (!observer) {
    return () => {};
  }
  callbacks.set(target, cb);
  observer.observe(target);
  return () => {
    callbacks.delete(target);
    try {
      observer.unobserve(target);
    } catch (_) {
      /* ignore */
    }
  };
}
