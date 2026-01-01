<script lang="ts">
	import {
		Download,
		Flame,
		//Clock,
		Star,
		Spade,
		Gamepad2,
		LayoutDashboard,
		FolderHeart,
		Search,
		Layers,
		BookOpen,
		Folder,
		RefreshCw,
	} from "lucide-svelte";
	import ModView from "./ModView.svelte";
	import { fly } from "svelte/transition";
	import {
		SortOption,
		backgroundEnabled,
		currentSort,
		loadingStates2,
	} from "../../stores/modStore";
	import { ArrowUpDown } from "lucide-svelte";
	import {
		currentModView,
		currentCategory,
		uninstallDialogStore,
	} from "../../stores/modStore";
	import type { LocalMod, Mod } from "../../stores/modStore";
	import { Category } from "../../stores/modStore";
import {
	modsStore,
	installationStatus,
	withModsCachePersistenceSuspended,
} from "../../stores/modStore";
import {
	descriptionsStore,
	setDescriptions,
	withDescriptionsPersistenceSuspended,
} from "../../stores/descriptions";
import {
	collectionsStore,
	activeCollectionId,
	createCollection,
	renameCollection,
	deleteCollection,
	setActiveCollection,
	exportCollectionCode,
	openCollectionImport,
	setCollectionImportCode,
	lastImportedCollectionId,
} from "../../stores/collections";
import {
	catalogLastRefreshed,
	catalogLoading,
	catalogResetNonce,
} from "../../stores/modStore";
	import type { InstalledMod } from "../../stores/modStore";
	import { invoke, convertFileSrc } from "@tauri-apps/api/core";
// Lazy-load SearchView only when Search tab is active
import type { Component } from "svelte";
let SearchViewComp = $state<Component | null>(null);
import { onMount, onDestroy } from "svelte";
import { listen } from "@tauri-apps/api/event";
import { get, writable } from "svelte/store";
	import { addMessage } from "$lib/stores";
import { currentPage, itemsPerPage, paginationWindow } from "../../stores/modStore";
	import ModCard from "./ModCard.svelte";
	import LocalModCard from "./LocalModCard.svelte";
import {
		checkModInCache,
		fetchCachedMods,
		forceRefreshCache,
	} from "../../stores/modCache";
	import { updateAvailableStore } from "../../stores/modStore";
	import { modEnabledStore } from "../../stores/modStore";
	import { browser } from "$app/environment";
import { openExternal } from "$lib/opener";

	const loadingDots = writable(0);
	let installedMods: InstalledMod[] = [];

	// Dedupe description loads across helpers
	const inflightDescriptions = new Set<string>();
	const attemptedDescriptions = new Set<string>();
	const attemptedCacheTitles = new Set<string>();
	let visibleFirstRunning = false;
	let visibleHydrateTimer: number | null = null;

	// Add these variables to track enabled/disabled mods
	let enabledMods: Mod[] = $state([]);
	let disabledMods: Mod[] = $state([]);
	let enabledLocalMods: LocalMod[] = $state([]);
	let disabledLocalMods: LocalMod[] = $state([]);

	// Animate the dots
	let dotInterval: number;
	let paginating = $state(false);
	let paginationIdleTimer: number | null = null;
	let hydrationTimer: number | null = null;
	let hydrationPending = false;
let downloadsRefreshTimer: number | null = null;
let downloadsRefreshing = false;
let isLinux = false;
let modsScrollContainer: HTMLDivElement | null = $state(null);
	let scrollIdleTimer: number | null = null;
	let isUserScrolling = $state(false);
	let renderLimitLocal = $state(60);
	const renderChunkLocal = 24;
	let localSentinel: HTMLDivElement | null = $state(null);
	let thumbRefreshTimer: number | null = null;
	let thumbRefreshAttempts = 0;
	let selectedCollectionId = $state<string | null>(null);
	let newCollectionName = $state("");
	let renamingId = $state<string | null>(null);
	let renameValue = $state("");
	let collectionBusy = $state<string | null>(null);

	const normalizeCollectionTitle = (name: string) =>
		name.toLowerCase().replace(/[^a-z0-9+]+/g, "").trim();

	async function handleModUninstalled() {
		// Refresh the local mods list
		getLocalMods();
		// Also refresh installed mods for consistency
		refreshInstalledMods();
	}

	// let mods: Mod[] = [];
	let isLoading = $state(true);
	let lastCatalogReset = 0;
	let catalogRetryTimer: number | null = null;
	let catalogRetryCount = 0;
	let catalogRetryPending = $state(false);

	function isRateLimitError(error: unknown): boolean {
		const message = error instanceof Error ? error.message : String(error);
		return message.toLowerCase().includes("rate limited");
	}

	function clearCatalogRetry() {
		if (catalogRetryTimer !== null) {
			clearTimeout(catalogRetryTimer);
			catalogRetryTimer = null;
		}
		catalogRetryCount = 0;
		catalogRetryPending = false;
	}

	function scheduleCatalogRetry(
		mode: "foreground" | "background",
		showMessages: boolean,
	) {
		if (catalogRetryTimer !== null || typeof window === "undefined") {
			return;
		}
		const baseDelayMs = 5000;
		const backoffStep = Math.min(catalogRetryCount, 4);
		const delayMs = Math.min(60000, baseDelayMs * Math.pow(2, backoffStep));
		const jitterMs = Math.floor(Math.random() * 1000);
		const waitMs = delayMs + jitterMs;
		catalogRetryCount += 1;
		catalogRetryPending = true;
		if (showMessages) {
			addMessage(
				`Rate limited. Retrying in ${Math.ceil(waitMs / 1000)}s…`,
				"warning",
			);
		}
		catalogRetryTimer = window.setTimeout(() => {
			catalogRetryTimer = null;
			if (mode === "foreground") {
				isLoading = true;
				catalogRetryPending = false;
				loadCatalogForeground()
					.catch(() => {})
					.finally(() => {
						isLoading = false;
					});
			} else {
				refreshCatalogInBackground(showMessages).catch(() => {});
			}
		}, waitMs);
	}

	function normalizeText(text: string): string {
		return text
			.toLowerCase()
			.replace(/!\[[^\]]*\]\([^)]+\)/g, " ")
			.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
			.replace(/<img[^>]*>/gi, " ")
			.replace(/<[^>]+>/g, " ")
			.replace(/[^a-z0-9]+/g, " ")
			.trim()
			.replace(/\s+/g, " ");
	}

	function hasMeaningfulDescription(desc: string | null | undefined, title: string): boolean {
		if (!desc) return false;
		const trimmed = desc.trim();
		if (!trimmed) return false;
		const normalized = normalizeText(trimmed);
		const normalizedTitle = normalizeText(title);
		if (!normalized || normalized === normalizedTitle) return false;
		if (normalized.startsWith(`what is ${normalizedTitle}`)) return false;
		if (trimmed.length < 60) return false;
		return true;
	}

	interface DependencyCheck {
		steamodded: boolean;
		talisman: boolean;
	}

	let localMods: LocalMod[] = $state([]);
	let isLoadingLocalMods = $state(false);

	async function handleModToggled(): Promise<void> {
		if ($currentCategory === "Installed Mods") {
			// First check catalog mods
			for (const mod of paginatedMods) {
				if ($installationStatus[mod.title]) {
					try {
						const isEnabled = await invoke<boolean>(
							"is_mod_enabled",
							{
								modName: mod.title,
							},
						);
						modEnabledStore.update((s) => ({
							...s,
							[mod.title]: isEnabled,
						}));
					} catch (error) {
						console.error(
							`Failed to check catalog mod status: ${error}`,
						);
					}
				}
			}

			// Then check local mods via batch
			// (summary refresh handles local + catalog states)

			// Update filtered lists
			updateEnabledDisabledLists();

			// Force Svelte reactivity by creating new array references
			enabledMods = [...enabledMods];
			disabledMods = [...disabledMods];
			enabledLocalMods = [...enabledLocalMods];
			disabledLocalMods = [...disabledLocalMods];
		}
	}

	async function hydrateRequirements(mod: Mod): Promise<Mod> {
		if (!mod._dirName) return mod;
		if (mod.requires_steamodded || mod.requires_talisman) return mod;
		try {
			const [requiresSteamodded, requiresTalisman] = await invoke<
				[boolean, boolean]
			>("get_mod_requirements", { dirName: mod._dirName });
			if (!requiresSteamodded && !requiresTalisman) return mod;
			modsStore.update((arr) =>
				arr.map((m) =>
					m.title === mod.title
						? {
								...m,
								requires_steamodded: requiresSteamodded,
								requires_talisman: requiresTalisman,
						  }
						: m,
				),
			);
			return {
				...mod,
				requires_steamodded: requiresSteamodded,
				requires_talisman: requiresTalisman,
			};
		} catch (_) {
			return mod;
		}
	}

	async function getLocalMods() {
		if ($currentCategory === "Installed Mods") {
			isLoadingLocalMods = true;
			try {
				localMods = await invoke("get_detected_local_mods");
				await refreshStateSummary();
			} catch (error) {
				console.error("Failed to load local mods:", error);
				addMessage(`Failed to load local mods: ${error}`, "error");
				localMods = [];
			} finally {
				isLoadingLocalMods = false;
			}
		}
	}

	async function refreshDownloadsLive() {
		if (downloadsRefreshing) return;
		downloadsRefreshing = true;
		try {
			const sort = get(currentSort);
			if (
				sort === SortOption.DownloadsAsc ||
				sort === SortOption.DownloadsDesc
			) {
				const items = await invoke<ArchiveModItem[]>("fetch_repo_mods", {
					sort,
				});
				const mods = mapArchiveItems(items);
				mergeIncomingMods(mods);
				return;
			}
			const downloads = await invoke<
				Record<string, { total: number; today?: number }>
			>("fetch_repo_downloads", { sort });
			modsStore.update((arr) =>
				arr.map((m) => {
					if (!m._dirName) return m;
					const entry = downloads[m._dirName];
					if (!entry) return m;
					if (m.downloads_total === entry.total) return m;
					return { ...m, downloads_total: entry.total };
				}),
			);
		} catch (e) {
			console.warn("downloads refresh failed", e);
		} finally {
			downloadsRefreshing = false;
		}
	}

    // Avoid forcing a refresh on every reactive pass; only fetch local mods here.
    // We refresh installed mods on category switch and after install/uninstall events.
    $effect(() => {
        if ($currentCategory === "Installed Mods") {
            getLocalMods();
        }
    });

const { handleDependencyCheck, mod } = $props<{
    handleDependencyCheck: (
        requirements: DependencyCheck,
        downloadAction?: () => Promise<void>,
    ) => void;
    mod: Mod | null;
}>();

	// Add this helper function to handle scrolling to top
	function scrollToTop() {
		const scrollContainer = document.querySelector(
			".mods-scroll-container",
		);
		if (scrollContainer) {
			scrollContainer.scrollTo({
				top: 0,
				behavior: isLinux ? "auto" : "smooth",
			});
		}
		setTimeout(() => {}, 500); // Delay to prevent scroll handler triggering during animated scroll
	}

	function markPaginating() {
		paginating = true;
		if (paginationIdleTimer) {
			clearTimeout(paginationIdleTimer);
		}
		paginationIdleTimer = window.setTimeout(() => {
			paginating = false;
			if (hydrationPending) scheduleHydration();
		}, isLinux ? 220 : 150);
	}

	function updateEnabledDisabledLists() {
		// Filter catalog mods - explicitly check for boolean values
		enabledMods = paginatedMods.filter(
			(mod) =>
				$installationStatus[mod.title] &&
				$modEnabledStore[mod.title] === true,
		);
		disabledMods = paginatedMods.filter(
			(mod) =>
				$installationStatus[mod.title] &&
				$modEnabledStore[mod.title] === false,
		);

		// Filter local mods - explicitly check for boolean values
		const enabled: LocalMod[] = [];
		const disabled: LocalMod[] = [];
		for (const mod of localMods) {
			const direct = $modEnabledStore[mod.name];
			if (direct === true) {
				enabled.push(mod);
				continue;
			}
			if (direct === false) {
				disabled.push(mod);
				continue;
			}
			const folderName = mod.path.split(/[\\/]/).pop();
			const byPath =
				folderName && folderName in $modEnabledStore
					? $modEnabledStore[folderName]
					: undefined;
			if (byPath === false) {
				disabled.push(mod);
			} else {
				enabled.push(mod);
			}
		}
		enabledLocalMods = enabled;
		disabledLocalMods = disabled;
	}

	// Update the lists whenever the stores change
	$effect(() => {
		if ($currentCategory === "Installed Mods") {
			updateEnabledDisabledLists();
		}
	});

	$effect(() => {
		if ($catalogResetNonce === 0 || $catalogResetNonce === lastCatalogReset) {
			return;
		}
		lastCatalogReset = $catalogResetNonce;
		if ($catalogLoading) {
			return;
		}
		if ($modsStore.length > 0) {
			return;
		}
		(async () => {
			isLoading = true;
			try {
				await loadCatalogForeground();
			} finally {
				isLoading = false;
			}
		})();
	});

	onMount(() => {
		// Animation dots initialization
		dotInterval = setInterval(() => {
			loadingDots.update((n) => (n + 1) % 4);
		}, 500);

		refreshDownloadsLive().catch(() => {});
		downloadsRefreshTimer = window.setInterval(() => {
			refreshDownloadsLive().catch(() => {});
		}, 60000);

		if (browser) {
			const plat =
				document.documentElement.dataset.platform ||
				(navigator.userAgent.toLowerCase().includes("linux")
					? "linux"
					: "");
			isLinux = plat === "linux";
		}

		// Separate async function for initialization
		const initialize = async () => {
			try {
				isLoading = true;
				// If the user is on Installed Mods, pre-seed placeholders so they are visible immediately
				if ($currentCategory === "Installed Mods") {
					await seedInstalledPlaceholders();
				}
				// If we have no cached catalog yet, try to hydrate from Rust cache first
				if ($modsStore.length === 0) {
					let hydrated = false;
					try {
						const cached = await invoke<[CachedModItem[], number] | null>(
							"load_mods_cache",
						);
						if (cached) {
							const [items, ts] = cached;
							if (items && items.length > 0) {
								modsStore.set(mapCachedMods(items));
								if (ts) {
									catalogLastRefreshed.set(ts * 1000);
								}
								hydrated = true;
							}
						}
					} catch (_) {
						// ignore cache read errors
					}
					if (hydrated) {
						refreshCatalogInBackground(false).catch(() => {});
					} else {
						await loadCatalogForeground();
					}
				} else {
					// Otherwise, refresh in the background
					refreshCatalogInBackground();
					// Also try to hydrate missing descriptions/images from Rust cache.
					try {
						const cached = await invoke<[CachedModItem[], number] | null>(
							"load_mods_cache",
						);
						if (cached) {
							applyCachedDetails(cached[0]);
						}
					} catch (_) {
						// ignore cache read errors
					}
				}

				// After mods load, update install status and local mods if needed
				try {
					const installed = await fetchCachedMods();
					const installedSet = new Set(
						installed.map((mod) => mod.name.toLowerCase()),
					);
					installationStatus.set(
						Object.fromEntries(
							$modsStore.map((mod) => [
								mod.title,
								installedSet.has(mod.title.toLowerCase()),
							]),
						),
					);
				} catch (error) {
					console.error("Install status check failed:", error);
				}

				// Fill local thumbnails for installed mods to avoid remote image fetches
				try {
					await fillInstalledThumbnails($modsStore);
				} catch (e) {
					console.warn("thumbnail fill failed", e);
				}

				if ($currentCategory === "Installed Mods") {
					await getLocalMods();
				}
			} finally {
				isLoading = false;
			}
		};

		// Separate async function for background state
		const initBackgroundState = async () => {
			try {
				const isBackgroundAnimationEnabled: boolean = await invoke(
					"get_background_state",
				);
				backgroundEnabled.set(isBackgroundAnimationEnabled);
			} catch (error) {
				console.error("Failed to get background status:", error);
				addMessage(
					"Error fetching background animation status",
					"error",
				);
			}
		};

		// Call async functions without awaiting them directly in onMount
		initialize();
		initBackgroundState();

		let installedModsRefresh: Promise<void> | null = null;
		const scheduleInstalledModsRefresh = () => {
			if (!installedModsRefresh) {
				installedModsRefresh = (async () => {
					await refreshInstalledMods();
					await getLocalMods();
					await refreshStateSummary();
				})().finally(() => {
					installedModsRefresh = null;
				});
			}
			return installedModsRefresh;
		};
		// Intersection observers to extend render window for local mods
		onMount(() => {
			if (typeof IntersectionObserver !== "undefined") {
				if (localSentinel) {
					observerLocal = new IntersectionObserver((entries) => {
						for (const entry of entries) {
							if (entry.isIntersecting) {
								const localMax = Math.max(
									enabledLocalMods.length,
									disabledLocalMods.length,
								);
								renderLimitLocal = Math.min(
									renderLimitLocal + renderChunkLocal,
									localMax,
								);
							}
						}
					}, { root: modsScrollContainer, rootMargin: "200px" });
					observerLocal.observe(localSentinel);
				}
			}
		});

		// Lazy-load SearchView when needed
			$effect(() => {
				if (showSearch && !SearchViewComp) {
					import("./SearchView.svelte")
						.then((m) => (SearchViewComp = m.default))
						.catch((err) =>
							console.warn("Failed to load SearchView:", err),
						);
				}
			});

			// Listen for backend notifications of installed mods changes
			let unlistenModsChanged: (() => void) | null = null;
			listen("installed-mods-changed", async () => {
				if ($currentCategory === "Installed Mods") {
					try {
						await scheduleInstalledModsRefresh();
					} catch (err) {
						console.warn("Failed to refresh installed mods:", err);
					}
				}
			})
				.then((un) => (unlistenModsChanged = un))
				.catch((err) =>
					console.warn("Failed to subscribe to installed-mods:", err),
				);

			// Return synchronous cleanup function
			return () => {
				clearInterval(dotInterval);
				if (downloadsRefreshTimer) clearInterval(downloadsRefreshTimer);
				if (scrollIdleTimer) {
					clearTimeout(scrollIdleTimer);
					scrollIdleTimer = null;
				}
				try {
					observerLocal?.disconnect();
				} catch (_) { /* ignore */ }
				try {
					if (typeof unlistenModsChanged === "function") unlistenModsChanged();
				} catch (err) {
					console.warn("Failed to unlisten installed-mods:", err);
				}
			};
		});

	const getAllInstalledMods = async () => {
		try {
			installedMods = await fetchCachedMods();
		} catch (error) {
			console.error("Failed to get installed mods:", error);
		}
	};

    const uninstallMod = async (mod: Mod) => {
        const isCoreMod = ["steamodded", "talisman"].includes(
            mod.title.toLowerCase(),
        );
        try {
            await getAllInstalledMods();
            const installedMod = installedMods.find(
                (m) => m.name.toLowerCase() === mod.title.toLowerCase(),
            );
            if (isCoreMod) {
                // Get dependents
                const dependents = await invoke<string[]>("get_dependents", {
                    modName: mod.title,
                });
                // Set the dialog properties directly
                uninstallDialogStore.set({
                    show: true,
                    modName: mod.title,
                    // Path may be resolved in the dialog if missing
                    modPath: installedMod?.path || "",
                    dependents,
                });
            } else {
                // For non-core mods
                if (!installedMod) return;
                await invoke("remove_installed_mod", {
                    name: mod.title,
                    path: installedMod.path,
                });
                installationStatus.update((s) => ({
                    ...s,
					[mod.title]: false,
				}));
			}
		} catch (error) {
			console.error("Uninstall failed:", error);
			addMessage(`Uninstall failed: ${error}`, "error");
		}
	};

    let hasUpdatesAvailable = $derived(
        Object.values($updateAvailableStore).some((value) => value === true)
    );

	async function updateAllMods(e?: Event) {
		if (e) e.preventDefault();

		try {
			// Get all installed mods with available updates
			const modsToUpdate = $modsStore.filter(
				(mod) =>
					$installationStatus[mod.title] &&
					$updateAvailableStore[mod.title],
			);

			if (modsToUpdate.length === 0) {
				addMessage("No updates available.", "info");
				return;
			}

			const previousEnabledMap: Record<string, boolean> = Object.fromEntries(
				await Promise.all(
					modsToUpdate.map(async (mod) => {
						const cached = $modEnabledStore?.[mod.title];
						if (cached !== undefined) {
							return [mod.title, cached] as const;
						}

						try {
							const enabled = await invoke<boolean>("is_mod_enabled", {
								modName: mod.title,
							});
							return [mod.title, enabled] as const;
						} catch (error) {
							console.error(
								`Failed to read enabled state for ${mod.title}:`,
								error,
							);
							return [mod.title, true] as const;
						}
					}),
				),
			);

			// Set loading state for all mods simultaneously
			for (const mod of modsToUpdate) {
				loadingStates2.update((s) => ({ ...s, [mod.title]: true }));
			}

			// Run all updates in parallel
			const updateResults = await Promise.allSettled(
				modsToUpdate.map(async (mod) => {
					try {
						if (mod.title.toLowerCase() === "steamodded") {
							const latestReleaseURL = await invoke<string>(
								"get_latest_steamodded_release",
							);
							await installModFromURL(mod, latestReleaseURL);
						} else if (mod.downloadURL) {
							const folderName =
								mod.folderName || mod.title.replace(/\s+/g, "");
							const installedPath = await invoke<string>(
								"install_mod",
								{
									url: mod.downloadURL,
									folderName,
								},
							);

							await invoke("add_installed_mod", {
								name: mod.title,
								path: installedPath,
								dependencies: mod.requires_steamodded
									? ["Steamodded"]
									: mod.requires_talisman
										? ["Talisman"]
										: [],
								currentVersion: mod.version || "",
							});
						} else {
							throw new Error("No download URL available");
						}

						// Update was successful
						return mod.title;
					} catch (error) {
						console.error(
							`Failed to update mod ${mod.title}:`,
							error,
						);
						throw new Error(
							`Failed to update ${mod.title}: ${error instanceof Error ? error.message : String(error)}`,
						);
					}
				}),
			);

			// Process results
			const successful: string[] = [];
			const failed: string[] = [];

			updateResults.forEach((result, index) => {
				const modTitle = modsToUpdate[index].title;

				// Clear loading state
				loadingStates2.update((s) => ({ ...s, [modTitle]: false }));

				if (result.status === "fulfilled") {
					successful.push(modTitle);
					// Mark as updated
					updateAvailableStore.update((s) => ({
						...s,
						[modTitle]: false,
					}));
					modEnabledStore.update((s) => ({
						...s,
						[modTitle]: previousEnabledMap[modTitle],
					}));
				} else {
					failed.push(modTitle);
					// Show error message
					addMessage(result.reason.message, "error");
				}
			});

			// Refresh the installed mods list
			await refreshInstalledMods();

			// Show success message
			if (successful.length > 0) {
				addMessage(
					`Successfully updated ${successful.length} mod(s).`,
					"success",
				);
			}
		} catch (error) {
			console.error("Failed to update mods:", error);
			addMessage(
				`Update all failed: ${error instanceof Error ? error.message : String(error)}`,
				"error",
			);
		}
	}

	// Helper function for Steamodded installation (matching ModCard.svelte pattern)
	async function installModFromURL(
		mod: Mod,
		url: string,
		folder_name: string = "",
	) {
		const wasInstalled = Boolean($installationStatus[mod.title]);
		let desiredEnabledState = true;

		if (wasInstalled) {
			let previousEnabled = $modEnabledStore?.[mod.title];
			if (previousEnabled === undefined) {
				try {
					previousEnabled = await invoke<boolean>("is_mod_enabled", {
						modName: mod.title,
					});
				} catch (error) {
					console.error(
						`Failed to read existing enabled state for ${mod.title}:`,
						error,
					);
				}
			}

			if (previousEnabled !== undefined) {
				desiredEnabledState = previousEnabled;
			}
		}

		try {
			if (!url.startsWith("http") && !url.startsWith("bmi://")) {
				console.error("Invalid URL format:", url);
				throw new Error(`Invalid URL format: ${url}`);
			}

			// Use mod title as fallback if folder_name is empty
			const folderName =
				folder_name || mod.folderName || mod.title.replace(/\s+/g, "");

			const installedPath = await invoke<string>("install_mod", {
				url,
				folderName,
			});

			await invoke("add_installed_mod", {
				name: mod.title,
				path: installedPath,
				dependencies: mod.requires_steamodded ? ["Steamodded"] : [],
				currentVersion: mod.version || "",
			});

			installationStatus.update((s) => ({ ...s, [mod.title]: true }));
			updateAvailableStore.update((s) => ({ ...s, [mod.title]: false }));

			modEnabledStore.update((s) => ({
				...s,
				[mod.title]: desiredEnabledState,
			}));
		} catch (error) {
			console.error("Failed to install mod:", error);
			throw error; // Rethrow to be handled by the caller
		}
	}

	const installMod = async (mod: Mod) => {
		if (!mod?.title || !mod?.downloadURL) return;
		const modToInstall = await hydrateRequirements(mod);

		// Define the actual download function that will be stored and executed later if needed
		const performDownload = async () => {
			try {
				loadingStates2.update((s: Record<string, boolean>) => ({
					...s,
					[modToInstall.title]: true,
				}));

				// Create dependencies array for the database
				const dependencies = [];
				if (modToInstall.requires_steamodded) dependencies.push("Steamodded");
				if (modToInstall.requires_talisman) dependencies.push("Talisman");

				const installedPath = await invoke<string>("install_mod", {
					url: modToInstall.downloadURL,
					folderName:
						modToInstall.folderName ||
						modToInstall.title.replace(/\s+/g, ""),
				});

				await invoke("add_installed_mod", {
					name: modToInstall.title,
					path: installedPath,
					dependencies,
					currentVersion: modToInstall.version || "",
				});

				installationStatus.update((s) => ({
					...s,
					[modToInstall.title]: true,
				}));
				updateAvailableStore.update((s) => ({
					...s,
					[modToInstall.title]: false,
				}));
				await refreshInstalledMods();
			} catch (error) {
				console.error("Failed to install mod:", error);
				const raw =
					error instanceof Error ? error.message : String(error);
				const onlyUrlMsg = raw.includes("Download URL not reachable")
					? raw.match(/Download URL not reachable[^"]*/)?.[0] || raw
					: `Installation failed: ${raw}`;
				addMessage(onlyUrlMsg, "error");
			} finally {
				loadingStates2.update((s: Record<string, boolean>) => ({
					...s,
					[modToInstall.title]: false,
				}));
			}
		};

		try {
			// Check for dependencies
			if (modToInstall.requires_steamodded || modToInstall.requires_talisman) {
				// Check Steamodded if required
				const steamoddedInstalled = modToInstall.requires_steamodded
					? await invoke<boolean>("check_mod_installation", {
							modType: "Steamodded",
						})
					: true;

				// Check Talisman if required
				const talismanInstalled = modToInstall.requires_talisman
					? await invoke<boolean>("check_mod_installation", {
							modType: "Talisman",
						})
					: true;

				// If any dependency is missing, show the Requires Popup
				if (
					(modToInstall.requires_steamodded && !steamoddedInstalled) ||
					(modToInstall.requires_talisman && !talismanInstalled)
				) {
					// Call the handler with the appropriate requirements and download action
					handleDependencyCheck(
						{
							steamodded:
								modToInstall.requires_steamodded && !steamoddedInstalled,
							talisman:
								modToInstall.requires_talisman && !talismanInstalled,
						},
						performDownload,
					);
					return; // Stop installation
				}
			}

			// If we get here, either no dependencies are required or all are installed
			// Proceed with installation directly
			await performDownload();
		} catch (error) {
			console.error("Failed to check dependencies:", error);
			addMessage(
				`Dependency check failed: ${error instanceof Error ? error.message : String(error)}`,
				"error",
			);
		}
	};

	interface ModMeta {
		title: string;
		"requires-steamodded": boolean;
		"requires-talisman": boolean;
		requires_steamodded?: boolean;
		requires_talisman?: boolean;
		categories: string[];
		author: string;
		repo: string;
		downloadURL?: string;
		folderName?: string;
		version?: string;
		"last-updated"?: number;
		downloads?: { total: number; today?: number };
	}

	// Do not depend on cache for catalog; prefer fresh data + lazy UI
	const CACHE_DURATION = 0;

	// Types returned by the single-archive Tauri command
	interface ArchiveModItem {
		dir_name: string;
		meta: ModMeta;
		description: string;
		image_url: string;
		has_thumbnail?: boolean;
	}

	interface CachedModItem {
		id?: string;
		title: string;
		description: string;
		image: string;
		categories: Category[];
		colors: { color1: string; color2: string };
		installed: boolean;
		requires_steamodded: boolean;
		requires_talisman: boolean;
		publisher: string;
		repo: string;
		downloadURL: string;
		folderName?: string | null;
		version?: string | null;
	}

	function mapArchiveItems(
		items: ArchiveModItem[],
		cachedMap?: Record<string, string>,
	): Mod[] {
		return items.map((item) => {
			const mappedCategories = item.meta.categories
				.map((cat) => categoryMap[cat] ?? null)
				.filter((cat): cat is Category => cat !== null);

			const cachedThumb = cachedMap?.[item.meta.title];
			const img = cachedThumb ? convertFileSrc(cachedThumb) : "/images/cover.jpg";
			const hasThumb = item.has_thumbnail === true;
			return {
				id: item.dir_name,
				title: item.meta.title,
				description: item.description,
				image: img,
				imageFallback: cachedThumb ? img : undefined,
				_hasThumbnail: hasThumb,
				colors: getRandomColorPair(),
				categories: mappedCategories,
				requires_steamodded:
					item.meta["requires-steamodded"] ??
					item.meta.requires_steamodded ??
					false,
				requires_talisman:
					item.meta["requires-talisman"] ??
					item.meta.requires_talisman ??
					false,
				publisher: item.meta.author,
				repo: item.meta.repo,
				downloadURL: item.meta.downloadURL || "",
				folderName: item.meta.folderName,
				version: item.meta.version,
				installed: false,
				last_updated: item.meta["last-updated"] ?? 0,
				downloads_total: item.meta.downloads?.total,
				_dirName: item.dir_name,
			};
		});
	}

	function mapCachedMods(items: CachedModItem[]): Mod[] {
		return items.map((item) => {
			const image = item.image?.trim() || "/images/cover.jpg";
			const hasThumb = !/\/images\/cover\.jpg$/i.test(image);
			const cachedId =
				item.id ||
				(item.downloadURL?.startsWith("bmi://")
					? item.downloadURL.slice("bmi://".length)
					: item.folderName ?? undefined);
			return {
				id: cachedId,
				title: item.title,
				description: item.description ?? "",
				image,
				imageFallback: hasThumb ? image : undefined,
				colors: item.colors ?? getRandomColorPair(),
				categories: item.categories ?? [],
				requires_steamodded: item.requires_steamodded ?? false,
				requires_talisman: item.requires_talisman ?? false,
				publisher: item.publisher ?? "",
				repo: item.repo ?? "",
				downloadURL: item.downloadURL ?? "",
				folderName: item.folderName ?? null,
				version: item.version ?? null,
				installed: item.installed ?? false,
				last_updated: 0,
				_hasThumbnail: hasThumb,
			};
		});
	}

	function applyCachedDetails(items: CachedModItem[]) {
		if (!items || items.length === 0) return;
		const byTitle = new Map(items.map((item) => [item.title, item]));
		const descriptionUpdates: Record<string, string> = {};
		modsStore.update((arr) =>
			arr.map((m) => {
				const cached = byTitle.get(m.title);
				if (!cached) return m;
				const desc = cached.description?.trim();
				const image = cached.image?.trim();
				const hasThumb = image ? !/\/images\/cover\.jpg$/i.test(image) : false;
				const updated: Mod = { ...m };
				if ((!updated.description || updated.description.trim().length === 0) && desc) {
					updated.description = desc;
					descriptionUpdates[m.title] = desc;
				}
				if (image && (!updated.image || /\/images\/cover\.jpg$/i.test(updated.image))) {
					updated.image = image;
					updated.imageFallback = hasThumb ? image : undefined;
				}
				if (updated._hasThumbnail === undefined && image) {
					updated._hasThumbnail = hasThumb;
				}
				return updated;
			}),
		);
		setDescriptions(descriptionUpdates);
	}

	async function refreshCatalogInBackground(showMessages: boolean = true): Promise<void> {
		if ($catalogLoading) return;
		catalogLoading.set(true);
		if (showMessages) {
			addMessage("Loading mods in background…", "info");
		}
		try {
			const items = await invoke<ArchiveModItem[]>("fetch_repo_mods", {
				sort: $currentSort,
			});
			clearCatalogRetry();
			const titles = items.map((i) => i.meta.title);
			const cachedMap = await invoke<Record<string, string>>(
				"get_cached_thumbnails_map",
				{ titles },
			);
			// Enqueue background caching for thumbnails (non-blocking, handles 429)
			try {
				const thumbItems = items
					.filter((i) => i.image_url && /^https?:\/\//i.test(i.image_url))
					.map((i) => ({ title: i.meta.title, url: i.image_url }));
				if (thumbItems.length > 0) {
					const toEnqueue = thumbItems.filter(
						(t) => !cachedMap[t.title],
					);
					if (toEnqueue.length > 0) {
						// fire and forget
						invoke("enqueue_thumbnails", { items: toEnqueue }).catch(
							() => {},
						);
					}
				}
			} catch (_) {
				/* ignore */
			}
            const mods: Mod[] = mapArchiveItems(items, cachedMap);

            // Merge fresh remote mods with any locally seeded placeholders; keep server order
            const prunedCount = mergeIncomingMods(mods);

            if (prunedCount > 0) {
                addMessage(`Pruned ${prunedCount} removed mod${prunedCount === 1 ? '' : 's'} from cache`, "info");
            }

            // Persist refreshed upstream catalog to Rust cache for update checks
            try {
                const forCache = mods.map((m) => ({
                    title: m.title,
                    description: m.description,
                    image: m.image,
                    categories: m.categories,
                    colors: m.colors,
                    installed: false,
                    requires_steamodded: m.requires_steamodded,
                    requires_talisman: m.requires_talisman,
                    publisher: m.publisher,
                    repo: m.repo,
                    downloadURL: m.downloadURL || "",
                    folderName: m.folderName ?? null,
                    version: m.version ?? null,
                }));
                invoke("save_mods_cache", { mods: forCache }).catch(() => {});
            } catch (_) { /* ignore */ }

			// Re-apply local thumbnails for installed mods (non-blocking)
			fillInstalledThumbnails($modsStore).catch(() => {});
			// Re-check cached thumbnails after background fetches
			scheduleThumbCacheRefresh(titles);
			// Suspend cache persistence during description hydration to avoid thrashing localStorage
			await withModsCachePersistenceSuspended(async () => {
				await withDescriptionsPersistenceSuspended(async () => {
					try { await fillCachedDescriptionsVisibleFirst(); } catch { /* ignore */ }
					try { await fillDescriptionsVisibleFirst(); } catch { /* ignore */ }
					fillCachedDescriptions($modsStore).catch(() => {});
					fillDescriptions(mods).catch((e) => console.warn("desc fill failed", e));
				});
			});
			if (showMessages) {
				addMessage("All mods loaded", "success");
			}
		} catch (error) {
			console.error("Failed to refresh catalog:", error);
			if (isRateLimitError(error)) {
				scheduleCatalogRetry("background", showMessages);
				return;
			}
			if (showMessages) {
				addMessage(
					`Background load failed: ${error instanceof Error ? error.message : String(error)}`,
					"error",
				);
			}
		} finally {
			catalogLoading.set(false);
		}
	}

	// Foreground loader for first-run (no cached catalog): blocks UI spinner until ready
	async function loadCatalogForeground(): Promise<void> {
		if ($catalogLoading) return;
		catalogLoading.set(true);
        try {
            const items = await invoke<ArchiveModItem[]>("fetch_repo_mods", {
                sort: $currentSort,
            });
			clearCatalogRetry();
            const titles = items.map((i) => i.meta.title);
            const cachedMap = await invoke<Record<string, string>>(
                "get_cached_thumbnails_map",
                { titles },
            );
            // Enqueue background caching for thumbnails
            try {
                const thumbItems = items
                    .filter((i) => i.image_url && /^https?:\/\//i.test(i.image_url))
                    .map((i) => ({ title: i.meta.title, url: i.image_url }));
                if (thumbItems.length > 0) {
                    const seen = new Set<string>();
                    const filtered = thumbItems.filter((t) => {
                        if (seen.has(t.title)) return false;
                        seen.add(t.title);
                        if (cachedMap[t.title]) return false;
                        const existing = $modsStore.find((m) => m.title === t.title);
                        if (existing && existing.image && !existing.image.endsWith("cover.jpg")) {
                            return false;
                        }
                        return true;
                    });
                    if (filtered.length > 0) {
                        invoke("enqueue_thumbnails", { items: filtered }).catch(() => {});
                    }
                }
            } catch (_) { /* ignore */ }
            const mods: Mod[] = mapArchiveItems(items, cachedMap);

            // Merge with any pre-seeded placeholders, and cautiously prune removed mods
            const prunedCount = mergeIncomingMods(mods as Mod[]);

            if (prunedCount > 0) {
                addMessage(`Pruned ${prunedCount} removed mod${prunedCount === 1 ? '' : 's'} from cache`, "info");
            }

            // Persist refreshed upstream catalog to Rust cache
            try {
                const forCache = (mods as Mod[]).map((m) => ({
                    title: m.title,
                    description: m.description,
                    image: m.image,
                    categories: m.categories,
                    colors: m.colors,
                    installed: false,
                    requires_steamodded: m.requires_steamodded,
                    requires_talisman: m.requires_talisman,
                    publisher: m.publisher,
                    repo: m.repo,
                    downloadURL: m.downloadURL || "",
                    folderName: m.folderName ?? null,
                    version: m.version ?? null,
                }));
                invoke("save_mods_cache", { mods: forCache }).catch(() => {});
            } catch (_) { /* ignore */ }

			// Also kick off thumbnails/descriptions
			fillInstalledThumbnails($modsStore).catch(() => {});
			scheduleThumbCacheRefresh(titles);
			await withModsCachePersistenceSuspended(async () => {
				await withDescriptionsPersistenceSuspended(async () => {
					try { await fillCachedDescriptionsVisibleFirst(); } catch { /* ignore */ }
					try { await fillDescriptionsVisibleFirst(); } catch { /* ignore */ }
					fillCachedDescriptions($modsStore).catch(() => {});
					fillDescriptions(mods).catch(() => {});
				});
			});
        } catch (error) {
            if (isRateLimitError(error)) {
                scheduleCatalogRetry("foreground", true);
            } else {
                throw error;
            }
        } finally {
            catalogLoading.set(false);
        }
	}

	async function fillDescriptions(mods: (Mod & { _dirName?: string })[]) {
		// Limit concurrent requests to avoid 429s and prioritize detail view
		const limit = 6;
		let i = 0;
		const updates: { title: string; description: string }[] = [];
		const applyBatch = () => {
			if (updates.length === 0) return;
			const batch = updates.splice(0, updates.length);
			setDescriptions(
				Object.fromEntries(batch.map((b) => [b.title, b.description])),
			);
		};
		async function worker() {
			while (true) {
				const idx = i++;
				if (idx >= mods.length) break;
				const m = mods[idx];
				if (!m || hasMeaningfulDescription(m.description ?? "", m.title)) continue;
				if (attemptedDescriptions.has(m.title)) continue;
				if (inflightDescriptions.has(m.title)) continue;
                const dir = m._dirName as string | undefined;
				if (!dir) continue;
				try {
					inflightDescriptions.add(m.title);
					const text = await invoke<string>(
						"get_description_cached_or_remote",
						{ title: m.title, dirName: dir },
					);
					attemptedDescriptions.add(m.title);
					updates.push({ title: m.title, description: text });
				} catch (_) {
					// ignore per-mod desc failures
					attemptedDescriptions.add(m.title);
				} finally {
					inflightDescriptions.delete(m.title);
				}
			}
		}
		await Promise.all(
			new Array(Math.min(limit, mods.length)).fill(0).map(worker),
		);
		applyBatch();
	}

	async function fillDescriptionsVisibleFirst() {
		if (isUserScrolling) {
			hydrationPending = true;
			return;
		}
		// Prioritize current page mods so skeletons disappear quickly
		const candidates = paginatedMods
			.filter((m) => !m.description || m.description.trim().length === 0)
			.map((m) => ({ title: m.title, dir: m._dirName as string | undefined }))
			.filter((x) => Boolean(x.dir));
		if (candidates.length === 0) return;
		const limit = 4;
		let i = 0;
		const updates: { title: string; description: string }[] = [];
		const applyBatch = () => {
			if (updates.length === 0) return;
			const batch = updates.splice(0, updates.length);
			setDescriptions(
				Object.fromEntries(batch.map((b) => [b.title, b.description])),
			);
		};
		visibleFirstRunning = true;
		async function worker() {
			while (true) {
				const idx = i++;
				if (idx >= candidates.length) break;
				const c = candidates[idx]!;
				if (inflightDescriptions.has(c.title)) continue;
				if (attemptedDescriptions.has(c.title)) continue;
				if (hasMeaningfulDescription(
					get(descriptionsStore)[c.title] ?? "",
					c.title,
				)) {
					continue;
				}
				try {
					inflightDescriptions.add(c.title);
					const text = await invoke<string>(
						"get_description_cached_or_remote",
						{ title: c.title, dirName: c.dir }
					);
					attemptedDescriptions.add(c.title);
					updates.push({ title: c.title, description: text });
				} catch (_) {
					// ignore
					attemptedDescriptions.add(c.title);
				} finally {
					inflightDescriptions.delete(c.title);
				}
			}
		}
		await Promise.all(
			new Array(Math.min(limit, candidates.length)).fill(0).map(() => worker()),
		);
		applyBatch();
		visibleFirstRunning = false;
	}

	async function fillCachedDescriptions(mods: Mod[]) {
		// Only reads local cache; no network. Gentle concurrency.
		const limit = 12;
		let i = 0;
		const updates: { title: string; description: string }[] = [];
		const applyBatch = () => {
			if (updates.length === 0) return;
			const batch = updates.splice(0, updates.length);
			setDescriptions(
				Object.fromEntries(batch.map((b) => [b.title, b.description])),
			);
		};
		async function worker() {
			while (true) {
				const idx = i++;
				if (idx >= mods.length) break;
				const m = mods[idx];
				if (!m || (m.description && m.description.trim().length > 0)) continue;
				try {
					const cached = await invoke<string | null>(
						"get_cached_description_by_title",
						{ title: m.title },
					);
                        if (cached) {
							updates.push({ title: m.title, description: cached });
                        }
				} catch (_) {
					// ignore
				}
			}
		}
		await Promise.all(
			new Array(Math.min(limit, mods.length)).fill(0).map(() => worker()),
		);
		applyBatch();
	}

	async function fillCachedDescriptionsVisibleFirst() {
		if (isUserScrolling) {
			hydrationPending = true;
			return;
		}
		const candidates = paginatedMods
			.filter((m) => !m.description || m.description.trim().length === 0)
			.map((m) => m.title);
		if (candidates.length === 0) return;
		const limit = 8;
		let i = 0;
		const updates: { title: string; description: string }[] = [];
		const applyBatch = () => {
			if (updates.length === 0) return;
			const batch = updates.splice(0, updates.length);
			setDescriptions(
				Object.fromEntries(batch.map((b) => [b.title, b.description])),
			);
		};
		async function worker() {
			while (true) {
				const idx = i++;
				if (idx >= candidates.length) break;
				const title = candidates[idx]!;
				if (attemptedCacheTitles.has(title)) continue;
				try {
					attemptedCacheTitles.add(title);
					const cached = await invoke<string | null>(
						"get_cached_description_by_title",
						{ title },
					);
                        if (cached) {
							updates.push({ title, description: cached });
                        }
				} catch (_) {
					// ignore
				}
			}
		}
		await Promise.all(
			new Array(Math.min(limit, candidates.length)).fill(0).map(() => worker()),
		);
		applyBatch();
	}

	async function fillInstalledThumbnails(mods: Mod[]) {
		const limit = 8;
		let i = 0;
		const client = async () => {
			while (true) {
				const idx = i++;
				if (idx >= mods.length) break;
				const m = mods[idx];
				if (!m) continue;
				if (!$installationStatus[m.title]) continue; // only for installed mods
				const dir = m._dirName as string | undefined;
				if (!dir) continue;
				try {
					const dataUrl = await invoke<string | null>(
						"get_cached_installed_thumbnail",
						{ title: m.title, dirName: dir },
					);
					if (dataUrl) {
						const resolved =
							dataUrl.startsWith("data:") || dataUrl.startsWith("http")
								? dataUrl
								: convertFileSrc(dataUrl);
						modsStore.update((arr) => {
                            const pos = arr.findIndex((x) => x.title === m.title);
                            if (pos >= 0) {
                                arr = arr.slice();
                                arr[pos] = { ...arr[pos], image: resolved, imageFallback: undefined };
                            }
							return arr;
						});
					}
				} catch (_) {
					// ignore per-mod failures
				}
			}
		};
		await Promise.all(
			new Array(Math.min(limit, mods.length)).fill(0).map(() => client()),
		);
	}

	function mergeIncomingMods(incomingMods: Mod[]): number {
		let prunedCount = 0;
		modsStore.update((arr) => {
			const incoming = new Map<string, Mod>();
			for (const m of incomingMods) incoming.set(m.title, m);
			const incomingOrder = incomingMods.map((m) => m.title);
			const existingRemoteCount = arr.reduce(
				(n, it) => n + (it._dirName ? 1 : 0),
				0,
			);
			const pruneAllowed =
				incoming.size > 0 &&
				incoming.size >= Math.max(10, Math.floor(existingRemoteCount * 0.5));
			const existingByTitle = new Map<string, Mod>(
				arr.map((m) => [m.title, m]),
			);
			const out: Mod[] = [];

			for (const title of incomingOrder) {
				const inc = incoming.get(title);
				if (!inc) continue;
				const existing = existingByTitle.get(title);
				if (existing) {
					const keepExistingImage =
						Boolean(existing.image) &&
						existing.image.trim().length > 0 &&
						!/\bimages\/cover\.jpg$/i.test(existing.image.trim());
					const preferExistingDesc =
						(existing.description?.trim().length ?? 0) > 0;
					out.push({
						...existing,
						...inc,
						requires_steamodded:
							existing.requires_steamodded || inc.requires_steamodded,
						requires_talisman:
							existing.requires_talisman || inc.requires_talisman,
						description: preferExistingDesc
							? existing.description
							: (inc.description ?? ""),
						colors: existing.colors,
						image: keepExistingImage ? existing.image : inc.image,
						imageFallback: keepExistingImage
							? existing.imageFallback
							: inc.imageFallback,
					});
				} else {
					out.push(inc);
				}
			}

			for (const existing of arr) {
				if (incoming.has(existing.title)) continue;
				if (!existing._dirName) {
					out.push(existing);
				} else if (pruneAllowed) {
					prunedCount++;
				} else {
					out.push(existing);
				}
			}
			return out;
		});
		return prunedCount;
	}

	async function applyCachedThumbnails(titles: string[]) {
		if (!titles.length) return new Set<string>();
		try {
			const cachedMap = await invoke<Record<string, string>>(
				"get_cached_thumbnails_map",
				{ titles },
			);
			if (!cachedMap || Object.keys(cachedMap).length === 0) {
				return new Set<string>();
			}
			modsStore.update((arr) =>
				arr.map((m) => {
					const p = cachedMap[m.title];
					if (!p) return m;
					const src = convertFileSrc(p);
					return { ...m, image: src, imageFallback: src };
				}),
			);
			return new Set<string>(Object.keys(cachedMap));
		} catch (_) {
			/* ignore */
		}
		return new Set<string>();
	}

	function scheduleThumbCacheRefresh(titles: string[]) {
		if (!titles.length) return;
		if (thumbRefreshTimer !== null) {
			clearTimeout(thumbRefreshTimer);
			thumbRefreshTimer = null;
		}
		thumbRefreshAttempts = 0;
		const needsThumb = new Set(
			$modsStore.filter((m) => m._hasThumbnail).map((m) => m.title),
		);
		const pendingTitles = titles.filter((t) => needsThumb.has(t));
		if (pendingTitles.length === 0) return;

		const poll = async () => {
			thumbRefreshAttempts += 1;
			let resolved = new Set<string>();
			try {
				resolved = await applyCachedThumbnails(pendingTitles);
			} catch (_) {
				// ignore
			}
			if (resolved.size > 0) {
				for (const title of resolved) {
					needsThumb.delete(title);
				}
			}
			if (needsThumb.size === 0) {
				thumbRefreshTimer = null;
				return;
			}
			if (thumbRefreshAttempts >= 12) {
				thumbRefreshTimer = null;
				return;
			}
			thumbRefreshTimer = window.setTimeout(
				poll,
				thumbRefreshAttempts <= 4 ? 2000 : 5000,
			);
		};
		thumbRefreshTimer = window.setTimeout(poll, 1500);
	}

	async function seedInstalledPlaceholders() {
		try {
			// Load installed mods quickly from DB cache helper
			installedMods = await fetchCachedMods();
			if (!installedMods || installedMods.length === 0) return;
			modsStore.update((arr) => {
				const existingTitles = new Set(arr.map((m) => m.title));
				const additions: Mod[] = installedMods
					.filter((m) => !existingTitles.has(m.name))
					.map(
						(m) =>
							({
								title: m.name,
								description: "",
								image: "/images/cover.jpg",
								colors: getRandomColorPair(),
								categories: [],
								requires_steamodded: false,
								requires_talisman: false,
								publisher: "Installed",
								repo: "",
								downloadURL: "",
								folderName: m.name,
								version: "",
								installed: true,
								last_updated: 0,
								_hasThumbnail: false,
								// Keep private installed path for potential future local reads
								_installedPath: m.path,
							}),
					);
				return additions.length ? [...additions, ...arr] : arr;
			});

			// Immediately reflect installationStatus so filters show
			for (const m of installedMods) {
				installationStatus.update((s) => ({ ...s, [m.name]: true }));
			}
		} catch (e) {
			console.warn("seedInstalledPlaceholders failed", e);
		}
	}

	// No local clone or pull; we lazy-load from the repo instead.

	const categories = [
		{ name: "Installed Mods", icon: Download },
		{ name: "Search", icon: Search },
		{ name: "Collections", icon: Layers },
		{ name: "All Mods", icon: LayoutDashboard },
		{ name: "Content", icon: FolderHeart },
		{ name: "Miscellaneous", icon: BookOpen },
		{ name: "Joker", icon: Flame },
		{ name: "Quality of Life", icon: Star },
		{ name: "Technical", icon: Spade },
		{ name: "Resource Packs", icon: FolderHeart },
		{ name: "API", icon: Gamepad2 },
	];

	const colorPairs = [
		{ color1: "#4f6367", color2: "#425556" },
		{ color1: "#AA778D", color2: "#906577" },
		{ color1: "#A2615E", color2: "#89534F" },
		{ color1: "#A48447", color2: "#8B703C" },
		{ color1: "#4F7869", color2: "#436659" },
		{ color1: "#728DBF", color2: "#6177A3" },
		{ color1: "#5D5E8F", color2: "#4F4F78" },
		{ color1: "#796E9E", color2: "#655D86" },
		{ color1: "#64825D", color2: "#556E4E" },
		{ color1: "#86A367", color2: "#728A57" },
		{ color1: "#748C8A", color2: "#627775" },
	];

	const categoryMap: Record<string, Category> = {
		Content: Category.Content,
		content: Category.Content,
		Joker: Category.Joker,
		joker: Category.Joker,
		"Quality of Life": Category.QualityOfLife,
		"quality of life": Category.QualityOfLife,
		Technical: Category.Technical,
		technical: Category.Technical,
		Miscellaneous: Category.Miscellaneous,
		miscellaneous: Category.Miscellaneous,
		"Resource Packs": Category.ResourcePacks,
		"resource packs": Category.ResourcePacks,
		Resources: Category.ResourcePacks,
		resources: Category.ResourcePacks,
		API: Category.API,
		api: Category.API,
	};

	function getRandomColorPair() {
		return colorPairs[Math.floor(Math.random() * colorPairs.length)];
	}

	function handleModClick(mod: Mod) {
		currentModView.set(mod);
	}

	let showSearch = $derived($currentCategory === "Search");
	$currentCategory = "All Mods";

	let filteredMods = $derived($modsStore.filter((mod) => {
		switch ($currentCategory) {
			case "Content":
				return (
					mod.categories.includes(Category.Content) ||
					mod.categories.some((cat) => cat === 0) || // Assuming Content is enum value 0
					mod.title.toLowerCase().includes("content") ||
					(typeof mod.description === "string" &&
						mod.description.toLowerCase().includes("new content"))
				);
			case "Joker":
				return (
					mod.categories.includes(Category.Joker) ||
					mod.categories.some((cat) => cat === 1)
				);
			case "Quality of Life":
				return (
					mod.categories.includes(Category.QualityOfLife) ||
					mod.categories.some((cat) => cat === 2)
				);
			case "Technical":
				return (
					mod.categories.includes(Category.Technical) ||
					mod.categories.some((cat) => cat === 3)
				);
			case "Resource Packs":
				return (
					mod.categories.includes(Category.ResourcePacks) ||
					mod.categories.some((cat) => cat === 5)
				);
			case "API":
				return (
					mod.categories.includes(Category.API) ||
					mod.categories.some((cat) => cat === 6)
				);
			case "Miscellaneous":
				return (
					mod.categories.includes(Category.Miscellaneous) ||
					mod.categories.some((cat) => cat === 4)
				);
			case "Installed Mods":
				return Boolean($installationStatus[mod.title]);
			case "Collections":
				return true;
			default:
				return true;
		}
	}));

	let selectedCollection = $derived(
		selectedCollectionId
			? $collectionsStore.find((c) => c.id === selectedCollectionId) ?? null
			: null,
	);
	let selectedCollectionMods = $derived(
		selectedCollection
			? (() => {
					const wantedIds = new Set(selectedCollection.modIds ?? []);
					const wanted = new Set(
						selectedCollection.modTitles.map((t) =>
							normalizeCollectionTitle(t),
						),
					);
					return $modsStore.filter((m) => {
						if (m.id && wantedIds.has(m.id)) return true;
						return wanted.has(normalizeCollectionTitle(m.title));
					});
			  })()
			: [],
	);

	$effect(() => {
		if ($currentCategory !== "Collections") return;
		const list = $collectionsStore;
		if (!list || list.length === 0) {
			selectedCollectionId = null;
			return;
		}
		if (selectedCollectionId && list.some((c) => c.id === selectedCollectionId)) {
			return;
		}
		const preferred =
			$activeCollectionId && list.some((c) => c.id === $activeCollectionId)
				? $activeCollectionId
				: list[0].id;
		selectedCollectionId = preferred;
	});

	function handleCategoryClick(category: string) {
		currentPage.set(1);
		startPage = 1; // Reset sliding window
		currentCategory.set(category);
		scrollToTop();
		markPaginating();
		updateVirtualWindow();
	}

	function handleModsScroll() {
		isUserScrolling = true;
		if (scrollIdleTimer) clearTimeout(scrollIdleTimer);
		const delay = isLinux ? 240 : 160;
		scrollIdleTimer = window.setTimeout(() => {
			isUserScrolling = false;
			if (hydrationPending) scheduleHydration();
		}, delay);
	}

// Safely register global click handler with cleanup to avoid duplicates
let globalClickHandler: ((e: MouseEvent) => void) | null = null;
	onMount(() => {
	  globalClickHandler = (e: MouseEvent) => {
	    const target = e.target as HTMLElement;
	    const anchor = target?.closest?.("a");
	    if (anchor && anchor instanceof HTMLAnchorElement && anchor.href.startsWith("https://")) {
	      e.preventDefault();
	      openExternal(anchor.href);
	    }
	  };
	  document.addEventListener("click", globalClickHandler);
	});
onDestroy(() => {
  if (globalClickHandler) {
    document.removeEventListener("click", globalClickHandler);
    globalClickHandler = null;
  }
});

	function sortMods(mods: Mod[], _sortOption: SortOption): Mod[] {
		// Sorting is provided by the server; preserve incoming order.
		return mods;
	}

	// Add sort handler
	function handleSortChange(event: Event) {
		const select = event.target as HTMLSelectElement;
		currentSort.set(select.value as SortOption);
		// Derived values react to $currentSort; no manual assignment needed
		// Reset to first page when sort changes to prevent out-of-bounds issues
		if ($currentPage > 1) {
			currentPage.set(1);
			startPage = 1;
		}
		refreshCatalogInBackground(false).catch(() => {});
	}

	function handleCreateCollection() {
		const result = createCollection(newCollectionName);
		if (!result.ok) {
			addMessage(result.error || "Failed to create collection.", "error");
			return;
		}
		newCollectionName = "";
		if (result.id) {
			selectedCollectionId = result.id;
		}
	}

	function startRenameCollection(id: string, name: string) {
		renamingId = id;
		renameValue = name;
	}

	function cancelRenameCollection() {
		renamingId = null;
		renameValue = "";
	}

	function saveRenameCollection(id: string) {
		const result = renameCollection(id, renameValue);
		if (!result.ok) {
			addMessage(result.error || "Failed to rename collection.", "error");
			return;
		}
		cancelRenameCollection();
	}

	function handleDeleteCollection(id: string) {
		deleteCollection(id);
		if (selectedCollectionId === id) {
			selectedCollectionId = null;
		}
	}

	async function handleShareCollection(id: string) {
		const result = exportCollectionCode(id);
		if (!result.ok || !result.code) {
			addMessage(result.error || "Failed to generate share code.", "error");
			return;
		}
		try {
			await navigator.clipboard.writeText(result.code);
			addMessage("Collection code copied.", "success");
		} catch {
			setCollectionImportCode(result.code);
			openCollectionImport(result.code);
			addMessage("Copy failed. Code shown for manual copy.", "warning");
		}
	}

	$effect(() => {
		if ($lastImportedCollectionId) {
			selectedCollectionId = $lastImportedCollectionId;
			lastImportedCollectionId.set(null);
		}
	});


	async function activateCollection(id: string) {
		if (collectionBusy) return;
		const collection = $collectionsStore.find((c) => c.id === id);
		if (!collection) return;
		collectionBusy = id;
		try {
			const normalizeName = (name: string) =>
				name.toLowerCase().replace(/[^a-z0-9+]+/g, "").trim();
			const localPaths = localMods.map((m) => m.path);

			const normalizeInstalled = (map: Record<string, boolean>) =>
				new Set(Object.keys(map).map((n) => normalizeName(n)));

			const modsByNormalized = new Map(
				get(modsStore).map((m) => [normalizeName(m.title), m]),
			);
			const modsById = new Map(
				get(modsStore)
					.filter((m) => m.id)
					.map((m) => [m.id as string, m]),
			);
			const repoModsByNormalized = new Map<string, Mod>();
			const repoModsById = new Map<string, Mod>();
			let repoModsLoaded = false;
			const ensureRepoMods = async () => {
				if (repoModsLoaded) return;
				const items = await invoke<ArchiveModItem[]>("fetch_repo_mods", {
					sort: get(currentSort),
				});
				const mapped = mapArchiveItems(items);
				repoModsByNormalized.clear();
				repoModsById.clear();
				for (const mod of mapped) {
					repoModsByNormalized.set(normalizeName(mod.title), mod);
					if (mod.id) repoModsById.set(mod.id, mod);
				}
				repoModsLoaded = true;
			};

			let enabledMap = await invoke<Record<string, boolean>>(
				"enabled_state_map",
				{ localPaths },
			);
			let installedNormalized = normalizeInstalled(enabledMap);

			const desiredTitles: string[] = [...collection.modTitles];
			const desiredIds = new Set(collection.modIds ?? []);
			const desiredNormalized = new Set(
				desiredTitles.map((t) => normalizeName(t)),
			);
			const addDesiredTitle = (title: string) => {
				const normalized = normalizeName(title);
				if (desiredNormalized.has(normalized)) return;
				desiredNormalized.add(normalized);
				desiredTitles.push(title);
			};
			const addDesiredMod = (mod: Mod) => {
				addDesiredTitle(mod.title);
				if (mod.id) desiredIds.add(mod.id);
			};

			const resolvedForDeps: Mod[] = [];
			const resolvedKeys = new Set<string>();
			const addResolved = (mod: Mod) => {
				const key = mod.id ?? normalizeName(mod.title);
				if (resolvedKeys.has(key)) return;
				resolvedKeys.add(key);
				resolvedForDeps.push(mod);
			};

			const missing: Mod[] = [];
			const missingUnresolved: string[] = [];
			const missingUnresolvedIds: string[] = [];

			for (const id of collection.modIds ?? []) {
				let match = modsById.get(id);
				if (!match) {
					await ensureRepoMods();
					match = repoModsById.get(id);
					if (!match) {
						match = repoModsByNormalized.get(normalizeName(id));
					}
				}
				if (match) {
					addDesiredMod(match);
					addResolved(match);
				} else {
					missingUnresolvedIds.push(id);
				}
			}

			for (const title of collection.modTitles) {
				const normalized = normalizeName(title);
				let match = modsByNormalized.get(normalized);
				if (!match) {
					await ensureRepoMods();
					match = repoModsByNormalized.get(normalized);
				}
				if (match) {
					addDesiredMod(match);
					addResolved(match);
				}
			}

			for (const match of resolvedForDeps) {
				if (match.requires_steamodded) addDesiredTitle("Steamodded");
				if (match.requires_talisman) addDesiredTitle("Talisman");
			}

			const ensureDownloadUrl = (m: Mod): Mod => {
				if (m.downloadURL && m.downloadURL.trim().length > 0) return m;
				if (m._dirName) {
					return { ...m, downloadURL: `bmi://${m._dirName}` };
				}
				return m;
			};
			for (const title of desiredTitles) {
				const normalized = normalizeName(title);
				if (installedNormalized.has(normalized)) continue;
				let match = modsByNormalized.get(normalized);
				if (!match || !match.downloadURL) {
					await ensureRepoMods();
					match = repoModsByNormalized.get(normalized);
				}
				if (match) {
					const withUrl = ensureDownloadUrl(match);
					if (withUrl.downloadURL) {
						missing.push(withUrl);
					} else {
						missingUnresolved.push(title);
					}
				} else {
					missingUnresolved.push(title);
				}
			}

			const priority = (title: string) => {
				const normalized = normalizeName(title);
				if (normalized === "steamodded") return 0;
				if (normalized === "talisman") return 1;
				return 2;
			};
			missing.sort((a, b) => priority(a.title) - priority(b.title));

			const installModSilently = async (mod: Mod) => {
				if (!mod?.title || !mod?.downloadURL) return;
				loadingStates2.update((s) => ({ ...s, [mod.title]: true }));
				const folderName =
					mod.folderName || mod.title.replace(/\s+/g, "");
				const dependencies: string[] = [];
				if (mod.requires_steamodded) dependencies.push("Steamodded");
				if (mod.requires_talisman) dependencies.push("Talisman");
				try {
					const installedPath = await invoke<string>("install_mod", {
						url: mod.downloadURL,
						folderName,
					});
					await invoke("add_installed_mod", {
						name: mod.title,
						path: installedPath,
						dependencies,
						currentVersion: mod.version || "",
					});
					installationStatus.update((s) => ({
						...s,
						[mod.title]: true,
					}));
					updateAvailableStore.update((s) => ({
						...s,
						[mod.title]: false,
					}));
				} finally {
					loadingStates2.update((s) => ({ ...s, [mod.title]: false }));
				}
			};

			for (const mod of missing) {
				try {
					await installModSilently(mod);
				} catch (error) {
					addMessage(
						`Failed to install ${mod.title}: ${
							error instanceof Error ? error.message : String(error)
						}`,
						"error",
					);
				}
			}
			if (missingUnresolved.length > 0 || missingUnresolvedIds.length > 0) {
				const totalMissing = missingUnresolved.length + missingUnresolvedIds.length;
				addMessage(
					`Missing ${totalMissing} mod(s) in the catalog. Install them manually to include in this collection.`,
					"warning",
				);
			}

			await refreshInstalledMods();
			enabledMap = await invoke<Record<string, boolean>>(
				"enabled_state_map",
				{ localPaths },
			);
			const wanted = new Set(desiredTitles.map((t) => normalizeName(t)));
			const toEnable: string[] = [];
			const toDisable: string[] = [];
			const installedNames = Object.keys(enabledMap);
			let installedInCollection = 0;
			for (const name of installedNames) {
				const shouldEnable = wanted.has(normalizeName(name));
				if (shouldEnable) installedInCollection += 1;
				const current = enabledMap[name];
				if (current !== shouldEnable) {
					(shouldEnable ? toEnable : toDisable).push(name);
				}
			}
			if (installedNames.length === 0) {
				addMessage("No installed mods found to activate.", "info");
				return;
			}
			if (toEnable.length === 0 && toDisable.length === 0) {
				setActiveCollection(id);
				addMessage("Collection already active.", "info");
				return;
			}
			await invoke("toggle_mods_enabled_batch", {
				enabled: toEnable,
				disabled: toDisable,
				localPaths,
			});
			modEnabledStore.update((map) => {
				const next = { ...map };
				for (const name of toEnable) next[name] = true;
				for (const name of toDisable) next[name] = false;
				return next;
			});
			setActiveCollection(id);
			addMessage(`Activated "${collection.name}".`, "success");
			await refreshStateSummary();
		} catch (error) {
			addMessage(
				`Failed to activate collection: ${
					error instanceof Error ? error.message : String(error)
				}`,
				"error",
			);
		} finally {
			collectionBusy = null;
		}
	}

	async function deactivateCollection(id: string) {
		if (collectionBusy) return;
		if ($activeCollectionId !== id) return;
		const collection = $collectionsStore.find((c) => c.id === id);
		if (!collection) return;
		collectionBusy = id;
		try {
			const normalizeName = (name: string) =>
				name.toLowerCase().replace(/[^a-z0-9+]+/g, "").trim();
			const localPaths = localMods.map((m) => m.path);
			const enabledMap = await invoke<Record<string, boolean>>(
				"enabled_state_map",
				{ localPaths },
			);
			const wanted = new Set(
				collection.modTitles.map((t) => normalizeName(t)),
			);
			for (const modId of collection.modIds ?? []) {
				const match = get(modsStore).find((m) => m.id === modId);
				if (match) {
					wanted.add(normalizeName(match.title));
				} else {
					wanted.add(normalizeName(modId));
				}
			}

			const toDisable = Object.entries(enabledMap)
				.filter(([name, enabled]) => enabled && wanted.has(normalizeName(name)))
				.map(([name]) => name);
			if (toDisable.length > 0) {
				await invoke("toggle_mods_enabled_batch", {
					enabled: [],
					disabled: toDisable,
					localPaths,
				});
				modEnabledStore.update((map) => {
					const next = { ...map };
					for (const name of toDisable) next[name] = false;
					return next;
				});
			}
			setActiveCollection(null);
			addMessage("Collection deactivated.", "info");
			await refreshStateSummary();
		} catch (error) {
			addMessage(
				`Failed to deactivate collection: ${
					error instanceof Error ? error.message : String(error)
				}`,
				"error",
			);
		} finally {
			collectionBusy = null;
		}
	}

    let sortedAndFilteredMods = $derived(sortMods(filteredMods, $currentSort));

    $effect(() => {
        // touch dependencies so effect runs when these change
        sortedAndFilteredMods;
        paginatedMods;
        if ($currentCategory === "Installed Mods") {
            updateEnabledDisabledLists();
        }
    });

    let totalPages = $derived(Math.ceil(sortedAndFilteredMods.length / $itemsPerPage));
	let paginatedMods = $derived(
        sortedAndFilteredMods.slice(
            ($currentPage - 1) * $itemsPerPage,
            $currentPage * $itemsPerPage,
        )
    );

	let visiblePaginatedMods: Mod[] = $state([]);

	function updateVirtualWindow() {
		visiblePaginatedMods = paginatedMods;
	}

// Whenever the visible page changes, try to quickly hydrate from cache and recalc window
$effect(() => {
    // touch dependency
    paginatedMods;
    const localMax = Math.max(
        enabledLocalMods.length,
        disabledLocalMods.length,
    );
    renderLimitLocal = Math.min(60, localMax || 60);
    if (observerLocal && localSentinel) observerLocal.observe(localSentinel);
    updateVirtualWindow();
    scheduleHydration();
});

onDestroy(() => {
    if (visibleHydrateTimer !== null) {
        clearTimeout(visibleHydrateTimer);
        visibleHydrateTimer = null;
    }
});

	const maxVisiblePages = 5;
	let startPage = $state(1);

	let visibleEnabledLocal = $derived(
		enabledLocalMods.slice(0, renderLimitLocal),
	);
	let visibleDisabledLocal = $derived(
		disabledLocalMods.slice(0, renderLimitLocal),
	);
	let observerLocal: IntersectionObserver | null = null;

	function scheduleHydration() {
		hydrationPending = true;
		if (paginating || isUserScrolling) return;
		if (visibleHydrateTimer !== null) {
			clearTimeout(visibleHydrateTimer);
		}
		const delay = isLinux ? 160 : 120;
		visibleHydrateTimer = setTimeout(() => {
			hydrationPending = false;
			void withDescriptionsPersistenceSuspended(async () => {
				await fillCachedDescriptionsVisibleFirst().catch(() => {});
				if (!visibleFirstRunning) {
					await fillDescriptionsVisibleFirst().catch(() => {});
				}
			});
		}, delay) as unknown as number;
	}

	function updatePaginationWindow() {
		if ($currentPage > startPage + maxVisiblePages - 1) {
			startPage = $currentPage - maxVisiblePages + 1;
		} else if ($currentPage < startPage) {
			startPage = $currentPage;
		}
	}

	let lastPage = $state<number | null>(null);
	$effect(() => {
		const page = $currentPage;
		updatePaginationWindow();
		if (lastPage === null) {
			lastPage = page;
			return;
		}
		if (page !== lastPage) {
			lastPage = page;
			updatePaginationWindow();
			scrollToTop();
			markPaginating();
		}
	});

	$effect(() => {
		paginationWindow.set({
			startPage,
			totalPages,
			maxVisiblePages,
		});
	});

	async function refreshInstalledMods() {
		try {
			await forceRefreshCache();
			await refreshStateSummary();
		} catch (error) {
			console.error("Failed to refresh installed mods:", error);
		}
	}

	async function refreshStateSummary() {
		try {
			const localPaths = localMods.map((m) => m.path);
			const summary = await invoke<{
				installed: { name: string; path: string }[];
				enabled: Record<string, boolean>;
				updates: Record<string, boolean>;
				thumbnails: Record<string, string>;
				descriptions: Record<string, string>;
			}>("mods_state_summary", {
				localPaths,
				catalogTitles: paginatedMods.map((m) => m.title),
			});

			installedMods = summary.installed.map((m) => ({
				name: m.name,
				path: m.path,
			}));

			const installedSet = new Set(
				summary.installed.map((m) => m.name.toLowerCase()),
			);
			installationStatus.set(
				Object.fromEntries(
					$modsStore.map((mod) => [
						mod.title,
						installedSet.has(mod.title.toLowerCase()),
					]),
				),
			);
			modEnabledStore.set(summary.enabled || {});
			updateAvailableStore.set(summary.updates || {});
			// Apply cached thumbnails for installed mods
			const thumbMap = summary.thumbnails || {};
			if (Object.keys(thumbMap).length > 0) {
				modsStore.update((arr) =>
					arr.map((m) => {
						const p = thumbMap[m.title];
						if (p) {
							return {
								...m,
								image: convertFileSrc(p),
								imageFallback: convertFileSrc(p),
							};
						}
						return m;
					}),
				);
			}
			if (summary.descriptions && Object.keys(summary.descriptions).length > 0) {
				setDescriptions(summary.descriptions);
			}
			updateEnabledDisabledLists();
		} catch (error) {
			console.warn("Failed to refresh state summary:", error);
		}
	} 

	async function openModsFolder() {
		try {
			const modsPath: string = await invoke("get_mods_folder");
			await invoke("open_directory", { path: modsPath });
		} catch (error) {
			addMessage(`Failed to open mods directory: ${error}`, "error");
		}
	}

	let prevCategory = "";
	$effect(() => {
		const cat = $currentCategory;
		if (
			$currentModView === null &&
			cat === "Installed Mods" &&
			prevCategory !== "Installed Mods"
		) {
			// Category just switched to Installed Mods
			refreshInstalledMods();
			if (sortedAndFilteredMods.length === 0) {
				seedInstalledPlaceholders();
			}
		}
		prevCategory = cat;
	});

	$effect(() => {
		if (
			$modEnabledStore &&
			Object.keys($modEnabledStore).length > 0 &&
			$currentCategory === "Installed Mods"
		) {
			updateEnabledDisabledLists();
		}
	});
</script>

<div class="container default-scrollbar">
	<div class="mods-container">
		<div class="categories">
		{#each categories as category}
				{@const Icon = category.icon}
				<button
					class:active={$currentCategory === category.name}
					onclick={() => handleCategoryClick(category.name)}
				>
					<Icon size={16} />
					{category.name}
				</button>
			{/each}
		</div>

		<div class="separator"></div>

		{#if ($modsStore.length === 0 && (isLoading || $catalogLoading || catalogRetryPending)) && $currentCategory !== "Installed Mods"}
			<div class="loading-container">
				<p class="loading-text">
					Loading mods{".".repeat($loadingDots)}
				</p>
			</div>
		{:else if showSearch}
			{#if SearchViewComp}
				<SearchViewComp onCheckDependencies={handleDependencyCheck} />
			{:else}
				<div class="loading-container">
					<p class="loading-text">Loading search…</p>
				</div>
			{/if}
		{:else if $currentCategory === "Collections"}
			<div class="mods-wrapper">
				<div class="collections-shell">
					<div class="collections-left default-scrollbar">
						<div class="collections-create">
							<input
								class="text-input"
								type="text"
								placeholder="Collection name"
								bind:value={newCollectionName}
								onkeydown={(e) => e.key === "Enter" && handleCreateCollection()}
							/>
							<button class="add-button" onclick={handleCreateCollection}>
								+
							</button>
						</div>
						<div class="collections-import">
							<button class="ghost import" type="button" onclick={() => openCollectionImport()}>
								Import code
							</button>
						</div>

						{#if $collectionsStore.length === 0}
							<div class="collections-empty">
								No collections yet. Create one to get started.
							</div>
						{:else}
							<div class="collections-list">
								{#each $collectionsStore as col (col.id)}
									<div class="collection-row">
										{#if renamingId === col.id}
											<input
												class="text-input rename-input"
												type="text"
												bind:value={renameValue}
												onkeydown={(e) =>
													e.key === "Enter" && saveRenameCollection(col.id)}
											/>
										{:else}
											<button
												type="button"
												class="collection-item"
												class:active={selectedCollectionId === col.id}
												onclick={() => (selectedCollectionId = col.id)}
											>
												{col.name}
											</button>
										{/if}
										<div class="collection-actions">
											{#if renamingId === col.id}
												<button class="ghost confirm" onclick={() => saveRenameCollection(col.id)}>
													Save
												</button>
												<button class="ghost neutral" onclick={cancelRenameCollection}>
													Cancel
												</button>
											{:else}
												<button class="ghost rename" onclick={() => startRenameCollection(col.id, col.name)}>
													Rename
												</button>
												<button class="ghost share" onclick={() => handleShareCollection(col.id)}>
													Share
												</button>
												<button class="ghost delete" onclick={() => handleDeleteCollection(col.id)}>
													Delete
												</button>
											{/if}
											<button
												class="toggle-collection"
												class:enabled={$activeCollectionId === col.id}
												disabled={collectionBusy === col.id}
												onclick={() =>
													$activeCollectionId === col.id
														? deactivateCollection(col.id)
														: activateCollection(col.id)}
											>
												{#if collectionBusy === col.id}
													Loading{".".repeat($loadingDots)}
												{:else}
													{$activeCollectionId === col.id ? "On" : "Off"}
												{/if}
											</button>
										</div>
									</div>
								{/each}
							</div>
						{/if}
					</div>

					<div class="collections-separator"></div>

					<div class="collections-right default-scrollbar">
						{#if selectedCollection}
							<div class="collections-mods">
								{#if selectedCollectionMods.length === 0}
									<div class="collections-empty">
										This collection has no mods yet.
									</div>
								{:else}
									<div class="mods-grid collections-mods-grid">
										{#each selectedCollectionMods as mod (mod.title)}
											<div class="virtual-cell">
												<ModCard
													{mod}
													deferImages={paginating}
													hideDescription={true}
													disableInstall={$activeCollectionId === selectedCollectionId || collectionBusy === selectedCollectionId}
													onmodclick={handleModClick}
													oninstallclick={installMod}
													onuninstallclick={uninstallMod}
													onToggleEnabled={handleModToggled}
												/>
											</div>
										{/each}
									</div>
								{/if}
							</div>
						{:else}
							<div class="collections-empty">
								Select a collection to view its mods.
							</div>
						{/if}
					</div>

				</div>
			</div>
		{:else}
			<div class="mods-wrapper">
				<div class="controls-container">
					{#if $currentCategory === "Installed Mods" && !$currentModView}
						<button
							class="folder-icon-button"
							onclick={openModsFolder}
							title="Open Mods Folder"
							in:fly={{ duration: 400, y: 10, opacity: 0.2 }}
						>
							<Folder size={20} />
						</button>

						{#if hasUpdatesAvailable}
							<button
								class="update-all-button-top"
								onclick={updateAllMods}
								title="Update all mods with available updates"
								in:fly={{ duration: 400, y: 10, opacity: 0.2 }}
							>
								<RefreshCw size={18} /> <span>Update All</span>
							</button>
						{/if}
					{/if}

					<div
						class="sort-controls"
						in:fly={{ duration: 400, y: 10, opacity: 0.2 }}
					>
						<div class="sort-wrapper">
							<ArrowUpDown size={16} />
							<select
								value={$currentSort}
								onchange={handleSortChange}
							>
								<option value={SortOption.NameAsc}
									>Name (A-Z)</option
								>
								<option value={SortOption.NameDesc}
									>Name (Z-A)</option
								>
								<option value={SortOption.LastUpdatedDesc}
									>Last Updated</option
								>
								<option value={SortOption.LastUpdatedAsc}
									>Oldest Updated</option
								>
								<option value={SortOption.DownloadsDesc}
									>Downloads (Most)</option
								>
								<option value={SortOption.DownloadsAsc}
									>Downloads (Least)</option
								>
							</select>
						</div>
					</div>
				</div>

				<div
					class="mods-scroll-container default-scrollbar"
					class:no-local-mods={localMods.length === 0}
					bind:this={modsScrollContainer}
					onscroll={handleModsScroll}
				>
					{#if $currentCategory === "Installed Mods"}
						{#if isLoadingLocalMods}
							<div class="section-header">
								<h3>Local Mods</h3>
								<p>
									Loading local mods{".".repeat($loadingDots)}
								</p>
							</div>
						{:else if localMods.length > 0}
							<div class="section-header">
								<div class="section-header-content">
									<h3>Local Mods</h3>
									<p>
										These mods were installed manually
										(outside the mod manager)
									</p>
								</div>
								<!-- Removed Open Mods Folder button for Local Mods section -->
							</div>

							<!-- Enabled Local Mods -->
							{#if enabledLocalMods.length > 0}
								<div
									class="subsection-header enabled"
									class:top-margin={localMods.length === 0}
								>
									<h4>Enabled Local Mods</h4>
									<p>
										{enabledLocalMods.length} mod{enabledLocalMods.length !==
										1
											? "s"
											: ""} active
									</p>
								</div>
								<div class="mods-grid local-mods-grid">
									{#each visibleEnabledLocal as mod (mod.name)}
										<LocalModCard
											{mod}
											onUninstall={handleModUninstalled}
											onToggleEnabled={handleModToggled}
										/>
									{/each}
									<div
										bind:this={localSentinel}
										class="render-sentinel"
										aria-hidden="true"
									></div>
								</div>
							{/if}

							<!-- Disabled Local Mods -->
							{#if disabledLocalMods.length > 0}
								<div
									class="subsection-header disabled"
									class:top-margin={localMods.length === 0}
								>
									<h4>Disabled Local Mods</h4>
									<p>
										{disabledLocalMods.length} mod{disabledLocalMods.length !==
										1
											? "s"
											: ""} inactive
									</p>
								</div>
								<div class="mods-grid local-mods-grid">
									{#each visibleDisabledLocal as mod (mod.name)}
										<LocalModCard
											{mod}
											onUninstall={handleModUninstalled}
											onToggleEnabled={handleModToggled}
										/>
									{/each}
									<div
										bind:this={localSentinel}
										class="render-sentinel"
										aria-hidden="true"
									></div>
								</div>
							{/if}

							<!-- Mod Manager Catalog Section Header -->
							<div class="section-header">
								<div class="section-header-content">
									<h3>Mod Manager Catalog</h3>
									<p>
										These mods are available from the online
										catalog
									</p>
								</div>
								<!-- Removed Open Mods Folder button for Mod Manager Catalog section -->
							</div>
						{:else if !isLoadingLocalMods && localMods.length === 0 && paginatedMods.length === 0}
							<div class="no-mods-message">
								<p>No installed mods.</p>
								<div class="no-mods-buttons">
									<button
										class="open-folder-button"
										onclick={openModsFolder}
										title="Open mods folder"
									>
										<Folder size={20} /> Open Mods Folder
									</button>
								</div>
							</div>
						{/if}

						<!-- Only proceed with catalog enabled/disabled sections if there are mods to show -->
						{#if paginatedMods.length > 0}
							<!-- Enabled Catalog Mods -->
							{#if enabledMods.length > 0}
								<div class="subsection-header enabled">
									<h4>Enabled Catalog Mods</h4>
									<p>
										{enabledMods.length} mod{enabledMods.length !==
										1
											? "s"
											: ""} active
									</p>
								</div>
								<div
									class="mods-grid"
									class:has-local-mods={localMods.length > 0}
								>
									{#each visiblePaginatedMods.filter((m) =>
										enabledMods.some((e) => e.title === m.title)
									) as mod, index (mod.title)}
										<div class="virtual-cell">
											<ModCard
												{mod}
												deferImages={paginating}
												onmodclick={handleModClick}
												oninstallclick={installMod}
												onuninstallclick={uninstallMod}
												onToggleEnabled={handleModToggled}
											/>
										</div>
									{/each}
								</div>
							{/if}

							<!-- Disabled Catalog Mods -->
							{#if disabledMods.length > 0}
								<div class="subsection-header disabled">
									<h4>Disabled Catalog Mods</h4>
									<p>
										{disabledMods.length} mod{disabledMods.length !==
										1
											? "s"
											: ""} inactive
									</p>
								</div>
								<div
									class="mods-grid"
									class:has-local-mods={localMods.length > 0}
								>
									{#each visiblePaginatedMods.filter((m) =>
										disabledMods.some((e) => e.title === m.title)
									) as mod, index (mod.title)}
										<div class="virtual-cell">
											<ModCard
												{mod}
												deferImages={paginating}
												onmodclick={handleModClick}
												oninstallclick={installMod}
												onuninstallclick={uninstallMod}
												onToggleEnabled={handleModToggled}
											/>
										</div>
									{/each}
								</div>
							{/if}

							{#if enabledMods.length === 0 && disabledMods.length === 0}
								<!-- Fallback: show installed catalog mods before enabled state resolves -->
								<div class="mods-grid">
									{#each visiblePaginatedMods as mod, index (mod.title)}
										<div class="virtual-cell">
											<ModCard
												{mod}
												deferImages={paginating}
												onmodclick={handleModClick}
												oninstallclick={installMod}
												onuninstallclick={uninstallMod}
												onToggleEnabled={handleModToggled}
											/>
										</div>
									{/each}
								</div>
							{/if}
						{/if}
					{:else}
						<!-- Original non-InstalledMods categories -->
						<div class="mods-grid">
							{#each visiblePaginatedMods as mod, index (mod.title)}
								<div class="virtual-cell">
									<ModCard
										{mod}
										deferImages={paginating}
										onmodclick={handleModClick}
										oninstallclick={installMod}
										onuninstallclick={uninstallMod}
									/>
								</div>
							{/each}
						</div>
					{/if}
				</div>
			</div>
		{/if}
	</div>

	{#if $currentModView}
		<ModView
			mod={$currentModView!}
			onCheckDependencies={handleDependencyCheck}
		/>
	{/if}
</div>


<style>
	.container.default-scrollbar {
		position: relative;
	}

	.update-all-button-top {
		position: absolute;
		top: 50%;
		left: 2.5rem; /* Position it next to the folder button */
		transform: translateY(-50%);
		z-index: 3000;
		background: #3498db;
		color: #f4eee0;
		border: 2px solid #f4eee0;
		border-radius: 8px;
		height: 47px;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		transition: all 0.2s ease;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
		padding: 0 1rem;
		font-family: "M6X11", sans-serif;
		font-size: 0.9rem;
		white-space: nowrap;
		gap: 0.5rem;
	}

	.update-all-button-top:hover {
		background: #2980b9;
		transform: translateY(-50%) scale(1.1);
	}

	.update-all-button-top:active {
		transform: translateY(-50%) scale(0.95);
	}

	/* Adjust position for smaller screens */
	@media (max-width: 1160px) {
		.update-all-button-top {
			left: 2.2rem;
		}
	}

	.no-mods-buttons {
		display: flex;
		gap: 0.75rem;
		justify-content: center;
		flex-wrap: wrap;
	}

	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: 1rem;
	}

	.section-header-content {
		flex: 1;
		min-width: 200px;
	}

	.subsection-header {
		display: flex;
		flex-direction: column;
		background: #4f6367;
		border: 2px solid #f4eee0; /*Full white border like section header*/
		padding: 0.7rem 1.5rem;
		margin: 0 2rem 1rem 2rem;
		border-radius: 8px; /*Matching border-radius*/
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3); /*Matching box-shadow*/
	}

	.subsection-header.enabled {
		background: #27ae60;
		border: 2px solid #f4eee0;
	}

	.subsection-header.disabled {
		background: #7f8c8d;
		border: 2px solid #f4eee0;
	}

	.subsection-header h4 {
		margin: 0;
		font-size: 1.3rem;
		color: #f4eee0;
		text-shadow: 1px 1px 2px rgba(0, 0, 0, 0.5);
	}

	.subsection-header p {
		margin: 0.2rem 0 0 0;
		font-size: 1rem;
		color: #f4eee0;
		opacity: 0.9;
	}

	/*Adjustments for grid spacing when using subsections*/
	.mods-grid {
		padding-top: 0.5rem;
	}

	.mods-grid:last-child {
		padding-bottom: 2rem;
	}

	.folder-icon-button {
		position: absolute;
		top: 50%;
		left: -0.2rem; /* Nudge right to avoid clipping */
		transform: translateY(-50%);
		z-index: 3000;
		background: #4caf50;
		color: #f4eee0;
		border: 2px solid #f4eee0;
		border-radius: 8px;
		width: 52px;
		height: 47px;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		transition: all 0.2s ease;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
		padding: 0;
	}

	.folder-icon-button:hover {
		background: #45a049;
		transform: translateY(-50%) scale(1.1);
	}

	.folder-icon-button:active {
		transform: translateY(-50%) scale(0.95);
	}

	/*Adjust position for smaller screens*/
	@media (max-width: 1160px) {
		.folder-icon-button {
			left: -0.6rem;
		}
	}

	.open-folder-button {
		background: #4caf50;
		color: #f4eee0;
		border: none;
		outline: #3d8b40 solid 2px;
		border-radius: 4px;
		padding: 0.75rem 1.5rem;
		font-family: "M6X11", sans-serif;
		font-size: 1.2rem;
		cursor: pointer;
		transition: all 0.2s ease;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.open-folder-button:hover {
		background: #45a049;
		transform: translateY(-2px);
	}

	.render-sentinel {
		width: 100%;
		height: 1px;
	}

	.virtual-cell {
		min-height: 0;
	}

	.open-folder-button:active {
		transform: translateY(1px);
	}

	.section-header {
		background: #c14139;
		border: 2px solid #f4eee0;
		border-radius: 8px;
		padding: 1rem 2rem;
		margin: 0 2rem 1rem 2rem;
		margin-top: 2rem;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
	}

	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: 1rem;
	}

	.section-header-content {
		flex: 1;
	}

	.section-header h3 {
		margin: 0;
		font-size: 1.8rem;
		color: #f4eee0;
		text-shadow: 1px 1px 2px rgba(0, 0, 0, 0.5);
	}

	.section-header p {
		margin: 0.5rem 0 0 0;
		font-size: 1.1rem;
		color: #f4eee0;
	}

	.mods-container {
		display: flex;
		gap: 1rem;
		padding: 0 2rem;
		overflow: hidden;
		height: 100%;
		contain: layout paint;
	}

	.no-mods-message {
		display: flex;
		justify-content: center;
		flex-direction: column;
		align-items: center;
		height: 100%;
		width: 100%;
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		margin: auto;
		padding-top: 3rem; /*Add space for the controls at the top*/
	}

	.no-mods-message p {
		font-family: "M6X11", sans-serif;
		font-size: 1.8rem;
		color: #f4eee0;
		text-align: center;
		/*Add black stroke with two methods for better browser compatibility*/
		-webkit-text-stroke: 0.1px black;
		/*Fallback using text-shadow for browsers that don't support text-stroke*/
		text-shadow:
			-1px -1px 0 #000,
			1px -1px 0 #000,
			-1px 1px 0 #000,
			1px 1px 0 #000,
			2px 2px 3px rgba(0, 0, 0, 0.5);
	}

	.separator {
		width: 2px;
		background: #f4eee0;
		height: 100%;
	}

	.controls-container {
		height: 75px;
		width: 100%;
		display: flex;
		position: absolute;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1rem;
	}

	.categories {
		width: 190px;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
		padding: 2rem 0;
		&::-webkit-scrollbar {
			width: 0;
			height: 0;
			display: none;
		}
		&::-webkit-scrollbar-track {
			background: transparent;
			border-radius: 15px;
		}
		&::-webkit-scrollbar-thumb {
			background: #f4eee0;
			border: 2px solid rgba(193, 65, 57, 0.8);
			border-radius: 15px;
		}
		&::-webkit-scrollbar:horizontal {
			display: none;
		}
		&::-webkit-scrollbar-corner {
			background-color: transparent;
		}
		scrollbar-width: 0;
		scrollbar-color: transparent transparent;
	}

	.categories button {
		text-align: left;
		padding: 1rem 1rem;
		background: #ea9600;
		border: 2px solid #f4eee0;
		color: #f4eee0;
		font-family: "M6X11", sans-serif;
		font-size: 1.1rem;
		cursor: pointer;
		transition: all 0.2s ease;
		border-radius: 6px;
		margin-right: 0.3rem;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.categories button:hover {
		background: #f4eee0;
		color: #393646;
	}

	.categories button.active {
		background: #f4eee0;
		color: #393646;
	}

	.collections-shell {
		display: flex;
		height: 100%;
	}

	.collections-left,
	.collections-right {
		display: flex;
		flex-direction: column;
		overflow-y: auto;
		padding: 1.5rem 0.6rem 2rem 0;
	}

	.collections-left {
		width: 42%;
		padding-right: 2.2rem;
	}

	.collections-right {
		flex: 1;
	}

	.collections-separator {
		width: 2px;
		background: #f4eee0;
		height: 100%;
	}

	.collections-create {
		display: flex;
		gap: 0.6rem;
		align-items: center;
		margin-bottom: 1rem;
	}

	.collections-create .text-input {
		flex: 1;
		min-width: 260px;
		background: #2d2d2d;
		color: #f4eee0;
		border: 2px solid #f4eee0;
		border-radius: 6px;
		padding: 0.6rem 0.75rem;
		font-family: "M6X11", sans-serif;
		font-size: 1rem;
	}

	.collections-create .text-input:focus {
		outline: none;
		border-color: #ea9600;
		box-shadow: 0 0 0 2px rgba(234, 150, 0, 0.35);
	}

	.collections-create .add-button {
		min-width: 44px;
		height: 44px;
		border-radius: 6px;
		border: 2px solid #f4eee0;
		background: #27ae60;
		color: #f4eee0;
		font-size: 1.4rem;
		font-family: "M6X11", sans-serif;
		cursor: pointer;
		transition: transform 0.15s ease, box-shadow 0.15s ease, background 0.15s ease;
		box-shadow: 0 3px 0 rgba(0, 0, 0, 0.25);
	}

	.collections-create .add-button:hover {
		transform: translateY(-2px);
		background: #2ecc71;
		box-shadow: 0 6px 0 rgba(0, 0, 0, 0.25);
	}

	.collections-create .add-button:active {
		transform: translateY(1px);
		box-shadow: 0 2px 0 rgba(0, 0, 0, 0.25);
	}

	.collections-import {
		margin-bottom: 1rem;
	}

	.collections-import .ghost.import {
		width: 100%;
		background: #3c5aa6;
		border: 2px solid #f4eee0;
		color: #f4eee0;
		padding: 0.7rem 0.9rem;
		border-radius: 6px;
		font-family: "M6X11", sans-serif;
		font-size: 1.05rem;
		cursor: pointer;
		transition: transform 0.15s ease, box-shadow 0.15s ease, background 0.15s ease;
	}

	.collections-import .ghost.import:hover {
		transform: translateY(-2px);
		background: #4867bf;
		box-shadow: 0 4px 10px rgba(0, 0, 0, 0.2);
	}

	.collections-import .ghost.import:active {
		transform: translateY(1px);
		box-shadow: none;
	}

	.collections-list {
		display: flex;
		flex-direction: column;
		gap: 0.8rem;
		margin: 0 0 2rem;
	}

	.collection-row {
		display: flex;
		gap: 0.6rem;
		align-items: center;
	}

	.collection-item {
		flex: 1;
		min-width: 180px;
		background: #ea9600;
		border: 2px solid #f4eee0;
		color: #f4eee0;
		padding: 0 0.9rem;
		border-radius: 6px;
		font-family: "M6X11", sans-serif;
		font-size: 1.05rem;
		cursor: pointer;
		height: 44px;
		display: flex;
		align-items: center;
		justify-content: center;
		text-align: center;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		box-sizing: border-box;
		transition: transform 0.18s ease, box-shadow 0.18s ease;
	}

	.rename-input {
		flex: 0 0 auto;
		width: 180px;
		max-width: 180px;
		box-sizing: border-box;
		background: #ea9600;
		border: 2px solid #f4eee0;
		color: #f4eee0;
		padding: 0 0.9rem;
		border-radius: 6px;
		font-family: "M6X11", sans-serif;
		font-size: 1.05rem;
		text-align: center;
		height: 44px;
	}

	.rename-input:focus {
		outline: none;
		background: #d9791c;
		box-shadow: 0 0 0 2px rgba(244, 238, 224, 0.35);
	}

	.rename-input::selection {
		background: #f4eee0;
		color: #393646;
	}

	.collection-item.active {
		background: #f4eee0;
		color: #393646;
	}

	.collection-item:hover {
		transform: translateY(-2px);
		box-shadow: 0 4px 10px rgba(0, 0, 0, 0.2);
	}

	.collection-item:active {
		transform: translateY(1px);
		box-shadow: none;
	}

	.collection-actions {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}

	.collection-actions .ghost {
		background: transparent;
		border: 2px solid #f4eee0;
		color: #f4eee0;
		padding: 0.7rem 0.9rem;
		border-radius: 6px;
		font-family: "M6X11", sans-serif;
		font-size: 1.05rem;
		cursor: pointer;
		transition: transform 0.15s ease, box-shadow 0.15s ease, background 0.15s ease;
	}

	.collection-actions .ghost.rename {
		background: #ea9600;
		border-color: #f4eee0;
		color: #f4eee0;
	}

	.collection-actions .ghost.delete {
		background: #e74c3c;
		border-color: #f4eee0;
		color: #f4eee0;
	}

	.collection-actions .ghost.share {
		background: #3c5aa6;
		border-color: #f4eee0;
		color: #f4eee0;
	}

	.collection-actions .ghost.confirm {
		background: #27ae60;
		border-color: #f4eee0;
		color: #f4eee0;
	}

	.collection-actions .ghost.neutral {
		background: #b86a2b;
		border-color: #f4eee0;
		color: #f4eee0;
	}

	.collection-actions .ghost:hover {
		transform: translateY(-2px);
		box-shadow: 0 4px 10px rgba(0, 0, 0, 0.2);
	}

	.collection-actions .ghost:active {
		transform: translateY(1px);
		box-shadow: none;
	}

	.toggle-collection {
		background: #ea9600;
		color: #f4eee0;
		border: 2px solid #f4eee0;
		border-radius: 6px;
		padding: 0.7rem 0.9rem;
		font-family: "M6X11", sans-serif;
		font-size: 1.05rem;
		cursor: pointer;
		transition: transform 0.15s ease, box-shadow 0.15s ease, background 0.15s ease;
	}

	.toggle-collection.enabled {
		background: #27ae60;
	}

	.toggle-collection:hover:not(:disabled) {
		transform: translateY(-2px);
		box-shadow: 0 4px 10px rgba(0, 0, 0, 0.2);
	}

	.toggle-collection:active:not(:disabled) {
		transform: translateY(1px);
		box-shadow: none;
	}

	.toggle-collection:disabled {
		opacity: 0.6;
		cursor: not-allowed;
	}


	@media (max-width: 1200px) {
		.collections-left {
			width: 38%;
			padding-right: 3.2rem;
		}

		.collections-create .text-input {
			min-width: 200px;
			font-size: 0.95rem;
			padding: 0.5rem 0.65rem;
		}

		.collections-create .add-button {
			min-width: 40px;
			height: 40px;
			font-size: 1.2rem;
		}

		.collection-item {
			min-width: 160px;
			font-size: 0.95rem;
			padding: 0.6rem 0.75rem;
		}

		.collection-actions .ghost,
		.toggle-collection {
			font-size: 0.95rem;
			padding: 0.6rem 0.75rem;
		}

		.rename-input {
			width: 165px;
			max-width: 165px;
		}
	}

	@media (max-width: 980px) {
		.collections-left {
			width: 42%;
			padding-right: 2.6rem;
		}

		.collections-create .text-input {
			min-width: 180px;
			font-size: 0.9rem;
		}

		.collections-create .add-button {
			min-width: 36px;
			height: 36px;
			font-size: 1.1rem;
		}

		.collection-item {
			min-width: 150px;
			font-size: 0.9rem;
		}

		.collection-actions .ghost,
		.toggle-collection {
			font-size: 0.9rem;
			padding: 0.55rem 0.7rem;
		}

		.rename-input {
			width: 150px;
			max-width: 150px;
		}
	}

	@media (max-width: 1500px) {
		.collection-row {
			flex-wrap: wrap;
			row-gap: 0.5rem;
		}

		.collection-item {
			flex: 1 1 100%;
		}

		.collection-actions {
			width: 100%;
			justify-content: space-between;
		}

		.collection-actions .ghost,
		.toggle-collection {
			flex: 1 1 0;
		}

		.rename-input {
			width: 100%;
			max-width: 100%;
		}
	}

	.collections-empty {
		text-align: center;
		color: #f4eee0;
		opacity: 0.9;
		padding: 2rem 1rem;
		font-size: 1.4rem;
	}

	.collections-mods {
		margin: 0 0 1.5rem;
	}


	.mods-scroll-container {
		overflow-y: auto;
		flex: 1 1 auto;
		min-height: 0;
		height: auto;
		contain: layout paint;
		scrollbar-gutter: stable;
		backface-visibility: hidden;
		transform: translateZ(0);
		will-change: scroll-position;
		overscroll-behavior: contain;
	}

	.mods-scroll-container.no-local-mods .subsection-header:first-of-type {
		margin-top: 3rem; /*Add spacing at the top when there are no local mods*/
	}

	.top-margin {
		margin-top: 3rem !important;
	}

	.mods-grid {
		padding: 1rem 2rem 2rem 2rem;
		flex: 1;
		display: grid;
		grid-template-columns: repeat(
			auto-fill,
			minmax(calc(280px * var(--card-scale, 1)), 1fr)
		);
		gap: 30px;
		content-visibility: auto;
		contain-intrinsic-size: 900px 1200px;
	}

	.collections-mods-grid {
		--card-scale: 0.8;
		grid-template-columns: repeat(
			auto-fill,
			minmax(calc(250px * var(--card-scale, 1)), 1fr)
		);
		gap: 24px;
	}

	.local-mods-grid {
		padding-top: 0.5rem;
		padding-bottom: 1rem;
	}

	.sort-controls {
		position: absolute;
		/*top: 0.25rem; Increased from 2rem*/
		right: 4rem; /*Increased from 2.5rem*/
		z-index: 1000;
		margin: 0;
		background: transparent;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
		/*transform: translateY(0); /*Reset any transforms*/
	}
	/**/
	/*.sort-controls {*/
	/*position: absolute;*/
	/*top: 1rem;*/
	/*right: 3rem;*/
	/*z-index: 1000;*/
	/*margin: 0;*/
	/*background: transparent;*/
	/*}*/

	.sort-wrapper {
		background: #ea9600;
		border: 2px solid #f4eee0;
		padding: 0.5rem;
		border-radius: 6px;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		transition: all 0.2s ease;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
	}

	.mods-wrapper {
		position: relative;
		/*192px being the width of the catagories + seperator*/
		width: calc(100% - 192px);
		padding: 0 1rem;
		display: flex;
		flex-direction: column;
		contain: layout paint;
	}

	.sort-wrapper :global(svg) {
		color: #f4eee0;
	}

	select {
		background: #ea9600;
		color: #f4eee0;
		border: none;
		font-family: "M6X11", sans-serif;
		font-size: 1rem;
		padding: 0.25rem 1.5rem 0.25rem 0.5rem;
		border-radius: 4px;
		cursor: pointer;
		-webkit-appearance: none;
		-moz-appearance: none;
		appearance: none;
		background-image: url("data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%23F4EEE0%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.4-12.8z%22%2F%3E%3C%2Fsvg%3E");
		background-repeat: no-repeat;
		background-position: right 0.5em top 50%;
		background-size: 0.65em auto;
	}

	select:hover {
		background-color: #f0a620;
	}

	select:focus {
		outline: none;
		box-shadow: 0 0 0 2px #f4eee0;
	}

	select option {
		background: #ea9600;
		color: #f4eee0;
		padding: 0.5rem;
	}

	.sort-wrapper:hover {
		transform: translateY(-1px);
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
	}

	.loading-container {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		flex: 1;
	}

	.loading-text {
		color: #f4eee0;
		font-family: "M6X11", sans-serif;
		font-size: 1.5rem;
		min-width: 150px;
	}

	@media (max-width: 1160px) {
		.controls-container {
			margin-bottom: 0.5rem;
		}

		.sort-controls {
			right: 1rem;
		}
	}
</style>
