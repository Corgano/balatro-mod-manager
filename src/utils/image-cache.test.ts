import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock Tauri fs plugin
const mockExists = vi.fn();
const mockReadFile = vi.fn();
const mockWriteFile = vi.fn();
const mockMkdir = vi.fn();

vi.mock('@tauri-apps/plugin-fs', () => ({
  exists: (...args: unknown[]) => mockExists(...args),
  readFile: (...args: unknown[]) => mockReadFile(...args),
  writeFile: (...args: unknown[]) => mockWriteFile(...args),
  mkdir: (...args: unknown[]) => mockMkdir(...args),
}));

vi.mock('@tauri-apps/api/path', () => ({
  BaseDirectory: {
    AppData: 'AppData',
  },
}));

// Mock fetch
const mockFetch = vi.fn();
global.fetch = mockFetch;

// Import after mocking
import { displayCachedImage } from './image-cache';

describe('image-cache', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('displayCachedImage', () => {
    it('should return base64 data URL when image exists in cache', async () => {
      const imageUrl = 'https://example.com/images/test.jpg';
      const mockImageData = new Uint8Array([72, 101, 108, 108, 111]); // "Hello"

      mockExists.mockResolvedValue(true);
      mockReadFile.mockResolvedValue(mockImageData);

      const result = await displayCachedImage(imageUrl);

      expect(mockExists).toHaveBeenCalledWith('cache/test.jpg');
      expect(mockReadFile).toHaveBeenCalledWith('cache/test.jpg', {
        baseDir: 'AppData',
      });
      expect(result).toMatch(/^data:image\/jpg;base64,/);
      // Verify it's valid base64 of "Hello"
      expect(result).toBe('data:image/jpg;base64,SGVsbG8=');
    });

    it('should cache and return original URL when image not in cache', async () => {
      const imageUrl = 'https://example.com/images/new.jpg';
      const mockArrayBuffer = new ArrayBuffer(8);

      mockExists.mockResolvedValue(false);
      mockFetch.mockResolvedValue({
        ok: true,
        arrayBuffer: () => Promise.resolve(mockArrayBuffer),
      });
      mockMkdir.mockResolvedValue(undefined);
      mockWriteFile.mockResolvedValue(undefined);

      const result = await displayCachedImage(imageUrl);

      expect(mockExists).toHaveBeenCalledWith('cache/new.jpg');
      expect(mockFetch).toHaveBeenCalledWith(imageUrl);
      expect(mockMkdir).toHaveBeenCalledWith('cache', {
        recursive: true,
        baseDir: 'AppData',
      });
      expect(mockWriteFile).toHaveBeenCalled();
      expect(result).toBe(imageUrl);
    });

    it('should extract filename from URL correctly', async () => {
      const imageUrl = 'https://cdn.example.com/path/to/image.png';

      mockExists.mockResolvedValue(true);
      mockReadFile.mockResolvedValue(new Uint8Array([1, 2, 3]));

      await displayCachedImage(imageUrl);

      expect(mockExists).toHaveBeenCalledWith('cache/image.png');
    });

    it('should handle URLs with query parameters', async () => {
      // The current implementation doesn't strip query params
      const imageUrl = 'https://example.com/img.jpg?v=123';

      mockExists.mockResolvedValue(true);
      mockReadFile.mockResolvedValue(new Uint8Array([1, 2, 3]));

      await displayCachedImage(imageUrl);

      // Filename includes query string in current implementation
      expect(mockExists).toHaveBeenCalledWith('cache/img.jpg?v=123');
    });

    it('should return original URL when fetch fails', async () => {
      const imageUrl = 'https://example.com/fail.jpg';

      mockExists.mockResolvedValue(false);
      mockFetch.mockResolvedValue({
        ok: false,
        status: 404,
      });

      const result = await displayCachedImage(imageUrl);

      // Should still return the URL even on fetch failure
      expect(result).toBe(imageUrl);
    });

    it('should handle network errors gracefully', async () => {
      const imageUrl = 'https://example.com/error.jpg';

      mockExists.mockResolvedValue(false);
      mockFetch.mockRejectedValue(new Error('Network error'));

      const result = await displayCachedImage(imageUrl);

      expect(result).toBe(imageUrl);
    });
  });
});
