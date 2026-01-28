import { writable, derived, get } from "svelte/store";

// Network status store
export const isOnline = writable(
  typeof navigator !== "undefined" ? navigator.onLine : true,
);

// Track if we've shown the offline notification
let offlineNotificationShown = false;

// Initialize network listeners
if (typeof window !== "undefined") {
  window.addEventListener("online", () => {
    isOnline.set(true);
    offlineNotificationShown = false;
  });

  window.addEventListener("offline", () => {
    isOnline.set(false);
  });
}

// Derived store for UI messaging
export const networkStatus = derived(isOnline, ($isOnline) => ({
  online: $isOnline,
  message: $isOnline ? null : "You are offline. Showing cached data.",
}));

// Check if we should show offline notification (only once per offline session)
export function shouldShowOfflineNotification(): boolean {
  if (!get(isOnline) && !offlineNotificationShown) {
    offlineNotificationShown = true;
    return true;
  }
  return false;
}

// Utility to wrap async operations with offline handling
export async function withOfflineHandling<T>(
  operation: () => Promise<T>,
  fallback: T,
  _options?: { silent?: boolean },
): Promise<T> {
  if (!get(isOnline)) {
    return fallback;
  }

  try {
    return await operation();
  } catch (error) {
    // Check if error is network-related
    if (isNetworkError(error)) {
      isOnline.set(false);
      return fallback;
    }
    throw error;
  }
}

// Detect network-related errors
export function isNetworkError(error: unknown): boolean {
  if (error instanceof Error) {
    const message = error.message.toLowerCase();
    return (
      message.includes("network") ||
      message.includes("fetch") ||
      message.includes("failed to fetch") ||
      message.includes("networkerror") ||
      message.includes("connection") ||
      message.includes("offline") ||
      message.includes("timeout") ||
      message.includes("econnrefused") ||
      message.includes("enotfound")
    );
  }
  return false;
}
