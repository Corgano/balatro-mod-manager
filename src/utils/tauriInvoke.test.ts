import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock the Tauri invoke function
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Import after mocking
import { invokeTyped, invokeWithTimeout } from "./tauriInvoke";

describe("tauriInvoke", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("invokeTyped", () => {
    it("should call invoke with command name", async () => {
      mockInvoke.mockResolvedValue(["/path/to/balatro"]);

      const promise = invokeTyped("find_steam_balatro");
      await vi.runAllTimersAsync();
      const result = await promise;

      expect(mockInvoke).toHaveBeenCalledWith("find_steam_balatro", undefined);
      expect(result).toEqual(["/path/to/balatro"]);
    });

    it("should call invoke with command and args", async () => {
      mockInvoke.mockResolvedValue(true);

      const promise = invokeTyped("check_custom_balatro", {
        path: "/custom/path",
      });
      await vi.runAllTimersAsync();
      const result = await promise;

      expect(mockInvoke).toHaveBeenCalledWith("check_custom_balatro", {
        path: "/custom/path",
      });
      expect(result).toBe(true);
    });

    it("should propagate errors from invoke", async () => {
      mockInvoke.mockRejectedValue(new Error("Command failed"));

      await expect(invokeTyped("find_steam_balatro")).rejects.toThrow(
        "Command failed",
      );
    });
  });

  describe("invokeWithTimeout", () => {
    it("should resolve if invoke completes before timeout", async () => {
      mockInvoke.mockResolvedValue("result");

      const promise = invokeWithTimeout(
        "check_existing_installation",
        undefined,
        5000,
      );
      await vi.runAllTimersAsync();
      const result = await promise;

      expect(result).toBe("result");
    });

    it("should reject with timeout error if invoke takes too long", async () => {
      // Never resolve the mock
      mockInvoke.mockImplementation(() => new Promise(() => {}));

      const promise = invokeWithTimeout("find_steam_balatro", undefined, 1000);

      // Advance past the timeout
      vi.advanceTimersByTime(1001);

      await expect(promise).rejects.toThrow(
        "invoke-timeout:find_steam_balatro",
      );
    });

    it("should use default timeout of 5000ms", async () => {
      mockInvoke.mockImplementation(() => new Promise(() => {}));

      const promise = invokeWithTimeout("find_steam_balatro");

      // Should not reject at 4999ms
      vi.advanceTimersByTime(4999);

      // Advance past 5000ms
      vi.advanceTimersByTime(2);

      await expect(promise).rejects.toThrow(
        "invoke-timeout:find_steam_balatro",
      );
    });

    it("should clear timeout on successful resolve", async () => {
      mockInvoke.mockResolvedValue("success");

      const promise = invokeWithTimeout(
        "check_existing_installation",
        undefined,
        5000,
      );
      await vi.runAllTimersAsync();
      await promise;

      // Verify no pending timers
      expect(vi.getTimerCount()).toBe(0);
    });

    it("should clear timeout on rejection", async () => {
      mockInvoke.mockRejectedValue(new Error("invoke error"));

      const promise = invokeWithTimeout("find_steam_balatro", undefined, 5000);

      // Catch the rejection to prevent unhandled rejection
      await expect(promise).rejects.toThrow("invoke error");

      // Run any remaining timers
      await vi.runAllTimersAsync();
      expect(vi.getTimerCount()).toBe(0);
    });

    it("should pass args to invoke", async () => {
      mockInvoke.mockResolvedValue(true);

      const promise = invokeWithTimeout(
        "check_custom_balatro",
        { path: "/test" },
        3000,
      );
      await vi.runAllTimersAsync();
      await promise;

      expect(mockInvoke).toHaveBeenCalledWith("check_custom_balatro", {
        path: "/test",
      });
    });
  });
});
