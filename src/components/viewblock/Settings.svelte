<script lang="ts">
import PathSelector from "../PathSelector.svelte";
	import { Settings2, RefreshCw, Folder, Save, Info, Copy } from "lucide-svelte";
import { addMessage } from "$lib/stores";
import { onMount } from "svelte";
import { fade, fly } from "svelte/transition";
import { invoke } from "@tauri-apps/api/core";
import {
	backgroundEnabled,
	cachedVersions,
	catalogLastRefreshed,
	catalogResetNonce,
	currentJokerView,
	currentModView,
	modsStore,
	searchResults,
} from "../../stores/modStore";
import { descriptionsStore } from "../../stores/descriptions";
    import { cardScale } from "../../stores/ui";
import { browser } from "$app/environment";

	let isReindexing = false;
	let isClearingCache = false;
	let isConsoleEnabled = false;
	let isBackgroundAnimationEnabled = false;
	let lastReindexStats = {
		removedFiles: 0,
		cleanedEntries: 0,
	};
	let isDiscordRpcEnabled = false;
	let isLinux = false;
	let linuxPrefix = "";
	let showLinuxHelp = false;
	let activeHelpImage: { src: string; alt: string } | null = null;

	export async function performReindexMods() {
		isReindexing = true;
		try {
			const result = await invoke<[number, number]>("reindex_mods");
			lastReindexStats = {
				removedFiles: result[0], // Will always be 0
				cleanedEntries: result[1],
			};
			addMessage(
				`Reindex complete: Cleaned ${result[1]} database entries`,
				"success",
			);
		} catch (error) {
			addMessage("Reindex failed: " + error, "error");
		} finally {
			isReindexing = false;
		}
	}

	async function clearCache() {
		isClearingCache = true;
		try {
			await invoke("clear_cache");
			// Also clear small UI caches persisted in localStorage
			try {
				localStorage.removeItem("version-cache-steamodded");
				localStorage.removeItem("version-cache-talisman");
				localStorage.removeItem("mods-cache");
				localStorage.removeItem("mods-cache-ts");
				localStorage.removeItem("mods-descriptions-cache");
			} catch (e) {
				// ignore storage errors
			}
			// Clear in-memory stores so the Mods view refetches fresh data.
			modsStore.set([]);
			searchResults.set([]);
			currentModView.set(null);
			currentJokerView.set(null);
			catalogLastRefreshed.set(null);
			catalogResetNonce.update((n) => n + 1);
			cachedVersions.set({ steamodded: [], talisman: [] });
			descriptionsStore.set({});
			addMessage("Successfully cleared all caches!", "success");
		} catch (error) {
			addMessage("Failed to clear cache: " + error, "error");
		} finally {
			isClearingCache = false;
		}
	}

	async function handleDiscordRpcChange() {
		const newValue = !isDiscordRpcEnabled;
		try {
			await invoke("set_discord_rpc_status", { enabled: newValue });
			isDiscordRpcEnabled = newValue;
			addMessage(
				`Discord Rich Presence ${newValue ? "enabled" : "disabled"}`,
				"success",
			);
		} catch (error) {
			console.error("Failed to set Discord RPC status:", error);
			addMessage(
				"Failed to update Discord Rich Presence status",
				"error",
			);
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

	async function handleConsoleChange() {
		const newValue = !isConsoleEnabled;
		try {
			await invoke("set_lovely_console_status", { enabled: newValue });
			isConsoleEnabled = newValue;
			addMessage(
				`Lovely Console ${newValue ? "enabled" : "disabled"}`,
				"success",
			);
		} catch (error) {
			console.error("Failed to set console status:", error);
			addMessage("Failed to update Lovely Console status", "error");
		}
	}

	async function handleBackgroundAnimationChange() {
		const newValue = !isBackgroundAnimationEnabled;

		// Optimistic UI update
		backgroundEnabled.set(newValue);

		try {
			await invoke("set_background_state", { enabled: newValue });
			isBackgroundAnimationEnabled = newValue;
		} catch (error) {
			// Rollback on failure
			backgroundEnabled.set(!newValue);
			isBackgroundAnimationEnabled = !newValue;
		}
	}

	async function handleLinuxPrefixChange() {
		const newValue = linuxPrefix.replace(/\s+/g, " ").trim();
		if (!newValue) {
			addMessage("Linux prefix is empty", "error");
			return;
		}
		if (newValue !== linuxPrefix) {
			linuxPrefix = newValue;
			addMessage("Linux prefix had extra spaces and was normalized", "warning");
		}
		try {
			await invoke("set_linux_prefix", { value: newValue });
			addMessage(`Linux prefix set to ${newValue}`, "success");
		} catch (error) {
			console.error("Failed to set prefix:", error);
			addMessage("Failed to update Linux prefix", "error");
		}
	}

	async function copyLinuxLaunchOptions() {
		const text = 'WINEDLLOVERRIDES="version=n,b" %command%';
		try {
			await navigator.clipboard.writeText(text);
			addMessage("Copied Steam launch options to clipboard", "success");
		} catch (error) {
			console.error("Failed to copy launch options:", error);
			addMessage("Failed to copy Steam launch options", "error");
		}
	}

	function toggleLinuxHelp() {
		showLinuxHelp = !showLinuxHelp;
	}

	function openHelpImage(src: string, alt: string) {
		activeHelpImage = { src, alt };
	}

	function closeHelpImage() {
		activeHelpImage = null;
	}

	function handleModalKeydown(event: KeyboardEvent) {
		if (event.key === "Escape") {
			closeHelpImage();
			return;
		}
		if (event.key === "Enter" || event.key === " ") {
			event.preventDefault();
			closeHelpImage();
		}
	}

	onMount(async () => {
		// Detect platform for Linux-specific UI gating
		try {
			const { platform } = await import("@tauri-apps/plugin-os");
			isLinux = (await platform()) === "linux";
		} catch (_) {
			if (browser) {
				isLinux =
					navigator.userAgent.toLowerCase().includes("linux") &&
					!navigator.userAgent.toLowerCase().includes("android");
			}
		}

		try {
			isDiscordRpcEnabled = await invoke("get_discord_rpc_status");
		} catch (error) {
			console.error("Failed to get Discord RPC status:", error);
			addMessage("Error fetching Discord Rich Presence status", "error");
		}
		try {
			isConsoleEnabled = await invoke("get_lovely_console_status");
		} catch (error) {
			console.error("Failed to get console status:", error);
			addMessage("Error fetching Lovely Console status", "error");
		}
		try {
			isBackgroundAnimationEnabled = await invoke("get_background_state");
			backgroundEnabled.set(isBackgroundAnimationEnabled);
		} catch (error) {
			console.error("Failed to get background status:", error);
			addMessage("Error fetching background animation status", "error");
		}
		if (isLinux) {
			try {
				linuxPrefix = await invoke("get_linux_prefix");
				if (!linuxPrefix) {
					linuxPrefix = "steam -applaunch 2379780";
					await invoke("set_linux_prefix", { value: linuxPrefix });
					addMessage("Linux prefix defaulted to steam -applaunch 2379780", "info");
				}
			} catch (error) {
				console.error("Failed to get Linux prefix:", error);
				addMessage("Error fetching Linux prefix", "error");
			}
		}
	});
</script>

<div class="container default-scrollbar">
	<div class="settings-container">
		<h2>Settings</h2>
		<div class="content">
			<h3>Game Path</h3>
			<PathSelector />
			<h3>Cache</h3>
			<button
				class="clear-cache-button"
				on:click={clearCache}
				disabled={isClearingCache}
			>
				{#if isClearingCache}
					<div class="throbber"></div>
				{:else}
					<RefreshCw size={20} />
					Clear Cache
				{/if}
			</button>

			<p class="description warning">
				<span class="warning-icon">⚠️</span>
				Frequent cache clearing may trigger API rate limits
			</p>

			<h3>Mods</h3>

			<div class="mods-settings">
				<button
					class="open-folder-button"
					on:click={openModsFolder}
					title="Open mods folder"
				>
					<Folder size={20} />
					Open Mods Folder
				</button>

				<p class="description">
					Open the folder where mods are stored on your system.
				</p>

				<button
					class="reindex-button"
					on:click={performReindexMods}
					disabled={isReindexing}
					title="Synchronize database with filesystem state"
				>
					{#if isReindexing}
						<div class="throbber"></div>
						Scanning...
					{:else}
						<Settings2 size={20} />
						Validate Mod Database
					{/if}
				</button>

				{#if lastReindexStats.removedFiles + lastReindexStats.cleanedEntries > 0}
					<div class="reindex-stats">
						<strong>Last cleanup:</strong>
						<span
							>Files removed: {lastReindexStats.removedFiles}</span
						>
						<span
							>Database entries cleaned: {lastReindexStats.cleanedEntries}</span
						>
					</div>
				{/if}
				<p class="description-small">
					Performs consistency check on the mod database. Will only
					remove:
					<br />• Database entries for missing mod installations
				</p>
				{#if isLinux}
					<div class="linux-note">
						<span class="linux-note-icon">
							<Info size={18} />
						</span>
						<div class="linux-note-content">
							<strong>Linux Steam launch:</strong>
							Set Steam launch options for Balatro to
							<code>WINEDLLOVERRIDES="version=n,b" %command%</code> so Lovely and mods load
							when using <code>steam -applaunch 2379780</code>.
							<div class="linux-note-actions">
								<button class="linux-copy-button" on:click={copyLinuxLaunchOptions}>
									<Copy size={16} />
									Copy launch options
								</button>
								<button class="linux-what-button" on:click={toggleLinuxHelp}>
									{showLinuxHelp ? "Hide" : "What?"}
								</button>
							</div>
							{#if showLinuxHelp}
								<div class="linux-help">
									<figure>
										<button
											type="button"
											class="linux-help-button"
											on:click={() =>
												openHelpImage(
													"/images/steam-help-first.png",
													"Open Balatro properties from Steam library",
												)}
											aria-label="Open step 1 help image"
										>
											<img
												src="/images/steam-help-first.png"
												alt="Open Balatro properties from Steam library"
												draggable="false"
											/>
										</button>
										<figcaption>1. Right-click Balatro → Properties.</figcaption>
									</figure>
									<figure>
										<button
											type="button"
											class="linux-help-button"
											on:click={() =>
												openHelpImage(
													"/images/steam-help-1.png",
													"Steam launch options menu",
												)}
											aria-label="Open step 2 help image"
										>
											<img
												src="/images/steam-help-1.png"
												alt="Steam launch options menu"
												draggable="false"
											/>
										</button>
										<figcaption>2. Find the Launch Options field.</figcaption>
									</figure>
									<figure>
										<button
											type="button"
											class="linux-help-button"
											on:click={() =>
												openHelpImage(
													"/images/steam-help-2.png",
													"Set WINEDLLOVERRIDES launch options",
												)}
											aria-label="Open step 3 help image"
										>
											<img
												src="/images/steam-help-2.png"
												alt="Set WINEDLLOVERRIDES launch options"
												draggable="false"
											/>
										</button>
										<figcaption>3. Paste the WINEDLLOVERRIDES line.</figcaption>
									</figure>
								</div>
							{/if}
						</div>
					</div>
					{#if activeHelpImage}
						<div
							class="image-modal"
							role="button"
							aria-label="Close image preview"
							tabindex="0"
							on:click={closeHelpImage}
							on:keydown={handleModalKeydown}
							transition:fade={{ duration: 120 }}
						>
							<div
								class="image-modal-content"
								role="presentation"
								on:click|stopPropagation
								transition:fly={{ y: 12, duration: 180 }}
							>
								<button class="image-modal-close" on:click={closeHelpImage}>
									×
								</button>
								<img
									src={activeHelpImage.src}
									alt={activeHelpImage.alt}
									draggable="false"
								/>
								<p>{activeHelpImage.alt}</p>
							</div>
						</div>
					{/if}
					<input
						type="text"
						bind:value={linuxPrefix}
						placeholder="Linux prefix (e.g. proton, protontricks-launch)"
						class="prefix-input"
					/>
					<button
						class="prefix-button"
						on:click={handleLinuxPrefixChange}
						title="Update Linux prefix"
					>
						<Save size={20} />
						Save Prefix
					</button>
					<p class="description-small">
						Launch command for Linux (Proton/Wine/Steam). Leave blank to use native LOVE.
						Use `{'{exe}'}` to place the Balatro.exe path, or `steam -applaunch 2379780` to
						launch via Steam. Lovely requires `WINEDLLOVERRIDES=version=n,b` and uses
						the Proton Mods folder (linked from your host Mods dir).
					</p>
				{/if}
			</div>
			<h3>Appearance</h3>
			{#if !isLinux}
				<div class="console-settings">
					<span class="label-text">Enable Background Animation</span>
					<div class="switch-container">
						<label class="switch">
							<input
								type="checkbox"
								checked={isBackgroundAnimationEnabled}
								on:change={handleBackgroundAnimationChange}
							/> <span class="slider"></span>
						</label>
					</div>
				</div>
				<p class="description-small">
					Enable or disable the animated background. Disabling may improve
					performance on low-end devices.
				</p>
			{/if}

			<!-- Card size slider -->
			<div class="slider-row">
				<div class="slider-label">
					<span class="label-text">Card Size</span>
					<span class="value">{Math.round($cardScale * 100)}%</span>
				</div>
				<input
					class="range"
					type="range"
					min="0.75"
					max="1.4"
					step="0.05"
					bind:value={$cardScale}
					aria-label="Card size"
				/>
			</div>
			<p class="description-small">
				Adjust how large mod cards render. Smaller cards fit more per row.
			</p>

			<div class="console-settings">
				<span class="label-text">Enable Discord Rich Presence</span>
				<div class="switch-container">
					<label class="switch">
						<input
							type="checkbox"
							checked={isDiscordRpcEnabled}
							on:change={handleDiscordRpcChange}
						/> <span class="slider"></span>
					</label>
				</div>
			</div>
			<p class="description-small">
				Show your Balatro activity in Discord. Displays your current
				status and mod manager usage.
			</p>

			<h3>Developer Options</h3>
			<div class="console-settings">
				<span class="label-text">Enable Lovely Console</span>
				<div class="switch-container">
					<label class="switch">
						<input
							type="checkbox"
							checked={isConsoleEnabled}
							on:change={handleConsoleChange}
						/> <span class="slider"></span>
					</label>
				</div>
			</div>
		</div>
	</div>
</div>

<style>
	.settings-container {
		padding: 0rem 2rem;
		padding-bottom: 2rem;
	}

	h2 {
		font-size: 2.5rem;
		margin-bottom: 2rem;
		color: #fdcf51;
	}
	h3 {
		font-size: 1.8rem;
		margin-bottom: 1rem;
		align-self: flex-start;
		color: #fdcf51;
	}
	.content {
		flex: 1;
	}
	.reindex-button {
		background: #56a786;
		color: #f4eee0;
		border: none;
		outline: #459373 solid 2px;
		border-radius: 4px;
		padding: 0.75rem 1.5rem;
		font-family: "M6X11", sans-serif;
		font-size: 1.2rem;
		cursor: pointer;
		transition: all 0.2s ease;
		align-self: flex-start;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.reindex-button:hover {
		background: #74cca8;
		transform: translateY(-2px);
	}
	.throbber {
		width: 20px;
		height: 20px;
		border: 3px solid #f4eee0;
		border-radius: 50%;
		border-top-color: transparent;
		animation: spin 1s linear infinite;
	}
	.warning {
		color: #ffd700;
		font-size: 1.1rem;
		border-left: 3px solid #ffd700;
		padding-left: 0.8rem;
		margin-top: 0.8rem;
		max-width: 600px !important;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	.reindex-button:disabled {
		cursor: not-allowed;
		opacity: 0.8;
		transform: none;
	}
	.clear-cache-button {
		background: #6d28d9;
		color: #f4eee0;
		border: none;
		outline: #5b21b6 solid 2px;
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
	.clear-cache-button:hover:not(:disabled) {
		background: #7c3aed;
		transform: translateY(-2px);
	}
	.clear-cache-button:disabled {
		cursor: not-allowed;
		opacity: 0.8;
		transform: none;
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
		align-self: flex-start;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-top: 1rem;
	}

	.open-folder-button:hover {
		background: #45a049;
		transform: translateY(-2px);
	}

	.open-folder-button:active {
		transform: translateY(1px);
	}
	.prefix-input {
		margin-top: 1rem;
		background: #1f1f1f;
		color: #f4eee0;
		border: 2px solid #5b21b6;
		border-radius: 4px;
		padding: 0.6rem 0.8rem;
		font-family: "M6X11", sans-serif;
		font-size: 1.1rem;
		min-width: 320px;
	}
	.prefix-input::placeholder {
		color: #bfb8a8;
	}
	.prefix-button {
		margin-top: 0.6rem;
		background: #7c3aed;
		color: #f4eee0;
		border: none;
		outline: #5b21b6 solid 2px;
		border-radius: 4px;
		padding: 0.6rem 1.2rem;
		font-family: "M6X11", sans-serif;
		font-size: 1.1rem;
		cursor: pointer;
		transition: all 0.2s ease;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.prefix-button:hover {
		background: #8b5cf6;
		transform: translateY(-2px);
	}
	.linux-note {
		margin-top: 0.8rem;
		padding: 1rem 1.1rem;
		background: #8a5b1a;
		border: 1px solid #d89b3f;
		border-radius: 4px;
		color: #f4eee0;
		font-size: 1.1rem;
		max-width: 100%;
		line-height: 1.4;
		display: flex;
		align-items: flex-start;
		gap: 0.6rem;
	}
	.linux-note-icon {
		color: #fdcf51;
		margin-top: 2px;
		display: inline-flex;
	}
	.linux-note-icon :global(svg) {
		display: block;
	}
	.linux-note-content {
		flex: 1;
	}
	.linux-note code {
		font-family: "M6X11", sans-serif;
		background: #6b4413;
		padding: 0.1rem 0.3rem;
		border-radius: 3px;
	}
	.linux-note-actions {
		margin-top: 0.5rem;
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.linux-copy-button {
		background: #c9971e;
		color: #f4eee0;
		border: 2px solid #e3a93a;
		border-radius: 4px;
		padding: 0.4rem 0.6rem;
		font-family: "M6X11", sans-serif;
		font-size: 1.15rem;
		cursor: pointer;
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		transition: transform 0.15s ease, background 0.15s ease, border-color 0.15s ease;
	}
	.linux-copy-button:hover {
		background: #d3a428;
		border-color: #d28a22;
		transform: translateY(-1px);
	}
	.linux-copy-button:focus-visible {
		outline: 2px solid #fdcf51;
		outline-offset: 2px;
	}
	.linux-copy-button:active {
		transform: translateY(0);
		background: #b48518;
		border-color: #c07a16;
	}
	.linux-what-button {
		background: #2878c8;
		color: #f4eee0;
		border: 2px solid #2b9ce9;
		border-radius: 4px;
		padding: 0.4rem 0.6rem;
		font-family: "M6X11", sans-serif;
		font-size: 1.15rem;
		cursor: pointer;
		transition: transform 0.15s ease, background 0.15s ease, border-color 0.15s ease;
	}
	.linux-what-button:hover {
		background: #2f88da;
		border-color: #33a6f2;
		transform: translateY(-1px);
	}
	.linux-what-button:focus-visible {
		outline: 2px solid #8ad7ff;
		outline-offset: 2px;
	}
	.linux-what-button:active {
		transform: translateY(0);
		background: #1f6bb3;
		border-color: #1f8ed6;
	}
	.linux-help {
		margin-top: 0.7rem;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
		gap: 1rem;
		width: 100%;
	}
	.linux-help figure {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.linux-help-button {
		background: transparent;
		border: none;
		padding: 0;
		cursor: zoom-in;
	}
	.linux-help-button:focus-visible {
		outline: 2px solid #8ad7ff;
		outline-offset: 3px;
		border-radius: 6px;
	}
	.linux-help img {
		width: 100%;
		height: auto;
		display: block;
		border: 2px solid #2b9ce9;
		border-radius: 6px;
		background: #0f1a2e;
	}
	.linux-help figcaption {
		color: #f4eee0;
		font-size: 1.2rem;
		opacity: 0.9;
	}
	.image-modal {
		position: fixed;
		inset: 0;
		background: rgba(10, 10, 12, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
		padding: 1.5rem;
		width: 100%;
		height: 100%;
		cursor: pointer;
	}
	.image-modal-content {
		position: relative;
		max-width: min(1100px, 92vw);
		max-height: 90vh;
		background: #0f1a2e;
		border: 2px solid #2b9ce9;
		border-radius: 10px;
		padding: 1rem 1.2rem 1.2rem;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}
	.image-modal-content img {
		width: 100%;
		height: auto;
		max-height: 75vh;
		object-fit: contain;
		border-radius: 6px;
	}
	.image-modal-content p {
		margin: 0;
		color: #f4eee0;
		font-size: 1.1rem;
	}
	.image-modal-close {
		position: absolute;
		top: 8px;
		right: 10px;
		background: #2878c8;
		color: #f4eee0;
		border: 2px solid #2b9ce9;
		border-radius: 6px;
		width: 32px;
		height: 32px;
		font-size: 1.2rem;
		line-height: 1;
		cursor: pointer;
		display: inline-flex;
		align-items: center;
		justify-content: center;
	}
	.image-modal-close:hover {
		background: #2f88da;
	}
	.description {
		color: #f4eee0;
		font-size: 1.2rem;
		margin-top: 0.5rem;
		opacity: 0.9;
		max-width: 400px;
		line-height: 1.4;
	} /* Custom Toggle Switch Styles */
	.description-small {
		/* color a bit grayer but still light */
		color: #c4c2c2;
		font-size: 1.1rem;
		margin-top: 0.5rem;
		opacity: 0.9;
		max-width: 400px;
		line-height: 1.4;
	}
	.console-settings {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-top: 1rem;
		font-size: 1.2rem;
		color: #f4eee0;
	}
	.label-text {
		white-space: nowrap;
	}

	.switch {
		position: relative;
		display: inline-block;
		width: 60px;
		height: 32px;
	}
	.switch input {
		opacity: 0;
		width: 0;
		height: 0;
	}
	.slider {
		position: absolute;
		cursor: pointer;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0; /* Disabled state: red fill and border */
		background-color: #f87171;
		border: 2px solid #fc4747;
		transition: 0.3s;
		border-radius: 10px;
	}
	.slider:before {
		position: absolute;
		content: "";
		height: 24px;
		width: 24px;
		left: 2px;
		bottom: 2px;
		background-color: #f4eee0;
		/* do a gray outline */
		outline: 2px solid #9e9a90;
		transition: 0.3s;
		border-radius: 5px;
	} /* Enabled state: green fill and border */
	.switch input:checked + .slider {
		background-color: #4ade80;
		border: 2px solid #2fba66;
	}
	.switch input:checked + .slider:before {
		transform: translateX(28px);
	}

	/* Range slider styling */
    .slider-row {
        margin-top: 1rem;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
        max-width: 420px;
    }
	.slider-label {
		display: flex;
		justify-content: space-between;
		align-items: center;
		color: #f4eee0;
		font-size: 1.1rem;
	}
    .slider-label .value {
        color: #fdcf51;
    }
    .range {
        -webkit-appearance: none;
        appearance: none;
        width: 100%;
        height: 28px; /* provide vertical room for thumb */
        background: transparent; /* move visuals to track */
        border: 0;
        box-shadow: none;
    }

    /* Track visuals */
    .range::-webkit-slider-runnable-track {
        height: 12px; /* thicker bar */
        border-radius: 6px;
        background: linear-gradient(90deg, #ea9600, #fdcf51);
        border: 2px solid #f4eee0;
        box-shadow: 0 2px 6px rgba(0,0,0,0.25);
    }
    .range::-moz-range-track {
        height: 12px; /* thicker bar */
        border-radius: 6px;
        background: linear-gradient(90deg, #ea9600, #fdcf51);
        border: 2px solid #f4eee0;
        box-shadow: 0 2px 6px rgba(0,0,0,0.25);
    }
    .range::-webkit-slider-thumb {
        -webkit-appearance: none;
        appearance: none;
        width: 20px;
        height: 20px;
        border-radius: 4px;
        background: #f4eee0; /* white thumb */
        border: 2px solid #9e9a90; /* subtle gray border */
        box-shadow: 0 2px 4px rgba(0,0,0,0.25);
        cursor: pointer;
        position: relative; /* allow offset */
        margin-top: -4px; /* center 20px thumb over 12px track */
    }
    .range::-moz-range-thumb {
        width: 20px;
        height: 20px;
        border-radius: 4px;
        background: #f4eee0; /* white thumb */
        border: 2px solid #9e9a90; /* subtle gray border */
        box-shadow: 0 2px 4px rgba(0,0,0,0.25);
        cursor: pointer;
    }
    .range::-webkit-slider-thumb:hover,
    .range::-moz-range-thumb:hover {
        box-shadow: 0 3px 6px rgba(0,0,0,0.3);
    }

    /* Responsive sizing */
    @media (max-width: 1160px) {
        /* Keep slider a comfortable width on smaller screens */
        .slider-row { max-width: 300px; }
        .slider-label { font-size: 1rem; }
        .slider-label .value { font-size: 0.95rem; }
        .range { height: 24px; }
        .range::-webkit-slider-runnable-track { height: 10px; }
        .range::-moz-range-track { height: 10px; }
        .range::-webkit-slider-thumb {
            width: 16px; height: 16px; margin-top: -3px; border-radius: 4px;
        }
        .range::-moz-range-thumb {
            width: 16px; height: 16px; border-radius: 4px;
        }
    }

	@media (max-width: 1160px) {
		.switch {
			width: 50px;
			height: 24px;
		}
		.slider {
			border-radius: 8px;
		}
		.slider:before {
			height: 16px;
			width: 16px;
			left: 1px;
			bottom: 2px;
			border-radius: 4px;
		}
		.switch input:checked + .slider:before {
			transform: translateX(26px);
		}
	}
	@media (max-width: 1160px) {
		h2 {
			font-size: 2rem;
			transition: all 0.2s ease;
		}
		h3 {
			font-size: 1.5rem;
			transition: all 0.2s ease;
		}
		.reindex-button {
			font-size: 1rem;
			padding: 0.6rem 1.2rem;
		}
		.open-folder-button {
			font-size: 1rem;
			padding: 0.6rem 1.2rem;
		}
		.clear-cache-button {
			font-size: 1rem;
			padding: 0.6rem 1.2rem;
		}
		.description {
			font-size: 1.1rem;
			max-width: 100%;
		}
		.description-small {
			font-size: 1rem;
			max-width: 100%;
		}
	}
</style>
