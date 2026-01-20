/**
 * Shared ResizeObserver pool to avoid creating hundreds of observers.
 *
 * Instead of each component creating its own ResizeObserver, we use a single
 * shared observer that watches all registered elements and calls their callbacks.
 * This reduces DOM overhead significantly when rendering many cards.
 */

type ResizeCallback = (_entry: ResizeObserverEntry) => void;

// Map from element to its callback
const callbacks = new Map<Element, ResizeCallback>();

// Single shared observer instance
let sharedObserver: ResizeObserver | null = null;

function getSharedObserver(): ResizeObserver {
  if (!sharedObserver && typeof ResizeObserver !== "undefined") {
    sharedObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const callback = callbacks.get(entry.target);
        if (callback) {
          callback(entry);
        }
      }
    });
  }
  return sharedObserver!;
}

/**
 * Register an element to be observed for resize events.
 * Returns a cleanup function to unregister.
 */
export function observeResize(
  element: Element,
  callback: ResizeCallback,
): () => void {
  if (typeof ResizeObserver === "undefined") {
    return () => {};
  }

  const observer = getSharedObserver();
  callbacks.set(element, callback);
  observer.observe(element);

  return () => {
    callbacks.delete(element);
    observer.unobserve(element);
  };
}

/**
 * Cleanup the shared observer (for testing or hot-reload).
 */
export function destroySharedObserver(): void {
  if (sharedObserver) {
    sharedObserver.disconnect();
    sharedObserver = null;
  }
  callbacks.clear();
}
