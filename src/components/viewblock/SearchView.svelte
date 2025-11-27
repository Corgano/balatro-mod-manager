<script lang="ts">
	import type { InstalledMod, Mod } from "../../stores/modStore";
import { onMount } from "svelte";
import {
	installationStatus,
	modsStore,
	loadingStates2 as loadingStates,
	uninstallDialogStore,
	} from "../../stores/modStore";
// lightweight debounce to avoid pulling in lodash for a single helper
function debounce<T extends (...args: unknown[]) => void>(fn: T, wait: number) {
  let t: ReturnType<typeof setTimeout> | null = null;
  return (...args: Parameters<T>) => {
    if (t) clearTimeout(t);
    t = setTimeout(() => fn(...args), wait);
  };
}
	import FlexSearch from "flexsearch";
	import { currentModView } from "../../stores/modStore";
	import { invoke } from "@tauri-apps/api/core";
	import { fetchCachedMods } from "../../stores/modCache";
import { addMessage } from "$lib/stores";
import { fade } from "svelte/transition";
import ModCard from "./ModCard.svelte";
import { cardScale } from "../../stores/ui";
import { tick } from "svelte";

	let searchQuery = $state("");
	let searchResults = $state<Mod[]>([]);
	let isSearching = $state(false);
	type SearchIndex = {
		add: (id: number, text: string) => void;
		search: (query: string) => number[];
	} | null;
	let searchIndex: SearchIndex = $state(null);
	let mods = $state<Mod[]>([]);
	let installedMods = $state<InstalledMod[]>([]);
	let mod = $state<Mod | null>(null);
	let searchInput: HTMLInputElement;

	function handleModClick(mod: Mod) {
		currentModView.set(mod);
	}

	const { onCheckDependencies } = $props<{
		onCheckDependencies?: (
			requirements: { steamodded: boolean; talisman: boolean },
			downloadAction: () => Promise<void>,
		) => void;
	}>();

	const getAllInstalledMods = async () => {
		try {
			const installed = await fetchCachedMods();
			installedMods = installed.map((mod) => ({
				name: mod.name,
				path: mod.path,
			}));
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

                // Always show dialog for core mods, even if no dependents
                uninstallDialogStore.set({
                    show: true,
                    modName: mod.title,
                    // Path may be resolved in the dialog if missing
                    modPath: installedMod?.path || "",
                    dependents,
                });
            } else {
                // Immediate uninstall for normal mods
                if (!installedMod) {
                    console.error("Mod not found in installed mods");
                    return;
                }
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
        }
    };

	const installMod = async (mod: Mod) => {
		// Create a closure-safe reference to the mod
		const modToInstall = { ...mod };

		// Define the actual download function
		const performDownload = async () => {
			try {
				loadingStates.update((s) => ({
					...s,
					[modToInstall.title]: true,
				}));

				// Create dependencies list
				const dependencies = [];
				if (modToInstall.requires_steamodded)
					dependencies.push("Steamodded");
				if (modToInstall.requires_talisman)
					dependencies.push("Talisman");

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

				await getAllInstalledMods();
				installationStatus.update((s) => ({
					...s,
					[modToInstall.title]: true,
				}));
			} catch (error) {
				console.error("Failed to install mod:", error);
				const raw = error instanceof Error ? error.message : String(error);
				const onlyUrlMsg = raw.includes("Download URL not reachable")
					? (raw.match(/Download URL not reachable[^"]*/)?.[0] || raw)
					: `Failed to install ${modToInstall.title}: ${raw}`;
				addMessage(onlyUrlMsg as string, "error");
			} finally {
				loadingStates.update((s) => ({
					...s,
					[modToInstall.title]: false,
				}));
			}
		};

		// Check dependencies first
		if (
			modToInstall.requires_steamodded ||
			modToInstall.requires_talisman
		) {
			const steamoddedInstalled = modToInstall.requires_steamodded
				? await invoke<boolean>("check_mod_installation", {
						modType: "Steamodded",
					})
				: true;

			const talismanInstalled = modToInstall.requires_talisman
				? await invoke<boolean>("check_mod_installation", {
						modType: "Talisman",
					})
				: true;

			if (
				(modToInstall.requires_steamodded && !steamoddedInstalled) ||
				(modToInstall.requires_talisman && !talismanInstalled)
			) {
				// Key change: pass both requirements AND download function
				onCheckDependencies?.(
					{
						steamodded:
							modToInstall.requires_steamodded &&
							!steamoddedInstalled,
						talisman:
							modToInstall.requires_talisman &&
							!talismanInstalled,
					},
					performDownload,
				);
				return;
			}
		}

		// Execute download if no dependencies are missing
		await performDownload();
	};

	const isModInstalled = async (mod: Mod) => {
		if (!mod) return false;

		await getAllInstalledMods();
		const status = installedMods.some((m) => m.name === mod.title);

		// Only update the store if the status has changed
		const currentStatus = $installationStatus[mod.title];
		if (currentStatus !== status) {
			installationStatus.update((s) => ({ ...s, [mod.title]: status }));
		}

		return status;
	};

	let prevMod: Mod | null = null;

	$effect(() => {
		const newMod = $currentModView;

		// Only proceed if newMod is different from the previous mod
		if (newMod && (!prevMod || newMod.title !== prevMod.title)) {
			prevMod = newMod;
			mod = newMod;

			// Move the installation check outside of the reactive context
			setTimeout(() => {
				isModInstalled(newMod);
			}, 0);
		}
	});

	onMount(() => {
		// Initialize the search index
		const IndexCtor = (FlexSearch as unknown as { Index: new (opts: { tokenize: string; preset: string; cache: boolean }) => { add: (id: number, text: string) => void; search: (q: string) => number[] } }).Index;
		searchIndex = new IndexCtor({
			tokenize: "forward",
			preset: "match",
			cache: true,
		});

		$effect(() => {
			if (searchInput) {
				searchInput.focus();
			}
		});

		// Subscribe to mods store
		return modsStore.subscribe((currentMods) => {
			mods = currentMods;
			if (mods.length > 0) {
		// Instead of clear(), recreate the index
			const IndexCtor = (FlexSearch as unknown as { Index: new (opts: { tokenize: string; preset: string; cache: boolean }) => { add: (id: number, text: string) => void; search: (q: string) => number[] } }).Index;
			searchIndex = new IndexCtor({
				tokenize: "forward",
				preset: "match",
				cache: true,
			});

				mods.forEach((mod, idx) => {
					const searchText =
						`${mod.title} ${mod.publisher}`.toLowerCase();
					if (searchIndex) searchIndex.add(idx, searchText);
				});
			}
		});
	});

	const handleSearch = debounce(() => {
		if (!searchIndex || searchQuery.length < 2) {
			searchResults = [];
			showSpinner = false;
			return;
		}

		isSearching = true;

		try {
			const searchTerm = searchQuery.toLowerCase();
			const results = searchIndex.search(searchTerm);

			searchResults = results.map((idx: number) => mods[idx]);
		} catch (error) {
			console.error("Search failed:", error);
			searchResults = [];
		} finally {
			showSpinner = false;
			isSearching = false;
		}
	}, 300);

	let showSpinner = $state(false);

	function handleInput() {
		showSpinner = true;
		handleSearch();
	}

	const BASE_CARD_WIDTH = 300;
	const BASE_CARD_HEIGHT = 330;
	const GRID_GAP_FALLBACK = 16;
	const OVERSCAN_ROWS = 1;

	let scrollContainer: HTMLDivElement | null = $state(null);
	let resultsWrapper: HTMLDivElement | null = $state(null);
	let containerHeight = $state(0);
	let containerWidth = $state(0);
	let measuredCardHeight = $state(BASE_CARD_HEIGHT);
	let measuredCardWidth = $state(BASE_CARD_WIDTH);
	let gridGap = $state(GRID_GAP_FALLBACK);
	let columnCount = $state(1);
	let paddingTop = $state(0);
	let paddingBottom = $state(0);
	let visibleResults = $state<Mod[]>([]);
	let totalRows = $state(0);
	let hasMeasuredCard = $state(false);
	let resizeObserver: ResizeObserver | null = null;
	let rafId: number | null = null;
	let measuring = false;

	function scheduleVirtualUpdate() {
		if (rafId !== null) {
			cancelAnimationFrame(rafId);
		}
		rafId = requestAnimationFrame(() => {
			rafId = null;
			updateVirtualWindow();
		});
	}

	function updateVirtualWindow() {
		if (!scrollContainer) {
			visibleResults = searchResults;
			paddingTop = 0;
			paddingBottom = 0;
			totalRows = searchResults.length > 0 ? 1 : 0;
			return;
		}

		const rowHeight = measuredCardHeight + gridGap;
		const width =
			containerWidth || scrollContainer.clientWidth || measuredCardWidth;
		const effectiveCardWidth = measuredCardWidth + gridGap;
		const cols = Math.max(
			1,
			Math.floor((width + gridGap) / Math.max(1, effectiveCardWidth)),
		);
		columnCount = cols;

		if (searchResults.length === 0 || rowHeight <= 0) {
			visibleResults = [];
			paddingTop = 0;
			paddingBottom = 0;
			totalRows = 0;
			return;
		}

		const totalRowCount = Math.max(
			1,
			Math.ceil(searchResults.length / cols),
		);
		totalRows = totalRowCount;
		const scrollTop = scrollContainer.scrollTop;
		const viewportHeight =
			containerHeight || scrollContainer.clientHeight || 0;
		const startRow = Math.max(
			0,
			Math.floor(scrollTop / rowHeight) - OVERSCAN_ROWS,
		);
		const endRow = Math.min(
			totalRowCount - 1,
			Math.ceil((scrollTop + viewportHeight) / rowHeight) +
				OVERSCAN_ROWS,
		);

		const startIndex = startRow * cols;
		const endIndex = Math.min(searchResults.length, (endRow + 1) * cols);
		visibleResults = searchResults.slice(startIndex, endIndex);

		paddingTop = startRow * rowHeight;
		const renderedRows = Math.ceil(visibleResults.length / cols);
		const consumed = paddingTop + renderedRows * rowHeight;
		const totalHeight = totalRowCount * rowHeight;
		paddingBottom = Math.max(totalHeight - consumed, 0);
	}

	async function measureCardSize() {
		if (measuring) return;
		measuring = true;
		await tick();
		const wrapper = resultsWrapper;
		if (wrapper) {
			const sample = wrapper.querySelector(".mod-card");
			if (sample instanceof HTMLElement) {
				const rect = sample.getBoundingClientRect();
				measuredCardHeight = rect.height;
				measuredCardWidth = rect.width;
				hasMeasuredCard = true;
			}
			const style = getComputedStyle(wrapper);
			const nextGap =
				parseFloat(style.rowGap || style.gap || "") ||
				GRID_GAP_FALLBACK;
			gridGap = Number.isFinite(nextGap) ? nextGap : GRID_GAP_FALLBACK;
		}
		measuring = false;
		scheduleVirtualUpdate();
	}

	function handleScroll() {
		if (!scrollContainer) return;
		scheduleVirtualUpdate();
	}

	onMount(() => {
		const updateSizes = () => {
			if (!scrollContainer) return;
			containerHeight = scrollContainer.clientHeight;
			containerWidth = scrollContainer.clientWidth;
			scheduleVirtualUpdate();
		};

		updateSizes();
		resizeObserver = new ResizeObserver(updateSizes);
		if (scrollContainer) resizeObserver.observe(scrollContainer);

		return () => {
			if (resizeObserver) resizeObserver.disconnect();
			if (rafId !== null) cancelAnimationFrame(rafId);
		};
	});

	$effect(() => {
		searchResults;
		if (scrollContainer) {
			scrollContainer.scrollTop = 0;
		}
		visibleResults =
			searchResults.length > 0 ? searchResults.slice(0, 1) : [];
		hasMeasuredCard = false;
		scheduleVirtualUpdate();
	});

	$effect(() => {
		if (!hasMeasuredCard && visibleResults.length > 0) {
			measureCardSize();
		}
	});

	$effect(() => {
		// Recompute when card scale changes (affects measured size)
		$cardScale;
		hasMeasuredCard = false;
		measuredCardHeight = BASE_CARD_HEIGHT * $cardScale;
		measuredCardWidth = BASE_CARD_WIDTH * $cardScale;
		scheduleVirtualUpdate();
	});
</script>

<div class="search-container">
	<div class="search-bar">
		<form onsubmit={handleSearch}>
			<input
				bind:this={searchInput}
				type="text"
				bind:value={searchQuery}
				oninput={handleInput}
				placeholder="Search mods... (Author or Title)"
				class="search-input"
			/>
			<!-- <button type="submit" class="search-button">
				<Search size={20} />
			</button> -->
		</form>

		{#if showSpinner}
			<!-- svelte-ignore element_invalid_self_closing_tag -->
			<div transition:fade={{ duration: 100 }} class="search-spinner" />
		{/if}
	</div>

	<div
		class="results-scroll-container default-scrollbar"
		bind:this={scrollContainer}
		onscroll={handleScroll}
	>
		<div class="results-container">
			{#if isSearching}
				<p transition:fade={{ duration: 100 }} class="resulting-text">
					Searching...
				</p>
			{:else if searchResults.length === 0 && searchQuery.length >= 2}
				<p transition:fade={{ duration: 100 }} class="resulting-text">
					No mods found matching "{searchQuery}"
				</p>
			{:else if searchResults.length > 0}
				<div
					transition:fade={{ duration: 100 }}
					class="results-wrapper"
					bind:this={resultsWrapper}
				>
					{#if paddingTop > 0}
						<div
							class="virtual-spacer"
							style={`height:${paddingTop}px`}
							aria-hidden="true"
						></div>
					{/if}
					{#each visibleResults as mod (mod.title)}
						<ModCard
							{mod}
							oninstallclick={installMod}
							onuninstallclick={uninstallMod}
							onmodclick={handleModClick}
						/>
					{/each}
					{#if paddingBottom > 0}
						<div
							class="virtual-spacer"
							style={`height:${paddingBottom}px`}
							aria-hidden="true"
						></div>
					{/if}
				</div>
			{/if}
		</div>
	</div>
</div>

<style>
	.search-container {
		position: relative;
		/* 192px being the width of the catagories + seperator */
		width: calc(100% - 192px);
		padding: 0 1rem;
	}

	::selection {
		background: #ea9600;
		color: #f4eee0;
	}

	.search-bar {
		height: 3rem;
		/* accounting for the padding (2rem) & scroll container's scrollbar (0.625rem/10px)*/
		width: calc(100% - 2.625rem);
		position: absolute;
		top: 1rem;
		z-index: 100;
	}

	.search-spinner {
		display: block;
		position: absolute;
		top: 25%;
		left: calc(100% - 2.5rem);
		width: 1rem;
		height: 1rem;
		z-index: 10;
		animation: spin infinite 1s linear;
		border-radius: 9999px;
		border: 2px solid #f4eee0;
		border-right: 2px solid transparent;
	}

	.search-bar form {
		display: flex;
		gap: 0.5rem;
		width: 100%;
	}

	.search-input {
		/* 2rem just for some spacing from the scrollbar */
		width: calc(100% - 2rem);
		padding: 0.75rem;
		border: 2px solid #f4eee0;
		border-radius: 6px;
		background-color: #393646;
		color: #f4eee0;
		font-family: "M6X11", sans-serif;
		font-size: 1.1rem;
	}
	.search-input:focus {
		outline: none;
		border-color: #ea9600;
		transition: border-color 0.2s ease;
	}
	/* legacy search button code */
	/* .search-button {
		padding: 0.75rem 1rem;
		background: #ea9600;
		border: 2px solid #f4eee0;
		border-radius: 6px;
		color: #f4eee0;
		cursor: pointer;
		display: flex;
		align-items: center;
		transition: all 0.2s ease;
	}

	.search-button:hover {
		background: #f4eee0;
		color: #393646;
	}

	.search-button:active {
		transform: scale(0.95);
		padding: 0.75rem 0.95rem;
	} */

	.resulting-text {
		position: absolute;
	}

	.results-container {
		padding: 1rem;
		padding-top: 5rem;
		contain: layout paint;
	}

	.results-wrapper {
		width: 100%;
		height: 100%;
		display: grid;
		grid-template-columns: repeat(
			auto-fill,
			minmax(calc(300px * var(--card-scale, 1)), 1fr)
		);
		gap: 1rem;
		content-visibility: auto;
		contain-intrinsic-size: 900px 1200px;
	}

	.results-scroll-container {
		overflow-y: auto;
		height: 100%;
		contain: layout paint;
		scrollbar-gutter: stable;
		backface-visibility: hidden;
		transform: translateZ(0);
		will-change: scroll-position;
		overscroll-behavior: contain;
	}

	.virtual-spacer {
		grid-column: 1 / -1;
		pointer-events: none;
	}

	@media (max-width: 1160px) {
		.results-container {
			padding: 1rem;
			padding-top: 5rem;
		}
	}
</style>
