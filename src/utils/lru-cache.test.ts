import { describe, it, expect, beforeEach } from "vitest";
import { LRUCache } from "./lru-cache";

describe("LRUCache", () => {
  let cache: LRUCache<string, number>;

  beforeEach(() => {
    cache = new LRUCache<string, number>(3);
  });

  describe("basic operations", () => {
    it("should store and retrieve values", () => {
      cache.set("a", 1);
      expect(cache.get("a")).toBe(1);
    });

    it("should return undefined for missing keys", () => {
      expect(cache.get("nonexistent")).toBeUndefined();
    });

    it("should check if key exists with has()", () => {
      cache.set("a", 1);
      expect(cache.has("a")).toBe(true);
      expect(cache.has("b")).toBe(false);
    });

    it("should delete keys", () => {
      cache.set("a", 1);
      expect(cache.delete("a")).toBe(true);
      expect(cache.get("a")).toBeUndefined();
      expect(cache.delete("nonexistent")).toBe(false);
    });

    it("should clear all entries", () => {
      cache.set("a", 1);
      cache.set("b", 2);
      cache.clear();
      expect(cache.size).toBe(0);
      expect(cache.get("a")).toBeUndefined();
    });

    it("should report correct size", () => {
      expect(cache.size).toBe(0);
      cache.set("a", 1);
      expect(cache.size).toBe(1);
      cache.set("b", 2);
      expect(cache.size).toBe(2);
    });
  });

  describe("capacity and eviction", () => {
    it("should not exceed max size", () => {
      cache.set("a", 1);
      cache.set("b", 2);
      cache.set("c", 3);
      cache.set("d", 4); // Should evict 'a'
      expect(cache.size).toBe(3);
    });

    it("should evict least recently used entry", () => {
      cache.set("a", 1);
      cache.set("b", 2);
      cache.set("c", 3);
      cache.set("d", 4); // Should evict 'a' (oldest)
      expect(cache.get("a")).toBeUndefined();
      expect(cache.get("b")).toBe(2);
      expect(cache.get("c")).toBe(3);
      expect(cache.get("d")).toBe(4);
    });

    it("should update LRU order on get()", () => {
      cache.set("a", 1);
      cache.set("b", 2);
      cache.set("c", 3);
      cache.get("a"); // 'a' is now most recently used
      cache.set("d", 4); // Should evict 'b' (now oldest)
      expect(cache.get("a")).toBe(1);
      expect(cache.get("b")).toBeUndefined();
      expect(cache.get("c")).toBe(3);
      expect(cache.get("d")).toBe(4);
    });

    it("should not update LRU order on has()", () => {
      cache.set("a", 1);
      cache.set("b", 2);
      cache.set("c", 3);
      cache.has("a"); // Should NOT affect order
      cache.set("d", 4); // Should still evict 'a'
      expect(cache.get("a")).toBeUndefined();
    });

    it("should update value and LRU order on set() for existing key", () => {
      cache.set("a", 1);
      cache.set("b", 2);
      cache.set("c", 3);
      cache.set("a", 10); // Update 'a', moves to most recent
      cache.set("d", 4); // Should evict 'b'
      expect(cache.get("a")).toBe(10);
      expect(cache.get("b")).toBeUndefined();
    });
  });

  describe("edge cases", () => {
    it("should handle cache with size 1", () => {
      const tinyCache = new LRUCache<string, number>(1);
      tinyCache.set("a", 1);
      expect(tinyCache.get("a")).toBe(1);
      tinyCache.set("b", 2);
      expect(tinyCache.get("a")).toBeUndefined();
      expect(tinyCache.get("b")).toBe(2);
      expect(tinyCache.size).toBe(1);
    });

    it("should handle various key types", () => {
      const numCache = new LRUCache<number, string>(3);
      numCache.set(1, "one");
      numCache.set(2, "two");
      expect(numCache.get(1)).toBe("one");
      expect(numCache.get(2)).toBe("two");
    });

    it("should handle void values", () => {
      const voidCache = new LRUCache<string, void>(3);
      voidCache.set("a", undefined);
      expect(voidCache.has("a")).toBe(true);
      expect(voidCache.get("a")).toBeUndefined();
    });

    it("should handle null values", () => {
      const nullCache = new LRUCache<string, null>(3);
      nullCache.set("a", null);
      expect(nullCache.has("a")).toBe(true);
      expect(nullCache.get("a")).toBeNull();
    });
  });
});
