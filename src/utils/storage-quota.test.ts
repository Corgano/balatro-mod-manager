import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  estimateStorageUsage,
  getStorageQuotaState,
  safeLocalStorageSet,
  clearLowPriorityCaches,
  formatBytes,
} from "./storage-quota";

describe("storage-quota", () => {
  let mockStorage: Record<string, string>;
  const originalKeys = Object.keys.bind(Object);

  beforeEach(() => {
    mockStorage = {};

    // Mock localStorage
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => mockStorage[key] ?? null,
      setItem: (key: string, value: string) => {
        mockStorage[key] = value;
      },
      removeItem: (key: string) => {
        delete mockStorage[key];
      },
      clear: () => {
        mockStorage = {};
      },
      key: (index: number) => originalKeys(mockStorage)[index] ?? null,
      get length() {
        return originalKeys(mockStorage).length;
      },
    });

    // Mock Object.keys to work with our mock localStorage
    vi.spyOn(Object, "keys").mockImplementation((obj) => {
      if (obj === localStorage) {
        return originalKeys(mockStorage);
      }
      return originalKeys(obj as object);
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("estimateStorageUsage", () => {
    it("should return 0 for empty storage", () => {
      expect(estimateStorageUsage()).toBe(0);
    });

    it("should calculate bytes correctly (2 bytes per char)", () => {
      mockStorage["key"] = "value";
      // 'key' (3 chars) + 'value' (5 chars) = 8 chars * 2 bytes = 16 bytes
      expect(estimateStorageUsage()).toBe(16);
    });

    it("should sum multiple entries", () => {
      mockStorage["a"] = "1";
      mockStorage["b"] = "2";
      // ('a' + '1') * 2 + ('b' + '2') * 2 = 4 + 4 = 8 bytes
      expect(estimateStorageUsage()).toBe(8);
    });
  });

  describe("getStorageQuotaState", () => {
    it("should return correct state for empty storage", () => {
      const state = getStorageQuotaState();
      expect(state.usedBytes).toBe(0);
      expect(state.quotaBytes).toBe(5 * 1024 * 1024);
      expect(state.percentUsed).toBe(0);
      expect(state.isNearLimit).toBe(false);
      expect(state.isExceeded).toBe(false);
    });

    it("should detect near limit state", () => {
      // Set storage to ~85% of 5MB quota
      const largeValue = "x".repeat(2 * 1024 * 1024); // ~4MB with UTF-16
      mockStorage["big"] = largeValue;

      const state = getStorageQuotaState();
      expect(state.isNearLimit).toBe(true);
      expect(state.isExceeded).toBe(false);
    });
  });

  describe("safeLocalStorageSet", () => {
    it("should set value and return true on success", () => {
      const result = safeLocalStorageSet("key", "value");
      expect(result).toBe(true);
      expect(mockStorage["key"]).toBe("value");
    });

    it("should return false on QuotaExceededError", () => {
      const error = new DOMException("Quota exceeded", "QuotaExceededError");
      vi.spyOn(localStorage, "setItem").mockImplementation(() => {
        throw error;
      });

      const result = safeLocalStorageSet("key", "value");
      expect(result).toBe(false);
    });

    it("should rethrow non-quota errors", () => {
      vi.spyOn(localStorage, "setItem").mockImplementation(() => {
        throw new Error("Some other error");
      });

      expect(() => safeLocalStorageSet("key", "value")).toThrow(
        "Some other error",
      );
    });
  });

  describe("clearLowPriorityCaches", () => {
    it("should clear thumbnails-cache and descriptions-cache", () => {
      mockStorage["thumbnails-cache"] = "data1";
      mockStorage["descriptions-cache"] = "data2";
      mockStorage["important-data"] = "keep";

      const freed = clearLowPriorityCaches();

      expect(freed).toBeGreaterThan(0);
      expect(mockStorage["thumbnails-cache"]).toBeUndefined();
      expect(mockStorage["descriptions-cache"]).toBeUndefined();
      expect(mockStorage["important-data"]).toBe("keep");
    });

    it("should return 0 if no low-priority caches exist", () => {
      mockStorage["important-data"] = "keep";
      const freed = clearLowPriorityCaches();
      expect(freed).toBe(0);
    });
  });

  describe("formatBytes", () => {
    it("should format bytes", () => {
      expect(formatBytes(0)).toBe("0 B");
      expect(formatBytes(512)).toBe("512 B");
      expect(formatBytes(1023)).toBe("1023 B");
    });

    it("should format kilobytes", () => {
      expect(formatBytes(1024)).toBe("1.0 KB");
      expect(formatBytes(1536)).toBe("1.5 KB");
      expect(formatBytes(10240)).toBe("10.0 KB");
    });

    it("should format megabytes", () => {
      expect(formatBytes(1024 * 1024)).toBe("1.00 MB");
      expect(formatBytes(5 * 1024 * 1024)).toBe("5.00 MB");
      expect(formatBytes(1.5 * 1024 * 1024)).toBe("1.50 MB");
    });
  });
});
