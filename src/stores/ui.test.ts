import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { get } from 'svelte/store';

describe('ui store', () => {
  let mockStorage: Record<string, string>;
  const originalKeys = Object.keys.bind(Object);

  beforeEach(() => {
    mockStorage = {};

    // Mock localStorage
    vi.stubGlobal('localStorage', {
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

    // Mock window for browser detection
    vi.stubGlobal('window', {});

    // Reset modules to pick up fresh localStorage
    vi.resetModules();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('cardScale', () => {
    it('should have default value of 1', async () => {
      const { cardScale } = await import('./ui');
      expect(get(cardScale)).toBe(1);
    });

    it('should load persisted value from localStorage', async () => {
      mockStorage['ui.cardScale'] = '1.2';
      const { cardScale } = await import('./ui');
      expect(get(cardScale)).toBe(1.2);
    });

    it('should clamp to minimum of 0.75', async () => {
      mockStorage['ui.cardScale'] = '0.5';
      const { cardScale } = await import('./ui');
      expect(get(cardScale)).toBe(0.75);
    });

    it('should clamp to maximum of 1.4', async () => {
      mockStorage['ui.cardScale'] = '2.0';
      const { cardScale } = await import('./ui');
      expect(get(cardScale)).toBe(1.4);
    });

    it('should persist changes to localStorage', async () => {
      const { cardScale } = await import('./ui');
      cardScale.set(1.1);
      expect(mockStorage['ui.cardScale']).toBe('1.1');
    });

    it('should handle invalid localStorage value', async () => {
      mockStorage['ui.cardScale'] = 'not-a-number';
      const { cardScale } = await import('./ui');
      // NaN check fails, should use fallback
      expect(get(cardScale)).toBe(1);
    });
  });

  describe('darkMode', () => {
    it('should have default value of false', async () => {
      const { darkMode } = await import('./ui');
      expect(get(darkMode)).toBe(false);
    });

    it('should load persisted true value', async () => {
      mockStorage['ui.darkMode'] = 'true';
      const { darkMode } = await import('./ui');
      expect(get(darkMode)).toBe(true);
    });

    it('should load persisted false value', async () => {
      mockStorage['ui.darkMode'] = 'false';
      const { darkMode } = await import('./ui');
      expect(get(darkMode)).toBe(false);
    });

    it('should persist changes to localStorage', async () => {
      const { darkMode } = await import('./ui');
      darkMode.set(true);
      expect(mockStorage['ui.darkMode']).toBe('true');
    });

    it('should treat non-true string as false', async () => {
      mockStorage['ui.darkMode'] = 'yes';
      const { darkMode } = await import('./ui');
      expect(get(darkMode)).toBe(false);
    });
  });
});
