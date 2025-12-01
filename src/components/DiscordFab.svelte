<script lang="ts">
	import { openExternal } from "$lib/opener";

	export let href = "https://discord.gg/JYuh8nP3qh";
	export let ariaLabel = "Open Discord";
	export let title = "Open Discord";
	export let size = 36;

	const handleClick = () => {
		openExternal(href);
	};
</script>

<button
	class="discord-fab"
	aria-label={ariaLabel}
	title={title}
	style={`width:${size}px;height:${size}px;`}
	onclick={handleClick}
>
	<img src="/images/Discord-Symbol-Blurple.svg" alt="Discord" />
</button>

<style>
	.discord-fab {
		border: none;
		border-radius: 50%;
		background: #292b2f;
		outline: 2px solid #2f3136;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
		transition: transform 0.15s ease, opacity 0.15s ease;
		padding: 6px;
	}

	.discord-fab img {
		width: 18px;
		height: 18px;
		display: block;
		user-select: none;
		-webkit-user-drag: none;
	}

	.discord-fab:hover {
		transform: scale(1.05);
	}

	.discord-fab:active {
		transform: scale(0.98);
	}
</style>
