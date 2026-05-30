import uFuzzy from "@leeoniya/ufuzzy";

// Shared fuzzy matcher for all mod search/filter UIs.
// intraMode 1 enables single-error tolerance (one insert/sub/del/transpose per
// term), so "jimbo" still finds "Jimbio" and "crdpk" loosely finds "Cardpack".
// uFuzzy matches case-insensitively by default.
const uf = new uFuzzy({
  intraMode: 1,
  intraIns: 1,
  intraSub: 1,
  intraTrn: 1,
  intraDel: 1,
});

/**
 * Fuzzy-rank a haystack against a needle.
 * Returns indices into `haystack`, best match first.
 * Empty needle returns all indices in original order.
 */
export function fuzzySearch(haystack: string[], needle: string): number[] {
  const query = needle.trim();
  if (!query) return haystack.map((_, i) => i);

  const idxs = uf.filter(haystack, query);
  if (!idxs || idxs.length === 0) return [];

  const info = uf.info(idxs, haystack, query);
  const order = uf.sort(info, haystack, query);
  return order.map((o) => info.idx[o]);
}

/**
 * Fuzzy-filter a list of items, ranked best-first.
 * `toText` builds the searchable string for each item.
 * Empty needle returns the list unchanged (original order).
 */
export function fuzzyFilter<T>(
  items: T[],
  needle: string,
  toText: (item: T) => string,
): T[] {
  const query = needle.trim();
  if (!query) return items;

  const haystack = items.map(toText);
  return fuzzySearch(haystack, query).map((i) => items[i]);
}
