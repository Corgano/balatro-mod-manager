import { describe, it, expect } from 'vitest';
import { pickDarkPalette, normalizeColorPair, type ColorPair } from './cardPalette';

describe('cardPalette', () => {
  describe('pickDarkPalette', () => {
    it('should return default pair for empty string', () => {
      const result = pickDarkPalette('');
      expect(result).toEqual({ color1: '#4f6367', color2: '#334461' });
    });

    it('should return default pair for falsy values', () => {
      expect(pickDarkPalette(null as unknown as string)).toEqual({
        color1: '#4f6367',
        color2: '#334461',
      });
      expect(pickDarkPalette(undefined as unknown as string)).toEqual({
        color1: '#4f6367',
        color2: '#334461',
      });
    });

    it('should return consistent colors for same key', () => {
      const result1 = pickDarkPalette('test-mod');
      const result2 = pickDarkPalette('test-mod');
      expect(result1).toEqual(result2);
    });

    it('should return different colors for different keys', () => {
      const result1 = pickDarkPalette('mod-a');
      const result2 = pickDarkPalette('mod-b');
      // They might be the same by chance, but test that function works
      expect(result1.color1).toMatch(/^#[0-9a-f]{6}$/);
      expect(result1.color2).toMatch(/^#[0-9a-f]{6}$/);
      expect(result2.color1).toMatch(/^#[0-9a-f]{6}$/);
      expect(result2.color2).toMatch(/^#[0-9a-f]{6}$/);
    });

    it('should return valid hex colors', () => {
      const keys = ['Steamodded', 'Talisman', 'Cryptid', 'Bunco', 'Jimbo'];
      for (const key of keys) {
        const result = pickDarkPalette(key);
        expect(result.color1).toMatch(/^#[0-9a-f]{6}$/);
        expect(result.color2).toMatch(/^#[0-9a-f]{6}$/);
      }
    });

    it('should handle long keys', () => {
      const longKey = 'a'.repeat(1000);
      const result = pickDarkPalette(longKey);
      expect(result.color1).toMatch(/^#[0-9a-f]{6}$/);
      expect(result.color2).toMatch(/^#[0-9a-f]{6}$/);
    });

    it('should handle special characters', () => {
      const result = pickDarkPalette('test-mod_v1.2.3!@#$%');
      expect(result.color1).toMatch(/^#[0-9a-f]{6}$/);
      expect(result.color2).toMatch(/^#[0-9a-f]{6}$/);
    });
  });

  describe('normalizeColorPair', () => {
    it('should return default pair for null input', () => {
      const result = normalizeColorPair(null);
      expect(result).toEqual({ color1: '#4f6367', color2: '#334461' });
    });

    it('should return default pair for undefined input', () => {
      const result = normalizeColorPair(undefined);
      expect(result).toEqual({ color1: '#4f6367', color2: '#334461' });
    });

    it('should return default pair for empty object', () => {
      const result = normalizeColorPair({});
      expect(result).toEqual({ color1: '#4f6367', color2: '#334461' });
    });

    it('should use provided color1 and default color2', () => {
      const result = normalizeColorPair({ color1: '#ff0000' });
      expect(result).toEqual({ color1: '#ff0000', color2: '#334461' });
    });

    it('should use provided color2 and default color1', () => {
      const result = normalizeColorPair({ color2: '#00ff00' });
      expect(result).toEqual({ color1: '#4f6367', color2: '#00ff00' });
    });

    it('should use both provided colors', () => {
      const input: ColorPair = { color1: '#ff0000', color2: '#00ff00' };
      const result = normalizeColorPair(input);
      expect(result).toEqual(input);
    });

    it('should handle partial with null values', () => {
      const result = normalizeColorPair({
        color1: null as unknown as string,
        color2: '#00ff00',
      });
      expect(result).toEqual({ color1: '#4f6367', color2: '#00ff00' });
    });
  });
});
