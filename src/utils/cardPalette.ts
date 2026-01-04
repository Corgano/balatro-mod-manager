export type ColorPair = { color1: string; color2: string };

const defaultPair: ColorPair = { color1: "#4f6367", color2: "#334461" };

const darkPalette: ColorPair[] = [
  { color1: "#262b31", color2: "#1b2026" }, // dark gray
  { color1: "#2a2f36", color2: "#1f242a" }, // dark gray
  { color1: "#2f353d", color2: "#232a31" }, // dark gray
  { color1: "#343b44", color2: "#262c33" }, // gray
  { color1: "#3c434d", color2: "#2d343d" }, // gray
  { color1: "#3f4651", color2: "#2f3741" }, // gray
  { color1: "#1b2c44", color2: "#132238" }, // dark blue
  { color1: "#1d3149", color2: "#15263d" }, // dark blue
  { color1: "#1f354f", color2: "#172a41" }, // dark blue
  { color1: "#223a5a", color2: "#1a2f4b" }, // blue
  { color1: "#243f62", color2: "#1c3450" }, // blue
  { color1: "#28466f", color2: "#203958" }, // blue
  { color1: "#2b4b7d", color2: "#213a63" }, // blue
  { color1: "#2d507f", color2: "#234066" }, // blue
  { color1: "#294c73", color2: "#1f3d5f" }, // blue
  { color1: "#295a8f", color2: "#1f476f" }, // blue
  { color1: "#2f5b93", color2: "#234578" }, // bright blue
  { color1: "#3466ab", color2: "#27508f" }, // bright blue
  { color1: "#3a6fc0", color2: "#2b59a1" }, // bright blue
  { color1: "#3f78cf", color2: "#2f61ad" }, // bright blue
  { color1: "#3c4b5b", color2: "#2d3946" }, // gray-blue
  { color1: "#425165", color2: "#313d4e" }, // gray-blue
  { color1: "#22364d", color2: "#1a2b40" }, // deep blue-gray
  { color1: "#2a4362", color2: "#213652" }, // blue-gray
];

function hashString(key: string): number {
  let hash = 0;
  for (let i = 0; i < key.length; i += 1) {
    hash = (hash * 31 + key.charCodeAt(i)) >>> 0;
  }
  return hash;
}

export function pickDarkPalette(key: string): ColorPair {
  if (!key) return defaultPair;
  const idx = hashString(key) % darkPalette.length;
  return darkPalette[idx];
}

export function normalizeColorPair(
  input?: Partial<ColorPair> | null,
): ColorPair {
  return {
    color1: input?.color1 ?? defaultPair.color1,
    color2: input?.color2 ?? defaultPair.color2,
  };
}
