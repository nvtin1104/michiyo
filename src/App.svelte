<script lang="ts">
  import { locale, t } from './lib/i18n';
  import { validateConfig } from './lib/validation';
  import { onMount } from 'svelte';
  import { load } from '@tauri-apps/plugin-store';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { enable, disable } from '@tauri-apps/plugin-autostart';
  import { call, desktop, defaults, type Config } from './lib/api';
  import { MascotMachine, type State } from './mascot/machine';
  let view =
    new URLSearchParams(location.search).get('view') ||
    (desktop ? 'mascot' : 'chat');
  let config: Config = structuredClone(defaults);
  let state: State = 'idle';
  let error = '';
  let keyMissing = false;
  const machine = new MascotMachine();
  let moved = false;
  let start = { x: 0, y: 0 };
  async function reload() {
    if (desktop) {
      const store = await load('settings.json');
      config = { ...defaults, ...(await store.get<Partial<Config>>('config')) };
    } else {
      config = {
        ...defaults,
        ...JSON.parse(localStorage.getItem('preview-config') || '{}'),
      };
    }
    locale.set(config.locale);
  }
  async function save() {
    validateConfig(config);
    locale.set(config.locale);
    if (!desktop) {
      localStorage.setItem('preview-config', JSON.stringify(config));
      return;
    }
    const store = await load('settings.json');
    const previous =
      (await store.get<Config>('config')) || structuredClone(defaults);
    const next = structuredClone(config);
    try {
      await store.set('config', next);
      await store.save();
      await call('apply_config');
      if (next.autostart) await enable();
      else await disable();
    } catch (e) {
      await store.set('config', previous);
      await store.save();
      await call('apply_config').catch(() => {});
      throw e;
    }
  }
  onMount(() => {
    let off: undefined | (() => void);
    let offConfig: undefined | (() => void);
    let offSecret: undefined | (() => void);
    reload();
    if (desktop) {
      listen<State>('mascot-state', (e) => {
        state = machine.transition(e.payload);
      }).then((f) => (off = f));
      listen('config-changed', reload).then((f) => (offConfig = f));
      const updateKey = () =>
        call<Record<string, boolean>>('secret_status').then(
          (s) => (keyMissing = !s.claude),
        );
      listen('secrets-changed', updateKey).then((f) => (offSecret = f));
      updateKey();
    }
    const timer = setInterval(
      () => (state = machine.tick(Date.now(), config.sleepMinutes)),
      1000,
    );
    return () => {
      off?.();
      offConfig?.();
      offSecret?.();
      clearInterval(timer);
    };
  });
</script>

{#if view === 'mascot'}<div
    class="mascot-window"
    style:opacity={config.opacity}
    style:transform={`scale(${config.scale})`}
    style:transform-origin="top left"
    onpointerenter={() => {
      if (!['thinking', 'talking'].includes(state))
        state = machine.transition('hover');
    }}
    onpointerleave={() => {
      if (!['thinking', 'talking'].includes(state))
        state = machine.transition('idle');
    }}
    role="button"
    tabindex="0"
    aria-label="Mở chat Michiyo"
    onkeydown={(e) => {
      if (e.key === 'Enter') call('open_panel', { label: 'chat' });
    }}
    onpointerdown={(e) => {
      start = { x: e.screenX, y: e.screenY };
      moved = false;
    }}
    onpointermove={(e) => {
      if (
        e.buttons === 1 &&
        Math.hypot(e.screenX - start.x, e.screenY - start.y) > 4 &&
        !moved
      ) {
        moved = true;
        getCurrentWindow()
          .startDragging()
          .catch((e) => (error = String(e)));
      }
    }}
    onpointerup={() => {
      if (!moved)
        call('open_panel', { label: keyMissing ? 'settings' : 'chat' });
    }}
  >
    {#await import('./mascot/Mascot.svelte') then module}<svelte:component
        this={module.default}
        {state}
        skin={config.skin}
        speed={config.animationSpeed}
        reduced={config.reducedMotion ||
          matchMedia('(prefers-reduced-motion: reduce)').matches}
      />{/await}{#if keyMissing}<span class="mascot-hint">⚿</span>{/if}
  </div>
{:else}<main class:preview={!desktop}>
    {#if !desktop}<div class="preview-bar">
        <span>MICHIYO · BẢN XEM TRƯỚC</span>
        <div>
          {#each [['chat', 'Chat'], ['translate', 'Dịch'], ['settings', 'Cài đặt']] as item}<button
              class:active={view === item[0]}
              onclick={() => (view = item[0])}>{t(item[1], $locale)}</button
            >{/each}
        </div>
      </div>{/if}
    <div class="page" class:settings={view === 'settings'}>
      {#if view === 'settings'}{#await import('./settings/Settings.svelte') then module}<svelte:component
            this={module.default}
            bind:config
            {save}
          />{/await}{:else if view === 'translate'}{#await import('./translate/Translate.svelte') then module}<svelte:component
            this={module.default}
          />{/await}{:else}{#await import('./chat/Chat.svelte') then module}<svelte:component
            this={module.default}
            {config}
          />{/await}{/if}
    </div>
    {#if error}<p class="error">{error}</p>{/if}
  </main>{/if}
