/**
 * Simple LRU (Least Recently Used) cache with max size.
 *
 * When the cache exceeds maxSize, the least recently accessed entries
 * are evicted. Uses Map's insertion order for efficient O(1) operations.
 */
export class LRUCache<K, V> {
  private cache: Map<K, V>;
  private readonly maxSize: number;

  constructor(maxSize: number) {
    this.cache = new Map();
    this.maxSize = maxSize;
  }

  /**
   * Get a value from the cache.
   * Accessing a key moves it to the "most recently used" position.
   */
  get(key: K): V | undefined {
    if (!this.cache.has(key)) {
      return undefined;
    }
    // Move to end (most recently used) by deleting and re-inserting
    const value = this.cache.get(key)!;
    this.cache.delete(key);
    this.cache.set(key, value);
    return value;
  }

  /**
   * Check if a key exists without affecting LRU order.
   */
  has(key: K): boolean {
    return this.cache.has(key);
  }

  /**
   * Set a value in the cache.
   * If the cache is full, evicts the least recently used entry.
   */
  set(key: K, value: V): void {
    // If key exists, delete it first to update its position
    if (this.cache.has(key)) {
      this.cache.delete(key);
    }
    // Evict oldest entries if at capacity
    while (this.cache.size >= this.maxSize) {
      const oldest = this.cache.keys().next().value;
      if (oldest !== undefined) {
        this.cache.delete(oldest);
      } else {
        break;
      }
    }
    this.cache.set(key, value);
  }

  /**
   * Delete a key from the cache.
   */
  delete(key: K): boolean {
    return this.cache.delete(key);
  }

  /**
   * Clear the entire cache.
   */
  clear(): void {
    this.cache.clear();
  }

  /**
   * Get the current size of the cache.
   */
  get size(): number {
    return this.cache.size;
  }
}
