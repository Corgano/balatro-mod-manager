/**
 * Map an async function over a list of inputs with bounded concurrency.
 *
 * Behaves like `Promise.allSettled(items.map(task))` but caps the number of
 * promises in flight at any moment to `concurrency`, so a large input does
 * not stampede the backend / Tauri IPC queue. Results are returned in the
 * original input order regardless of completion order.
 */
export async function mapWithConcurrency<T, R>(
  items: T[],
  concurrency: number,
  task: (item: T, index: number) => Promise<R>,
): Promise<PromiseSettledResult<R>[]> {
  if (items.length === 0) return [];
  const limit = Math.max(1, Math.floor(concurrency));
  const results: PromiseSettledResult<R>[] = Array.from({
    length: items.length,
  });
  let cursor = 0;

  const worker = async () => {
    while (true) {
      const idx = cursor++;
      if (idx >= items.length) return;
      try {
        const value = await task(items[idx], idx);
        results[idx] = { status: "fulfilled", value };
      } catch (reason) {
        results[idx] = { status: "rejected", reason };
      }
    }
  };

  const workers = Array.from({ length: Math.min(limit, items.length) }, () =>
    worker(),
  );
  await Promise.all(workers);
  return results;
}
