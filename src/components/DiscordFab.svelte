<script lang="ts">
  import { openExternal } from "$lib/opener";

  export let href = "https://discord.gg/JYuh8nP3qh";
  export let ariaLabel = "Open Discord";
  export let title = "Open Discord";
  export let size = 34;

  const handleClick = () => {
    openExternal(href);
  };
</script>

<button
  class="discord-fab"
  aria-label={ariaLabel}
  {title}
  style={`--fab-size:${size}px;`}
  onclick={handleClick}
>
  <img src="/images/Discord-Symbol-Blurple.svg" alt="Discord" />
</button>

<style>
  .discord-fab {
    width: var(--fab-size, 34px);
    height: var(--fab-size, 34px);
    border: none;
    border-radius: 50%;
    background: #292b2f;
    outline: 2px solid #2f3136;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    cursor: pointer;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
    transition:
      transform 0.15s ease,
      opacity 0.15s ease;
    flex-shrink: 0;
  }

  .discord-fab img {
    width: 16px;
    height: 16px;
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

  @media (max-width: 1160px) {
    .discord-fab {
      width: 32px;
      height: 32px;
    }
  }
</style>
