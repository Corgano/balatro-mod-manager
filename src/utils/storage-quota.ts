/**
 * LocalStorage quota monitoring utilities.
 *
 * Provides proactive quota monitoring to prevent silent failures
 * when localStorage is full.
 */

/** Storage quota state */
interface StorageQuotaState {
  /** Estimated bytes used */
  usedBytes: number;
  /** Estimated total quota (browser-dependent, ~5-10MB typically) */
  quotaBytes: number;
  /** Percentage used (0-100) */
  percentUsed: number;
  /** Whether we're approaching the limit */
  isNearLimit: boolean;
  /** Whether quota is exceeded */
  isExceeded: boolean;
}

/** Warning threshold (80% of quota) */
const WARNING_THRESHOLD = 0.8;

/** Estimated quota (5MB is conservative, most browsers allow more) */
const ESTIMATED_QUOTA_BYTES = 5 * 1024 * 1024;

/**
 * Estimate current localStorage usage.
 * This is approximate since localStorage stores strings.
 */
export function estimateStorageUsage(): number {
  if (typeof localStorage === "undefined") return 0;

  let totalBytes = 0;
  try {
    for (const key of Object.keys(localStorage)) {
      const value = localStorage.getItem(key);
      if (value) {
        // Each character is 2 bytes in JavaScript strings (UTF-16)
        totalBytes += (key.length + value.length) * 2;
      }
    }
  } catch {
    // Access error, return 0
    return 0;
  }
  return totalBytes;
}

/**
 * Get storage quota state including usage estimates.
 */
export function getStorageQuotaState(): StorageQuotaState {
  const usedBytes = estimateStorageUsage();
  const quotaBytes = ESTIMATED_QUOTA_BYTES;
  const percentUsed = (usedBytes / quotaBytes) * 100;

  return {
    usedBytes,
    quotaBytes,
    percentUsed,
    isNearLimit: percentUsed > WARNING_THRESHOLD * 100,
    isExceeded: percentUsed >= 100,
  };
}

/**
 * Safely write to localStorage with quota awareness.
 * Returns true if write succeeded, false if quota exceeded.
 */
export function safeLocalStorageSet(key: string, value: string): boolean {
  if (typeof localStorage === "undefined") return false;

  try {
    localStorage.setItem(key, value);
    return true;
  } catch (e) {
    // QuotaExceededError
    if (
      e instanceof DOMException &&
      (e.name === "QuotaExceededError" || e.code === 22)
    ) {
      console.warn(`LocalStorage quota exceeded when writing key: ${key}`);
      return false;
    }
    // Re-throw other errors
    throw e;
  }
}

/**
 * Clear low-priority cached data to free up space.
 * Returns bytes freed (approximate).
 */
export function clearLowPriorityCaches(): number {
  if (typeof localStorage === "undefined") return 0;

  const lowPriorityKeys = [
    "thumbnails-cache", // Thumbnails can be re-fetched
    "descriptions-cache", // Descriptions can be re-fetched
  ];

  let freedBytes = 0;

  for (const key of lowPriorityKeys) {
    const value = localStorage.getItem(key);
    if (value) {
      freedBytes += (key.length + value.length) * 2;
      try {
        localStorage.removeItem(key);
      } catch {
        // Ignore removal errors
      }
    }
  }

  console.info(
    `Cleared ${Math.round(freedBytes / 1024)}KB from low-priority caches`,
  );
  return freedBytes;
}

/**
 * Monitor storage quota and take action if needed.
 * Call this periodically (e.g., on startup) to proactively manage storage.
 */
export function monitorStorageQuota(): StorageQuotaState {
  const state = getStorageQuotaState();

  if (state.isExceeded) {
    console.warn(
      `LocalStorage quota exceeded (${Math.round(state.percentUsed)}% used). Clearing caches...`,
    );
    clearLowPriorityCaches();
  } else if (state.isNearLimit) {
    console.info(
      `LocalStorage approaching limit (${Math.round(state.percentUsed)}% used)`,
    );
  }

  return state;
}

/**
 * Format bytes as human-readable string.
 */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}
