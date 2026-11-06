<script lang="ts">
  import { tick, onMount } from 'svelte';
  export let value: string;
  export let label: string;
  export let options: { value: string; label: string }[] = [];
  export let onchange: (value: string) => void = () => {};
  let open = false,
    active = 0;
  let root: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let list: HTMLDivElement;
  const id = `options-${crypto.randomUUID()}`;
  let placement = { left: 0, top: 0, width: 0, height: 240 };
  $: selected = options.find((option) => option.value === value);
  async function reveal(
    index = options.findIndex((option) => option.value === value),
  ) {
    active = Math.max(0, index);
    const r = trigger.getBoundingClientRect();
    const below = innerHeight - r.bottom - 8,
      above = r.top - 8;
    const height = Math.min(
      240,
      options.length * 34 + 8,
      Math.max(below, above),
    );
    placement = {
      left: Math.max(8, Math.min(r.left, innerWidth - r.width - 8)),
      top: below >= height ? r.bottom + 4 : r.top - height - 4,
      width: r.width,
      height,
    };
    open = true;
    await tick();
    list
      ?.querySelector<HTMLElement>(`[data-index="${active}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }
  function choose(index: number) {
    const option = options[index];
    if (!option) return;
    value = option.value;
    open = false;
    onchange(value);
    trigger.focus();
  }
  async function key(event: KeyboardEvent) {
    if (event.key === 'Tab') {
      open = false;
      return;
    }
    if (event.key === 'Escape') {
      event.preventDefault();
      open = false;
      return;
    }
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      if (!open) {
        await reveal(event.key === 'End' ? options.length - 1 : undefined);
        return;
      }
      active =
        event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? options.length - 1
            : (active + (event.key === 'ArrowDown' ? 1 : -1) + options.length) %
              options.length;
      await tick();
      list
        ?.querySelector<HTMLElement>(`[data-index="${active}"]`)
        ?.scrollIntoView({ block: 'nearest' });
    }
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      if (open) choose(active);
      else reveal();
    }
  }
  onMount(() => {
    const scroll = (event: Event) => {
      if (event.target !== list) open = false;
    };
    document.addEventListener('scroll', scroll, true);
    return () => document.removeEventListener('scroll', scroll, true);
  });
</script>

<svelte:window
  onpointerdown={(event) => {
    if (open && !root.contains(event.target as Node)) open = false;
  }}
  onresize={() => (open = false)}
  onblur={() => (open = false)}
/>
<div class="custom-select" bind:this={root}>
  <button
    type="button"
    class="select-trigger"
    bind:this={trigger}
    role="combobox"
    aria-label={label}
    aria-expanded={open}
    aria-controls={id}
    aria-haspopup="listbox"
    aria-activedescendant={open ? `${id}-${active}` : undefined}
    onclick={() => (open ? (open = false) : reveal())}
    onkeydown={key}
    ><span>{selected?.label || value}</span><svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
      ><path d="m4 6 4 4 4-4" stroke="currentColor" stroke-width="1.5" /></svg
    ></button
  >
  {#if open}<div
      class="select-list"
      {id}
      role="listbox"
      aria-label={label}
      bind:this={list}
      style:left={`${placement.left}px`}
      style:top={`${placement.top}px`}
      style:width={`${placement.width}px`}
      style:max-height={`${placement.height}px`}
    >
      {#each options as option, index}<button
          type="button"
          role="option"
          aria-selected={value === option.value}
          id={`${id}-${index}`}
          data-index={index}
          tabindex="-1"
          class:highlighted={active === index}
          onpointermove={() => (active = index)}
          onpointerdown={(event) => event.preventDefault()}
          onclick={() => choose(index)}
          ><span>{option.label}</span>{#if value === option.value}<span
              aria-hidden="true">✓</span
            >{/if}</button
        >{/each}
    </div>{/if}
</div>
