<script lang="ts">
  import Mascot from './Mascot.svelte';
  import type { Pose } from './machine';
  export let state: Pose = 'idle';
  export let speed = 1;
  export let reduced = false;
  export let skin = 'default';
  export let pointer = { x: 80, y: 80 };
  export let tracking = true;
  let failed = false;
  let previousSkin = skin;
  $: if (previousSkin !== skin) {
    previousSkin = skin;
    failed = false;
  }
</script>

{#if skin === 'reze' && !failed}
  {#await import('./ModelMascot.svelte')}
    <div class="loading" role="status">Đang tải Reze…</div>
  {:then module}
    <svelte:component
      this={module.default}
      {state}
      {speed}
      {reduced}
      {pointer}
      {tracking}
      onfailure={() => (failed = true)}
    />
  {:catch}
    <Mascot {state} {speed} {reduced} {pointer} {tracking} />
  {/await}
{:else}
  <Mascot
    {state}
    {speed}
    {reduced}
    {pointer}
    {tracking}
    skin={failed ? 'default' : skin}
  />
  {#if failed}<span
      class="fallback"
      role="status"
      title="Không tải được Reze; đang dùng Moon gardener">!</span
    >{/if}
{/if}

<style>
  .loading {
    font-size: 11px;
    text-align: center;
    padding-top: 60px;
  }
  .fallback {
    position: absolute;
    bottom: 8px;
    right: 8px;
    color: #b66058;
  }
</style>
