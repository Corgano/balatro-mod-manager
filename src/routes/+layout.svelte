<script lang="ts">
	import { blur } from "svelte/transition";
	import MessageStack from "../components/MessageStack.svelte";
	import { backgroundEnabled } from "../stores/modStore";
	import { darkMode } from "../stores/ui";
	import { onMount } from "svelte";
	import DragDropOverlay from "../components/DragDropOverlay.svelte";
	import { Window } from "@tauri-apps/api/window";

    import "../app.css";
    import UpdateAvailablePopup from "../components/UpdateAvailablePopup.svelte";
    import { updatePromptDisabled } from "../stores/update";
    import { invoke } from "@tauri-apps/api/core";

	const { data, children } = $props();

	let isWindows = $state(false);
	let detectedPlatform: string | null = null;
    let showUpdatePopup = $state(false);
    let currentVersion = $state("");
    let latestVersion = $state("");

    function normalize(v: string): string {
        // strip leading 'v' and any pre-release metadata
        const t = v.trim().replace(/^v/i, "");
        // keep only digits and dots prefix
        const m = t.match(/^[0-9]+(?:\.[0-9]+)*/);
        return m ? m[0] : t;
    }

    function cmp(a: string, b: string): number {
        const as = normalize(a).split(".").map((n) => parseInt(n, 10));
        const bs = normalize(b).split(".").map((n) => parseInt(n, 10));
        const len = Math.max(as.length, bs.length);
        for (let i = 0; i < len; i++) {
            const ai = as[i] ?? 0;
            const bi = bs[i] ?? 0;
            if (ai < bi) return -1;
            if (ai > bi) return 1;
        }
        return 0;
    }

    async function checkForUpdate() {
        try {
            if ($updatePromptDisabled) return;
            const cur = await invoke<string>("get_app_version");
            currentVersion = cur;
            let tag = "";
            // Prefer tags API to avoid 404s when no releases exist
            const tagRes = await fetch(
                "https://api.github.com/repos/skyline69/balatro-mod-manager/tags?per_page=1",
                { headers: { "Accept": "application/vnd.github+json" } },
            );
            if (tagRes.ok) {
                const tags = await tagRes.json();
                if (Array.isArray(tags) && tags.length > 0) {
                    tag = tags[0].name || "";
                }
            }
            if (!tag) {
                // Fallback: newest release from list (handles repos without 'latest')
                const relRes = await fetch(
                    "https://api.github.com/repos/skyline69/balatro-mod-manager/releases?per_page=1",
                    { headers: { "Accept": "application/vnd.github+json" } },
                );
                if (relRes.ok) {
                    const list = await relRes.json();
                    if (Array.isArray(list) && list.length > 0) {
                        tag = list[0].tag_name || list[0].name || "";
                    }
                }
            }
            if (!tag) return;
            latestVersion = tag.replace(/^v/i, "");
            if (cmp(cur, latestVersion) < 0) {
                showUpdatePopup = true;
            }
        } catch (e) {
            console.warn("Update check failed:", e);
        }
    }

	async function detectPlatform() {
		if (typeof navigator === "undefined") return;

		// Use UA as an immediate hint while awaiting the plugin result to avoid UI jumps
		const ua = navigator.userAgent.toLowerCase();
		if (ua.includes("windows")) {
			isWindows = true;
		}

		try {
			const { platform } = await import("@tauri-apps/plugin-os");
			detectedPlatform = await platform();
		} catch (_) {
			if (ua.includes("linux")) detectedPlatform = "linux";
			else if (ua.includes("mac")) detectedPlatform = "macos";
			else if (ua.includes("windows")) detectedPlatform = "windows";
		}

		if (detectedPlatform) {
			document.documentElement.dataset.platform = detectedPlatform;
			isWindows = detectedPlatform === "windows";
		}
	}

	async function setupAppWindow() {
		const appWindow = Window.getCurrent();

		await appWindow.show();
		await appWindow.setFocus();
	}

    onMount(() => {
        const unsubscribeTheme = darkMode.subscribe((enabled) => {
            document.documentElement.dataset.theme = enabled ? "dark" : "light";
            document.documentElement.style.colorScheme = enabled ? "dark" : "light";
        });
        detectPlatform();
        setupAppWindow();
        checkForUpdate();
        return () => {
            unsubscribeTheme();
        };
    });
</script>

<MessageStack />
<DragDropOverlay />
<div
	class="layout-container app-body"
	style:--gradient-opacity={$backgroundEnabled ? 0 : 1}
	style:--dot-size={isWindows ? "1.5px" : "0.45px"}
	style:--dot-color={isWindows ? "var(--ui-bg-dot-win)" : "var(--ui-bg-dot)"}
>
	{#key data.url}
		<div
			in:blur={{ duration: 300, delay: 150 }}
			out:blur={{ duration: 150 }}
			class="page-content"
		>
			{@render children()}
		</div>
	{/key}
</div>

<UpdateAvailablePopup
    visible={showUpdatePopup}
    {currentVersion}
    {latestVersion}
    onClose={() => (showUpdatePopup = false)}
    onDontShow={() => { updatePromptDisabled.set(true); showUpdatePopup = false; }}
/>

<style>
	.layout-container {
		width: 100%;
		height: 100%;
		position: fixed;
		top: 0;
		left: 0;
		overflow: hidden;
		background-color: var(--ui-bg); /* Fallback background color */
	}

	.layout-container::before {
		content: "";
		position: fixed;
		top: 0;
		left: 0;
		width: 100%;
		height: 100%;
		opacity: var(--gradient-opacity, 1);
		transition: opacity 0.4s cubic-bezier(0.4, 0, 0.2, 1);
		background-color: var(--ui-bg);
		background-image: radial-gradient(
				var(--dot-color, #d66060) var(--dot-size, 0.45px),
				transparent var(--dot-size, 0.45px)
			),
			radial-gradient(
				var(--dot-color, #d66060) var(--dot-size, 0.45px),
				var(--ui-bg) var(--dot-size, 0.45px)
			);
		background-size: 18px 18px;
		background-position:
			0 0,
			9px 9px;
		z-index: -2; /* Adjust z-index to ensure proper layering */
		pointer-events: none; /* Ensure the background doesn't block interactions */
	}

	.page-content {
		width: 100%;
		height: 100%;
		position: relative;
		overflow: hidden;
		z-index: 1; /* Ensure content sits above the background */
	}

	@media screen and (min-width: 1920px) {
		.layout-container::before {
			background-size: 24px 24px;
			background-position:
				0 0,
				12px 12px;
		}
	}
</style>
