<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import LaunchAlertBox from "./LaunchAlertBox.svelte";
	import { addMessage } from "../lib/stores";
	import { lovelyPopupStore } from "../stores/modStore";
	import { isLinuxPlatform } from "../lib/platform";
	import { onMount } from "svelte";
	import { get } from "svelte/store";

let showAlert = false;
let isLinux = false;
let isLaunching = false;
let launchViaSteam = false;
let launchCheckTimer: ReturnType<typeof setInterval> | null = null;
let launchTimeoutTimer: ReturnType<typeof setTimeout> | null = null;

onMount(async () => {
	try {
		isLinux = await isLinuxPlatform();
	} catch (_) {
		isLinux = false;
	}
});

async function doLaunch() {
  let usesSteam = false;
  if (isLinux) {
    const is_balatro_running: boolean = await invoke("check_balatro_running");
    if (is_balatro_running) {
      addMessage("Balatro is already running", "error");
      return { launched: false, usesSteam };
    }
  }

  const path = await invoke("get_balatro_path");
  if (path && path.toString().includes("Steam")) {
    usesSteam = true;
    const is_balatro_running: boolean = await invoke("check_balatro_running");
    if (is_balatro_running) {
      addMessage("Balatro is already running", "error");
      return { launched: false, usesSteam };
    }
    if (!isLinux) {
      const is_steam_running: boolean = await invoke("check_steam_running");
      if (!is_steam_running) {
        showAlert = true;
        return { launched: false, usesSteam };
      }
    }
  }

  await invoke("launch_balatro");
  return { launched: true, usesSteam };
}

	const handleLaunch = async () => {
		if (isLaunching) return;
		isLaunching = true;
		try {
			isLinux = await isLinuxPlatform();
		} catch (_) {
			isLinux = false;
		}
		// Warn if Lovely injector is missing before any launch
  if (!isLinux) {
    try {
      const present = await invoke<boolean>("is_lovely_installed");
      if (!present) {
        lovelyPopupStore.set({
          visible: true,
          source: 'launch',
          onLaunchAnyway: async () => { await doLaunch(); },
        });
        isLaunching = false;
        return;
      }
    } catch (_) {
      // ignore detection errors, proceed
    }
  }
  let launched = false;
  let usesSteam = false;
  try {
    const res = await doLaunch();
    launched = res.launched;
    usesSteam = res.usesSteam;
  } catch (_) {
    launched = false;
  }
  if (!launched) {
    isLaunching = false;
    return;
  }
  launchViaSteam = usesSteam;

  if (launchCheckTimer) clearInterval(launchCheckTimer);
  if (launchTimeoutTimer) clearTimeout(launchTimeoutTimer);

  if (isLinux) {
    launchCheckTimer = setInterval(async () => {
      try {
        const running: boolean = await invoke("check_balatro_running");
        if (running) {
          if (launchCheckTimer) clearInterval(launchCheckTimer);
          launchCheckTimer = null;
          if (launchTimeoutTimer) clearTimeout(launchTimeoutTimer);
          launchTimeoutTimer = null;
          isLaunching = false;
          return;
        }
      } catch (_) {
        // ignore polling errors
      }
    }, 500);

    launchTimeoutTimer = setTimeout(async () => {
      if (launchCheckTimer) {
        clearInterval(launchCheckTimer);
        launchCheckTimer = null;
      }
      try {
        const running: boolean = await invoke("check_balatro_running");
        if (running) {
          isLaunching = false;
          return;
        }
      } catch (_) {
        // ignore polling errors
      }
      isLaunching = false;
      addMessage("Launch timed out. Try again.", "warning");
    }, 12000);
    return;
  }

  launchCheckTimer = setInterval(async () => {
    try {
      const running: boolean = await invoke("check_balatro_running");
      if (running) {
        if (launchCheckTimer) clearInterval(launchCheckTimer);
        launchCheckTimer = null;
        if (launchTimeoutTimer) clearTimeout(launchTimeoutTimer);
        launchTimeoutTimer = null;
        isLaunching = false;
        return;
      }
    } catch (_) {
      // ignore polling errors
    }
  }, 500);

  launchTimeoutTimer = setTimeout(async () => {
    if (launchCheckTimer) {
      clearInterval(launchCheckTimer);
      launchCheckTimer = null;
    }
    try {
      const running: boolean = await invoke("check_balatro_running");
      if (running) {
        isLaunching = false;
        return;
      }
    } catch (_) {
      // ignore polling errors
    }
    if (launchViaSteam) {
      try {
        const steamRunning: boolean = await invoke("check_steam_running");
        if (steamRunning) {
          isLaunching = false;
          return;
        }
      } catch (_) {
        // ignore polling errors
      }
    }
    isLaunching = false;
    addMessage("Launch timed out. Try again.", "warning");
  }, 12000);
};

onMount(() => {
  return () => {
    if (launchCheckTimer) clearInterval(launchCheckTimer);
    if (launchTimeoutTimer) clearTimeout(launchTimeoutTimer);
  };
});

	const handleAlertClose = () => {
		showAlert = false;
	};
</script>

<div class="launch-container">
	<button class="launch-button" onclick={handleLaunch} disabled={isLaunching}>
		Launch
	</button>
</div>

<LaunchAlertBox show={showAlert} onClose={handleAlertClose} />

<style>
	.launch-container {
		position: absolute;
		top: 2.5rem;
		right: 0rem;
	}

	.launch-button {
		background: var(--ui-launch-bg);
		color: var(--ui-launch-text);
		font-family: "M6X11", sans-serif;
		font-size: 3.2rem;
		padding: 0.5rem 2.2rem;
		border: none;
		cursor: pointer;
		transition: all 0.2s ease;
		text-shadow:
			-2px -2px 0 #000,
			2px -2px 0 #000,
			-2px 2px 0 #000,
			2px 2px 0 #000;
		border-radius: 8px;
		outline: 3px solid var(--ui-launch-outline);
		box-shadow: inset 0 0 10px rgba(0, 0, 0, 0.3);
	}

	.launch-button:hover {
		background: var(--ui-launch-hover);
		transform: translateY(-2px);
	}

	.launch-button:active {
		transform: translateY(0);
	}
	.launch-button:disabled {
		opacity: 0.8;
		cursor: not-allowed;
		transform: none;
	}

	@media (max-width: 1160px) {
		.launch-button {
			font-size: 2.8rem;
			text-shadow:
				-1.8px -1.8px 0 #000,
				1.8px -1.8px 0 #000,
				-1.8px 1.8px 0 #000,
				1.8px 1.8px 0 #000;
		}
		.launch-container {
			top: 2.4rem;
		}
	}
</style>
