<script lang="ts">
  import { onMount, onDestroy, createEventDispatcher } from "svelte";
  import { assets } from "$app/paths";
  import {
    cacheUrlMemo,
    thumbMemo,
    acquireLoadSlot,
    setMaxConcurrentLoads,
    observeIntersection,
  } from "../../utils/image-loader-shared";

  export let src: string;
  export let alt: string = "";
  export let fallbackSrc: string | void;
  export let defaultSrc: string = "/images/cover.jpg";
  export let className: string = "";
  // Optional caching by title: when provided, we try to use a cached
  // thumbnail and persist successful remote loads for future sessions.
  export let cacheTitle: string | void;
  export let enableCache: boolean = true;
  export let deferLoad: boolean = false;
  export let hasThumbnail: boolean = true;

  // Emit load/error for parent if needed
  const dispatch = createEventDispatcher();
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { configDir, join } from "@tauri-apps/api/path";

  const ua =
    typeof navigator !== "undefined" ? navigator.userAgent.toLowerCase() : "";
  const platformData =
    typeof document !== "undefined"
      ? document.documentElement.dataset.platform
      : "";
  const isLinux =
    platformData === "linux" ||
    (ua && ua.includes("linux") && !ua.includes("android"));
  // Tell the shared limiter what the current platform's cap should be.
  // Called every component mount, but the setter is idempotent.
  setMaxConcurrentLoads(isLinux ? 2 : 6);
  let releaseCurrent: (() => void) | null = null;

  let wrapper: HTMLDivElement | null = null;
  let currentSrc: string | null = null;
  let triedFallback = false;
  let loading = false;
  let usingDefault = false; // show static cover when thumbnail is missing
  let loaded = false; // only show <img> when real image decoded
  let loadTimer: number | null = null;
  const LOAD_TIMEOUT_MS = 8000;
  let spinnerDelayTimer: number | null = null;
  // Delay spinner so broken/missing thumbnails (often 404 quickly) won't show it
  const SPINNER_DELAY_MS = 700;
  let showSpinner = false;
  let showSpinnerOverlay = false;
  // When we decide to show default for a given src, remember it so we don't retry
  let lockDefaultFor: string | null = null;
  // Avoid duplicate cache writes in a single session (per instance, fine).
  const seenCacheTitles = new Set<string>();
  // One-shot cache recheck timer to update image after background caching
  let cacheRecheckTimer: number | null = null;
  const CACHE_RECHECK_DELAY_MS = 6000;
  // Shared intersection observer pool handles offscreen loading.
  let unobserve: (() => void) | null = null;
  let inView = false;
  let localFileFallback: string | null = null;
  let triedLocalFileFallback = false;

  function releaseSlot() {
    if (releaseCurrent) {
      releaseCurrent();
      releaseCurrent = null;
    }
  }

  function isValidSrc(val: string | void | null): boolean {
    if (!val) return false;
    const s = val.trim();
    if (s.length === 0) return false;
    const isAbsPosix = s.startsWith("/");
    const isAbsWin = s.length > 2 && /[a-z]:[\\/]/i.test(s);
    // Allow common safe schemes and known app asset paths
    return (
      s.startsWith("data:") ||
      s.startsWith("/images/") ||
      s.startsWith("/fonts/") ||
      s.startsWith("/static/") ||
      s.startsWith("asset:") ||
      s.startsWith("http://") ||
      s.startsWith("https://") ||
      s.startsWith("tauri://") ||
      s.startsWith("file://") ||
      isAbsPosix ||
      isAbsWin
    );
  }

  function hasScheme(val: string): boolean {
    return /^[a-z][a-z0-9+.-]*:\/\//i.test(val);
  }

  // Convert absolute file path to asset:// URL using Tauri's native convertFileSrc
  function toAssetUrl(path: string | void | null): string | null {
    if (!path) return null;
    const s = path.trim();
    if (s.length === 0) return null;
    if (hasScheme(s)) return s; // Already a URL

    const isWindowsPath = s.length > 2 && /[a-z]:[\\/]/i.test(s);
    const isUnc = s.startsWith("\\\\");
    const isPosixAbs = s.startsWith("/");

    if (!isWindowsPath && !isUnc && !isPosixAbs) return null;

    // Use Tauri's native convertFileSrc for proper encoding
    return convertFileSrc(s);
  }

  function fileUrlForAbsolute(path: string | void | null): string | null {
    if (!path) return null;
    const s = path.trim();
    if (s.length === 0) return null;
    if (hasScheme(s)) return null;
    const isWindowsPath = s.length > 2 && /[a-z]:[\\/]/i.test(s);
    const isUnc = s.startsWith("\\\\");
    const isPosixAbs = s.startsWith("/");
    if (!isWindowsPath && !isUnc && !isPosixAbs) return null;
    if (isUnc) {
      const normalized = s
        .replace(/^\\\\/, "")
        .replace(/\\\\/g, "/")
        .replace(/\\/g, "/");
      return encodeURI(`file://${normalized}`);
    }
    const normalized = isWindowsPath ? s.replace(/\\/g, "/") : s;
    const prefix = isWindowsPath ? "file:///" : "file://";
    return encodeURI(`${prefix}${normalized}`);
  }

  function resolveLocal(path: string | void | null): string | null {
    if (!path) return null;
    const s = path.trim();
    if (s.length === 0) return null;
    const isAppAsset =
      s.startsWith("/images/") ||
      s.startsWith("/fonts/") ||
      s.startsWith("/static/");
    const isWindowsPath = s.length > 2 && /[a-z]:[\\/]/i.test(s);
    const isPosixAbs =
      s.startsWith("/") && !isAppAsset && !s.startsWith(`${assets}/`);
    const isUnc = s.startsWith("\\\\");
    if (isWindowsPath || isPosixAbs || isUnc) {
      // Use file:// URLs directly - more reliable than asset:// protocol
      return toAssetUrl(s);
    }
    // Remote or data/asset schemes are left as-is
    if (
      s.startsWith("http://") ||
      s.startsWith("https://") ||
      s.startsWith("data:") ||
      s.startsWith("asset:") ||
      s.startsWith("tauri://")
    ) {
      return s;
    }
    // Only allow known app assets; otherwise defer to default
    if (isAppAsset) {
      const normalized = s.startsWith("/") ? s : `/${s}`;
      return `${assets}${normalized}`;
    }
    return null;
  }

  function safeSlug(input: string): string {
    let s = input.trim().toLowerCase();
    s = Array.from(s)
      .map((c) => (/[a-z0-9]/i.test(c) ? c : "-"))
      .join("");
    while (s.includes("--")) s = s.replace("--", "-");
    return s.replace(/^-+/, "").replace(/-+$/, "");
  }

  async function buildCachePaths(title: string | void | null) {
    if (!title || title.trim().length === 0) return { path: null, url: null };
    const key = title.trim();
    if (cacheUrlMemo.has(key)) {
      return { path: null, url: cacheUrlMemo.get(key) ?? null };
    }
    if (key.startsWith("http://") || key.startsWith("https://")) {
      // Not a local cache key
      cacheUrlMemo.set(key, null);
      return { path: null, url: null };
    }
    try {
      const base = await configDir();
      const path = await join(
        base,
        "Balatro",
        "mod_assets",
        "thumbnails",
        `${safeSlug(key)}.jpg`,
      );
      const url = toAssetUrl(path);
      cacheUrlMemo.set(key, url);
      return { path, url };
    } catch (e) {
      cacheUrlMemo.set(key, null);
      return { path: null, url: null };
    }
  }

  function resolvedDefaultSrc(): string {
    return resolveLocal(defaultSrc) || `${assets}/images/cover.jpg`;
  }

  function isDefaultResolved(path: string | null | void): boolean {
    if (!path) return false;
    const r = resolveLocal(path);
    return (
      r === resolvedDefaultSrc() ||
      /(^|\/)images\/cover\.jpg$/i.test(path.trim())
    );
  }

  function clearTimer() {
    if (loadTimer !== null) {
      clearTimeout(loadTimer);
      loadTimer = null;
    }
    if (spinnerDelayTimer !== null) {
      clearTimeout(spinnerDelayTimer);
      spinnerDelayTimer = null;
    }
  }

  function startTimeout() {
    clearTimer();
    loadTimer = setTimeout(() => {
      // Treat as a stalled load and fallback like an error
      handleStall();
    }, LOAD_TIMEOUT_MS) as unknown as number;
  }

  async function startLoading() {
    if (deferLoad) {
      ensureObserved();
      return;
    }
    if (!inView) {
      // Defer until the image is within (or near) the viewport
      ensureObserved();
      return;
    }
    // If no src or clearly invalid, use default immediately
    if (!isValidSrc(src)) {
      resetToDefault();
      lockDefaultFor = src ?? null;
      return;
    }
    // If src is the same as the default cover, don't animate
    if (isDefaultResolved(src)) {
      resetToDefault();
      lockDefaultFor = src ?? null;
      return;
    }
    triedFallback = false;
    usingDefault = false;
    triedLocalFileFallback = false;
    const { url: cacheUrl } = await buildCachePaths(cacheTitle);
    localFileFallback = cacheUrl;
    const resolved = resolveLocal(src);
    if (!resolved) {
      resetToDefault();
      lockDefaultFor = src ?? null;
      return;
    }
    try {
      releaseCurrent = await acquireLoadSlot();
    } catch (_) {
      releaseCurrent = null;
    }
    if (isLinux) {
      if (typeof requestIdleCallback !== "undefined") {
        await new Promise((res) => requestIdleCallback(res, { timeout: 24 }));
      }
      // Spread work over frames to avoid main-thread spikes
      await new Promise((res) => requestAnimationFrame(() => res(null)));
    }
    currentSrc = resolved;
    // Treat non-network sources as immediately loaded (no timeout)
    if (resolved && /^data:/i.test(resolved)) {
      clearTimer();
      loading = false;
      loaded = true;
      showSpinner = false;
      releaseSlot();
      dispatch("load");
      return;
    }
    loading = true;
    showSpinner = true; // show animation immediately for real thumbnails
    startTimeout();
    // Optional delayed assert remains to guard if needed
    spinnerDelayTimer = setTimeout(() => {
      if (loading && !usingDefault) {
        showSpinner = true;
      }
    }, SPINNER_DELAY_MS) as unknown as number;
  }

  function resetToDefault() {
    clearTimer();
    triedFallback = false;
    currentSrc = null;
    usingDefault = true;
    loading = false;
    loaded = false;
    showSpinner = false;
    localFileFallback = null;
    triedLocalFileFallback = false;
    releaseSlot();
    // Schedule a one-shot cache recheck: background queue may fetch it soon
    if (
      enableCache &&
      cacheTitle &&
      cacheRecheckTimer === null &&
      !thumbMemo.has(cacheTitle)
    ) {
      cacheRecheckTimer = setTimeout(async () => {
        cacheRecheckTimer = null;
        try {
          const cached = await invoke<string | null>(
            "get_cached_thumbnail_by_title",
            { title: cacheTitle },
          );
          if (cached) {
            const resolved = resolveLocal(cached);
            if (!resolved) {
              resetToDefault();
              return;
            }
            currentSrc = resolved;
            usingDefault = false;
            loading = false;
            loaded = true;
            showSpinner = false;
            localFileFallback = resolved;
            triedLocalFileFallback = false;
            dispatch("load");
          }
        } catch (_) {
          /* ignore */
        }
      }, CACHE_RECHECK_DELAY_MS) as unknown as number;
    }
  }

  function handleLoad(event: Event) {
    // Some webviews may fire load on 404 responses; validate dimensions
    const img = event.currentTarget as HTMLImageElement | null;
    if (img && (img.naturalWidth === 0 || img.naturalHeight === 0)) {
      // Treat as error to trigger fallback/default
      handleError();
      return;
    }
    clearTimer();
    loading = false;
    loaded = true;
    showSpinner = false;
    releaseSlot();
    dispatch("load");

    // If a remote image loaded successfully, persist it to the cache for future use
    if (
      enableCache &&
      cacheTitle &&
      currentSrc &&
      /^https?:\/\//i.test(currentSrc) &&
      !seenCacheTitles.has(cacheTitle)
    ) {
      seenCacheTitles.add(cacheTitle);
      // Non-blocking; backend will no-op if already cached
      invoke("cache_thumbnail_from_url", {
        title: cacheTitle,
        url: currentSrc,
      }).catch(() => {});
    }
  }

  function useLocalFileFallback(): boolean {
    if (
      !localFileFallback ||
      triedLocalFileFallback ||
      currentSrc === localFileFallback
    ) {
      return false;
    }
    triedLocalFileFallback = true;
    usingDefault = false;
    currentSrc = localFileFallback;
    loading = true;
    loaded = false;
    showSpinner = false;
    startTimeout();
    spinnerDelayTimer = setTimeout(() => {
      if (loading && !usingDefault) {
        showSpinner = true;
      }
    }, SPINNER_DELAY_MS) as unknown as number;
    return true;
  }

  function handleError() {
    clearTimer();
    if (useLocalFileFallback()) {
      return;
    }
    if (
      !triedFallback &&
      fallbackSrc &&
      currentSrc !== resolveLocal(fallbackSrc)
    ) {
      triedFallback = true;
      usingDefault = false;
      if (isDefaultResolved(fallbackSrc)) {
        resetToDefault();
        dispatch("error");
        lockDefaultFor = src ?? null;
        releaseSlot();
        return;
      }
      currentSrc = resolveLocal(fallbackSrc);
      // Keep local file fallback untouched; fallbackSrc may be a remote URL
      // keep loading true until fallback resolves
      loading = true;
      loaded = false;
      showSpinner = false;
      startTimeout();
      spinnerDelayTimer = setTimeout(() => {
        if (loading && !usingDefault) {
          showSpinner = true;
        }
      }, SPINNER_DELAY_MS) as unknown as number;
    } else {
      // Switch to static default cover and hide the spinner
      resetToDefault();
      dispatch("error");
      lockDefaultFor = src ?? null;
      releaseSlot();
    }
  }

  function handleStall() {
    // Same logic as error handler but keep one path
    if (useLocalFileFallback()) {
      return;
    }
    if (
      !triedFallback &&
      fallbackSrc &&
      currentSrc !== resolveLocal(fallbackSrc)
    ) {
      triedFallback = true;
      usingDefault = false;
      if (isDefaultResolved(fallbackSrc)) {
        resetToDefault();
        dispatch("error");
        lockDefaultFor = src ?? null;
        releaseSlot();
        return;
      }
      currentSrc = resolveLocal(fallbackSrc);
      loading = true;
      loaded = false;
      showSpinner = false;
      startTimeout();
      spinnerDelayTimer = setTimeout(() => {
        if (loading && !usingDefault) {
          showSpinner = true;
        }
      }, SPINNER_DELAY_MS) as unknown as number;
    } else {
      resetToDefault();
      dispatch("error");
      lockDefaultFor = src ?? null;
      releaseSlot();
    }
  }

  async function tryLoadCachedOrStart() {
    const srcStr = src?.trim() || "";
    // Always try cached thumbnail when a cacheTitle is provided
    if (enableCache && cacheTitle && cacheTitle.trim().length > 0) {
      if (thumbMemo.has(cacheTitle)) {
        const cached = thumbMemo.get(cacheTitle)!;
        // Data URLs and remote URLs can be used directly
        const resolved =
          cached.startsWith("data:") ||
          cached.startsWith("http://") ||
          cached.startsWith("https://")
            ? cached
            : toAssetUrl(cached);
        if (resolved) {
          triedFallback = false;
          usingDefault = false;
          localFileFallback = resolved;
          triedLocalFileFallback = false;
          currentSrc = resolved;
          // Data URLs should be considered loaded immediately
          clearTimer();
          loading = false;
          loaded = true;
          showSpinner = false;
          releaseSlot();
          dispatch("load");
          return;
        }
      }
      try {
        const cached = await invoke<string | null>(
          "get_cached_thumbnail_by_title",
          { title: cacheTitle },
        );
        if (cached) {
          thumbMemo.set(cacheTitle, cached);
          // Data URLs and remote URLs can be used directly
          const resolved =
            cached.startsWith("data:") ||
            cached.startsWith("http://") ||
            cached.startsWith("https://")
              ? cached
              : toAssetUrl(cached);
          triedFallback = false;
          usingDefault = false;
          localFileFallback = resolved;
          triedLocalFileFallback = false;
          currentSrc = resolved;
          // Data URLs should be considered loaded immediately
          clearTimer();
          loading = false;
          loaded = true;
          showSpinner = false;
          releaseSlot();
          dispatch("load");
          return;
        }
      } catch (_) {
        // ignore cache read errors
      }
    }
    startLoading();
  }

  function ensureObserved() {
    if (inView || !wrapper) return;
    if (unobserve) return;
    unobserve = observeIntersection(
      wrapper,
      (entry) => {
        if (!entry.isIntersecting) return;
        inView = true;
        unobserve?.();
        unobserve = null;
        tryLoadCachedOrStart();
      },
      { rootMargin: isLinux ? "0px" : "150px", threshold: 0.01 },
    );
  }

  onMount(() => {
    // If already in view on mount, we'll proceed immediately; else observe
    ensureObserved();
    if (inView) tryLoadCachedOrStart();
  });

  onDestroy(() => {
    clearTimer();
    if (cacheRecheckTimer !== null) {
      clearTimeout(cacheRecheckTimer);
      cacheRecheckTimer = null;
    }
    releaseSlot();
    if (unobserve) {
      unobserve();
      unobserve = null;
    }
  });

  // Reset when src changes so pagination or prop updates reload correct image
  $: if (src && src.trim().length > 0) {
    const srcStr = src.trim();
    const resolved = resolveLocal(srcStr);
    // If we previously locked default for this src, keep showing default without retrying
    if (lockDefaultFor === srcStr) {
      if (!usingDefault) resetToDefault();
    } else if (
      usingDefault &&
      isValidSrc(srcStr) &&
      !isDefaultResolved(srcStr) &&
      resolved
    ) {
      // A real src arrived after we were showing default; switch and load it.
      usingDefault = false;
      currentSrc = null;
      if (inView && !deferLoad) tryLoadCachedOrStart();
      else ensureObserved();
    } else if (!isValidSrc(srcStr) || isDefaultResolved(srcStr)) {
      if (!usingDefault) resetToDefault();
      lockDefaultFor = srcStr;
    } else if (currentSrc !== resolved && !usingDefault) {
      if (inView && !deferLoad) tryLoadCachedOrStart();
      else ensureObserved();
    } else if (currentSrc === null && !usingDefault) {
      if (inView && !deferLoad) tryLoadCachedOrStart();
      else ensureObserved();
    }
  } else {
    // If no src is provided, immediately show the static default cover
    resetToDefault();
    lockDefaultFor = src ?? null;
  }

  $: if (!deferLoad && inView && !loaded && !loading) {
    tryLoadCachedOrStart();
  }

  $: showSpinnerOverlay =
    hasThumbnail &&
    (showSpinner ||
      (!loaded && usingDefault && enableCache && !!cacheTitle && inView));
</script>

<div
  class={`lazy-image ${className} ${loaded ? "loaded" : ""}`}
  bind:this={wrapper}
  style={!loaded
    ? `background:url('${resolvedDefaultSrc()}') center/cover no-repeat`
    : ""}
>
  {#if usingDefault}
    <img
      src={resolveLocal(defaultSrc) || `${assets}/images/cover.jpg`}
      {alt}
      draggable="false"
      decoding="async"
    />
  {:else if currentSrc}
    {#key currentSrc}
      <img
        src={currentSrc}
        {alt}
        on:load={handleLoad}
        on:error={handleError}
        draggable="false"
        decoding="async"
        aria-hidden={!loaded}
      />
    {/key}
  {:else}
    <!-- Show placeholder cover while waiting to start loading -->
    <!-- default cover via background; no extra element needed -->
  {/if}
  {#if showSpinnerOverlay}
    <div class="spinner-backdrop" aria-hidden="true"></div>
    <div class="spinner-square" aria-hidden="true"></div>
  {/if}
</div>

<style>
  .lazy-image {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: 5px;
    overflow: hidden;
  }

  .lazy-image img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: 5px;
    opacity: 0;
    transition: opacity 120ms ease-out;
  }

  .lazy-image.loaded img {
    opacity: 1;
  }

  /* default-cover no longer needed; default is img-based */

  /* Square spinning throbber */
  .spinner-backdrop {
    position: absolute;
    inset: 0;
    background: rgba(10, 10, 10, 0.45);
  }

  .spinner-square {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 28px;
    height: 28px;
    margin: -14px 0 0 -14px;
    border-radius: 4px;
    background: #fdcf51;
    box-shadow: 0 0 0 2px #f4eee0 inset;
    animation: square-spin 1s linear infinite;
  }

  @keyframes square-spin {
    0% {
      transform: translateZ(0) rotate(0deg);
      opacity: 0.8;
    }
    50% {
      transform: translateZ(0) rotate(180deg);
      opacity: 1;
    }
    100% {
      transform: translateZ(0) rotate(360deg);
      opacity: 0.8;
    }
  }
</style>
