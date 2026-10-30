<script lang="ts">
  import { onMount } from 'svelte';

  export let active = false;
  export let reduced = false;
  export let staging = false;

  let visible = false;
  let phase: 'closed' | 'open' | 'fade' = 'closed';
  let timers: ReturnType<typeof setTimeout>[] = [];

  function clearAll() {
    for (const t of timers) clearTimeout(t);
    timers = [];
  }

  export function play() {
    clearAll();
    if (reduced || matchMedia('(prefers-reduced-motion: reduce)').matches) {
      visible = false;
      staging = false;
      return;
    }
    visible = true;
    staging = true;
    phase = 'closed';
    timers.push(
      setTimeout(() => {
        phase = 'open';
      }, 520),
      setTimeout(() => {
        phase = 'fade';
        staging = false;
      }, 1220),
      setTimeout(() => {
        visible = false;
      }, 1540),
    );
  }

  onMount(() => {
    if (active) play();
    return clearAll;
  });
</script>

{#if visible}
  <div
    class="reze-intro-stage"
    class:phase-open={phase === 'open' || phase === 'fade'}
    class:phase-fade={phase === 'fade'}
    aria-hidden="true"
  >
    <div class="portrait-wrap">
      <img
        class="portrait-img closed-img"
        class:hidden-img={phase === 'open' || phase === 'fade'}
        src="/skins/reze/intro-closed.png"
        alt=""
        draggable="false"
      />
      <img
        class="portrait-img open-img"
        class:revealed={phase === 'open' || phase === 'fade'}
        src="/skins/reze/intro.png"
        alt=""
        draggable="false"
      />
    </div>
  </div>
{/if}

<style>
  .reze-intro-stage {
    position: fixed;
    inset: 0;
    z-index: 80;
    pointer-events: none;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .reze-intro-stage.phase-fade {
    animation: stage-fade-out 320ms ease-out forwards;
  }
  .portrait-wrap {
    position: relative;
    width: min(86vmin, 540px);
    height: min(86vmin, 540px);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .portrait-img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: contain;
    user-select: none;
  }
  .closed-img.hidden-img {
    opacity: 0;
  }
  .open-img {
    opacity: 0;
  }
  .open-img.revealed {
    opacity: 1;
  }
  @keyframes stage-fade-out {
    0% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }
</style>
