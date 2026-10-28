<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { call, desktop } from '../lib/api';
  const items = [
    { action: 'main', label: 'Mở Michiyo', icon: 'M2 7l6-5 6 5v7H2V7Z' },
    { action: 'chat', label: 'Trò chuyện', icon: 'M3 3h10v7H7l-4 3V3Z' },
    {
      action: 'translate',
      label: 'Dịch văn bản',
      icon: 'M2 4h8M6 2v2m-3 0c0 4 5 6 5 6M9 4c0 3-3 5-6 7m7 2 2-6 2 6m-3-2h3',
    },
    {
      action: 'settings',
      label: 'Cài đặt',
      icon: 'M3 4h10M3 8h10M3 12h10M6 2v4m4 0v4m-4 0v4',
    },
    { action: 'hide', label: 'Ẩn pet', icon: 'M2 8h12' },
    { action: 'quit', label: 'Thoát', icon: 'm4 4 8 8m0-8-8 8' },
  ];
  let root: HTMLDivElement;
  let error = '';
  const focus = () =>
    tick().then(() =>
      root?.querySelector<HTMLButtonElement>('button')?.focus(),
    );
  const hide = () => desktop && getCurrentWindow().hide();
  async function choose(action: string) {
    try {
      await call('menu_action', { action });
    } catch (e) {
      error = String(e);
    }
  }
  function key(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      hide();
      return;
    }
    const buttons = Array.from(
      root.querySelectorAll<HTMLButtonElement>('button'),
    );
    let index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (['ArrowDown', 'ArrowUp', 'Home', 'End', 'Tab'].includes(event.key)) {
      event.preventDefault();
      index =
        event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? buttons.length - 1
            : (index +
                (event.key === 'ArrowUp' || event.shiftKey ? -1 : 1) +
                buttons.length) %
              buttons.length;
      buttons[index]?.focus();
    }
  }
  onMount(() => {
    let offFocus: (() => void) | undefined;
    let offBlur: (() => void) | undefined;
    focus();
    if (desktop) {
      listen('menu-focus', focus).then((f) => (offFocus = f));
      getCurrentWindow()
        .onFocusChanged((event) => {
          if (!event.payload) hide();
        })
        .then((f) => (offBlur = f));
    }
    return () => {
      offFocus?.();
      offBlur?.();
    };
  });
</script>

<div
  class="pet-context-menu"
  role="menu"
  tabindex="-1"
  aria-label="Tương tác với pet"
  bind:this={root}
  onkeydown={key}
>
  {#each items as item}<button
      type="button"
      role="menuitem"
      class:menu-quit={item.action === 'quit'}
      onclick={() => choose(item.action)}
      ><svg
        width="14"
        height="14"
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        stroke-width="1.3"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"><path d={item.icon} /></svg
      ><span>{item.label}</span></button
    >{/each}
  {#if error}<small role="alert">{error}</small>{/if}
</div>
