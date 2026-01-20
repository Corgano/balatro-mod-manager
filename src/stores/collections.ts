import { writable, type Writable, get } from "svelte/store";

export type ModCollection = {
  id: string;
  name: string;
  modTitles: string[];
  modIds: string[];
  createdAt: number;
  updatedAt: number;
};

const COLLECTIONS_KEY = "mods-collections";
const ACTIVE_COLLECTION_KEY = "mods-collections-active";
const COLLECTION_SHARE_PREFIX = "BMMCOLL1:";

/** Helper to check if a collection is currently active */
export function isCollectionActive(id: string): boolean {
  return get(activeCollectionIds).includes(id);
}

function generateId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `col_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
}

export const collectionsStore: Writable<ModCollection[]> = writable([]);
export const activeCollectionIds = writable<string[]>([]);
export const lastImportedCollectionId = writable<string | null>(null);

/**
 * Snapshot of mod enabled/disabled states taken before the first collection was activated.
 * Used to restore the previous state when all collections are deactivated.
 * Key: mod name (as returned by enabled_state_map), Value: was enabled before collections
 */
export const preCollectionSnapshot = writable<Record<string, boolean> | null>(
  null,
);

export const collectionImportStore = writable<{
  open: boolean;
  code: string;
}>({ open: false, code: "" });

export const collectionPickerStore = writable<{
  open: boolean;
  modTitle: string | null;
  modId: string | null;
}>({ open: false, modTitle: null, modId: null });

export function openCollectionPicker(modTitle: string, modId?: string | null) {
  collectionPickerStore.set({ open: true, modTitle, modId: modId ?? null });
}

export function closeCollectionPicker() {
  collectionPickerStore.set({ open: false, modTitle: null, modId: null });
}

export function openCollectionImport(code = "") {
  collectionImportStore.set({ open: true, code });
}

export function closeCollectionImport() {
  collectionImportStore.set({ open: false, code: "" });
}

export function setCollectionImportCode(code: string) {
  collectionImportStore.update((state) => ({ ...state, code }));
}

function normalizeName(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "");
}

/** Check if a mod title matches any title in the collection using normalized comparison */
export function isModInCollection(
  collection: ModCollection,
  modTitle: string,
): boolean {
  const normalizedInput = normalizeName(modTitle);
  return collection.modTitles.some((t) => normalizeName(t) === normalizedInput);
}

/** Find the original title in the collection that matches the given title (normalized) */
function findMatchingTitle(
  modTitles: string[],
  modTitle: string,
): string | null {
  const normalizedInput = normalizeName(modTitle);
  return modTitles.find((t) => normalizeName(t) === normalizedInput) ?? null;
}

function makeUniqueName(base: string, existing: Set<string>): string {
  let name = base;
  let counter = 1;
  while (existing.has(normalizeName(name))) {
    name = `${base} (${counter})`;
    counter += 1;
  }
  return name;
}

function encodeSharePayload(payload: unknown): string {
  const json = JSON.stringify(payload);
  const base64 = btoa(unescape(encodeURIComponent(json)));
  return `${COLLECTION_SHARE_PREFIX}${base64}`;
}

function decodeSharePayload(code: string): unknown {
  const trimmed = code.trim();
  const raw = trimmed.startsWith(COLLECTION_SHARE_PREFIX)
    ? trimmed.slice(COLLECTION_SHARE_PREFIX.length)
    : trimmed;
  const json = decodeURIComponent(escape(atob(raw)));
  return JSON.parse(json);
}

export function createCollection(name: string): {
  ok: boolean;
  id?: string;
  error?: string;
} {
  const trimmed = name.trim();
  if (!trimmed) {
    return { ok: false, error: "Collection name can't be empty." };
  }
  let newId = "";
  collectionsStore.update((list) => {
    const exists = list.some(
      (c) => normalizeName(c.name) === normalizeName(trimmed),
    );
    if (exists) {
      return list;
    }
    newId = generateId();
    const now = Date.now();
    return [
      ...list,
      {
        id: newId,
        name: trimmed,
        modTitles: [],
        modIds: [],
        createdAt: now,
        updatedAt: now,
      },
    ];
  });
  if (!newId) {
    return { ok: false, error: "A collection with that name already exists." };
  }
  return { ok: true, id: newId };
}

export function renameCollection(
  id: string,
  name: string,
): { ok: boolean; error?: string } {
  const trimmed = name.trim();
  if (!trimmed) {
    return { ok: false, error: "Collection name can't be empty." };
  }
  let didRename = false;
  collectionsStore.update((list) => {
    const exists = list.some(
      (c) => c.id !== id && normalizeName(c.name) === normalizeName(trimmed),
    );
    if (exists) return list;
    didRename = true;
    return list.map((c) =>
      c.id === id ? { ...c, name: trimmed, updatedAt: Date.now() } : c,
    );
  });
  if (!didRename) {
    return { ok: false, error: "A collection with that name already exists." };
  }
  return { ok: true };
}

export function deleteCollection(id: string) {
  collectionsStore.update((list) => list.filter((c) => c.id !== id));
  activeCollectionIds.update((ids) =>
    ids.filter((activeId) => activeId !== id),
  );
}

export function toggleModInCollection(
  id: string,
  modTitle: string,
  modId?: string | null,
) {
  collectionsStore.update((list) =>
    list.map((c) => {
      if (c.id !== id) return c;
      const idValue = modId ?? "";
      // Use normalized matching to find existing entry
      const existingTitle = findMatchingTitle(c.modTitles, modTitle);
      // Also check for matching ID with normalization
      const existingId = idValue
        ? c.modIds.find((i) => normalizeName(i) === normalizeName(idValue))
        : null;
      // Check if mod exists by either title OR id match
      const has = existingTitle !== null || existingId !== null;
      const nextTitles = has
        ? existingTitle
          ? c.modTitles.filter((t) => t !== existingTitle)
          : c.modTitles
        : [...c.modTitles, modTitle];
      const nextIds = idValue
        ? has
          ? existingId
            ? c.modIds.filter((t) => t !== existingId)
            : c.modIds
          : c.modIds.includes(idValue)
            ? c.modIds
            : [...c.modIds, idValue]
        : c.modIds;
      return {
        ...c,
        modTitles: nextTitles,
        modIds: nextIds,
        updatedAt: Date.now(),
      };
    }),
  );
}

export function setModInCollection(
  id: string,
  modTitle: string,
  enabled: boolean,
  modId?: string | null,
) {
  collectionsStore.update((list) =>
    list.map((c) => {
      if (c.id !== id) return c;
      const idValue = modId ?? "";
      // Use normalized matching to find existing entry
      const existingTitle = findMatchingTitle(c.modTitles, modTitle);
      const existingId = idValue
        ? c.modIds.find((i) => normalizeName(i) === normalizeName(idValue))
        : null;
      // Check if mod exists by either title OR id match
      const has = existingTitle !== null || existingId !== null;
      if (enabled) {
        const nextIds = idValue
          ? existingId || c.modIds.includes(idValue)
            ? c.modIds
            : [...c.modIds, idValue]
          : c.modIds;
        if (!has) {
          return {
            ...c,
            modTitles: [...c.modTitles, modTitle],
            modIds: nextIds,
            updatedAt: Date.now(),
          };
        }
        if (nextIds !== c.modIds) {
          return { ...c, modIds: nextIds, updatedAt: Date.now() };
        }
      }
      if (!enabled && has) {
        const nextTitles = existingTitle
          ? c.modTitles.filter((t) => t !== existingTitle)
          : c.modTitles;
        const nextIds = existingId
          ? c.modIds.filter((t) => t !== existingId)
          : c.modIds;
        return {
          ...c,
          modTitles: nextTitles,
          modIds: nextIds,
          updatedAt: Date.now(),
        };
      }
      return c;
    }),
  );
}

export function setActiveCollection(id: string | null) {
  if (id === null) {
    activeCollectionIds.set([]);
  } else {
    activeCollectionIds.update((ids) =>
      ids.includes(id) ? ids : [...ids, id],
    );
  }
}

export function addActiveCollection(id: string) {
  activeCollectionIds.update((ids) => (ids.includes(id) ? ids : [...ids, id]));
}

export function removeActiveCollection(id: string) {
  activeCollectionIds.update((ids) =>
    ids.filter((activeId) => activeId !== id),
  );
}

/** Get all mod titles that should remain enabled based on all active collections except the given one */
export function getModsFromOtherActiveCollections(
  excludeId: string,
): Set<string> {
  const collections = get(collectionsStore);
  const activeIds = get(activeCollectionIds);
  const otherActiveIds = activeIds.filter((id) => id !== excludeId);

  const modTitles = new Set<string>();
  for (const id of otherActiveIds) {
    const collection = collections.find((c) => c.id === id);
    if (collection) {
      for (const title of collection.modTitles) {
        modTitles.add(title);
      }
    }
  }
  return modTitles;
}

/** Check if we currently have a pre-collection snapshot saved */
export function hasPreCollectionSnapshot(): boolean {
  return get(preCollectionSnapshot) !== null;
}

/** Save a snapshot of current mod states (call when activating first collection) */
export function savePreCollectionSnapshot(
  enabledMap: Record<string, boolean>,
): void {
  preCollectionSnapshot.set({ ...enabledMap });
}

/** Get and clear the pre-collection snapshot (call when deactivating last collection) */
export function popPreCollectionSnapshot(): Record<string, boolean> | null {
  const snapshot = get(preCollectionSnapshot);
  preCollectionSnapshot.set(null);
  return snapshot;
}

/** Clear the snapshot without returning it */
export function clearPreCollectionSnapshot(): void {
  preCollectionSnapshot.set(null);
}

export function exportCollectionCode(id: string): {
  ok: boolean;
  code?: string;
  error?: string;
} {
  const collection = get(collectionsStore).find((c) => c.id === id) ?? null;
  if (!collection) {
    return { ok: false, error: "Collection not found." };
  }
  const payload = {
    v: 1,
    name: collection.name,
    modIds: collection.modIds,
    modTitles: collection.modTitles,
  };
  return { ok: true, code: encodeSharePayload(payload) };
}

export function importCollectionCode(code: string): {
  ok: boolean;
  id?: string;
  error?: string;
} {
  if (!code.trim()) {
    return { ok: false, error: "Paste a collection code first." };
  }
  let payload: { name?: unknown; modTitles?: unknown; modIds?: unknown };
  try {
    payload = decodeSharePayload(code) as typeof payload;
  } catch {
    return { ok: false, error: "Invalid collection code." };
  }
  const name =
    typeof payload?.name === "string" && payload.name.trim()
      ? payload.name.trim()
      : "Imported Collection";
  const modTitles = Array.isArray(payload?.modTitles)
    ? (payload.modTitles as unknown[])
        .filter((t): t is string => typeof t === "string")
        .map((t) => t.trim())
    : [];
  const modIds = Array.isArray(payload?.modIds)
    ? (payload.modIds as unknown[])
        .filter((t): t is string => typeof t === "string")
        .map((t) => t.trim())
    : [];
  const deduped: string[] = Array.from(new Set(modTitles.filter(Boolean)));
  const dedupedIds: string[] = Array.from(new Set(modIds.filter(Boolean)));

  let newId = "";
  collectionsStore.update((list) => {
    const existing = new Set(list.map((c) => normalizeName(c.name)));
    const uniqueName = makeUniqueName(name, existing);
    newId = generateId();
    const now = Date.now();
    return [
      ...list,
      {
        id: newId,
        name: uniqueName,
        modTitles: deduped,
        modIds: dedupedIds,
        createdAt: now,
        updatedAt: now,
      },
    ];
  });
  if (!newId) {
    return { ok: false, error: "Failed to import collection." };
  }
  lastImportedCollectionId.set(newId);
  return { ok: true, id: newId };
}

if (typeof window !== "undefined") {
  try {
    const raw = window.localStorage.getItem(COLLECTIONS_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        const hydrated = parsed.map((item) => ({
          ...item,
          modIds: Array.isArray(item.modIds) ? item.modIds : [],
        }));
        collectionsStore.set(hydrated);
      }
    }
    const active = window.localStorage.getItem(ACTIVE_COLLECTION_KEY);
    if (active) {
      // Migration: old format was a single string ID, new format is JSON array
      try {
        const parsed = JSON.parse(active);
        if (Array.isArray(parsed)) {
          activeCollectionIds.set(parsed);
        } else if (typeof parsed === "string") {
          activeCollectionIds.set([parsed]);
        }
      } catch {
        // Old format: plain string (not JSON)
        activeCollectionIds.set([active]);
      }
    }
  } catch {
    // ignore malformed cache
  }

  collectionsStore.subscribe((val) => {
    try {
      window.localStorage.setItem(COLLECTIONS_KEY, JSON.stringify(val));
    } catch {
      // ignore quota errors
    }
  });

  activeCollectionIds.subscribe((val) => {
    try {
      if (val.length > 0) {
        window.localStorage.setItem(ACTIVE_COLLECTION_KEY, JSON.stringify(val));
      } else {
        window.localStorage.removeItem(ACTIVE_COLLECTION_KEY);
      }
    } catch {
      // ignore
    }
  });
}
