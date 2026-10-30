<script lang="ts">
  import { locale, t } from './lib/i18n';
  import { validateConfig } from './lib/validation';
  import { onMount } from 'svelte';
  import { load } from '@tauri-apps/plugin-store';
  import { listen } from '@tauri-apps/api/event';
  import { createWanderController, type Scene } from './mascot/motion';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { enable, disable } from '@tauri-apps/plugin-autostart';
  import { call, desktop, defaults, type Config } from './lib/api';
  import {
    MascotMachine,
    isState,
    type State,
    type Pose,
  } from './mascot/machine';
  let view =
    new URLSearchParams(location.search).get('view') ||
    (desktop ? 'mascot' : 'main');
  let config: Config = structuredClone(defaults);
  let state: State = 'idle';
  let pose: Pose = 'idle';
  let pointer = { x: 80, y: 80 };
  let holdTimer: ReturnType<typeof setTimeout>;
  let pressing = false;
  let petting = false;
  let facing = 1;
  let motionDistance = 0;
  let mascotVisible = true;
  let chatVisible = false;
  let headPress = false;
  // Grabbing above the chest lifts her up; lower down slings her over a shoulder.
  let chestPress = false;
  let error = '';
  const machine = new MascotMachine();
  let moved = false;
  let start = { x: 0, y: 0 };

  function syncPose() {
    state = machine.state;
    pose = machine.pose;
  }

  const wanderer = createWanderController({
    machine,
    isMascotView: () => desktop && view === 'mascot',
    canWander: () =>
      config.autonomousMovement &&
      !config.reducedMotion &&
      !matchMedia('(prefers-reduced-motion: reduce)').matches &&
      mascotVisible &&
      !chatVisible &&
      !document.hidden &&
      machine.canMove &&
      !pressing,
    getScene: () => call<Scene>('motion_scene'),
    setPosition: (x, y) => call('motion_position', { x, y }),
    onUpdate: (next) => {
      facing = next.facing;
      motionDistance = next.motionDistance;
    },
    onSyncPose: syncPose,
  });

  function react(kind: Parameters<MascotMachine['react']>[0]) {
    if (
      config.touchReactions ||
      ['greeting', 'dragging', 'carried', 'settling'].includes(kind) ||
      machine.state === 'sleep'
    )
      machine.react(kind);
    syncPose();
  }
  function cancelPress() {
    clearTimeout(holdTimer);
    pressing = false;
  }
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
    // Migrate the old built-in default; the original remains selectable by name.
    if (config.skin === 'default') config.skin = 'reze';
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
    let offPointer: undefined | (() => void);
    let offVisible: undefined | (() => void);
    let offChatVisible: undefined | (() => void);
    let disposed = false;
    reload();
    react('greeting');
    if (desktop) {
      listen<State>('mascot-state', (e) => {
        if (isState(e.payload)) {
          wanderer.stop();
          wanderer.schedule();
          machine.transition(e.payload);
          syncPose();
        }
      }).then((f) => (disposed ? f() : (off = f)));
      listen<boolean>('mascot-visible', (e) => {
        mascotVisible = e.payload;
        wanderer.stop();
        if (mascotVisible) wanderer.schedule();
      }).then((f) => (disposed ? f() : (offVisible = f)));
      listen<boolean>('chat-visible', (e) => {
        chatVisible = e.payload;
        wanderer.stop();
        if (!chatVisible) wanderer.schedule();
      }).then((f) => (disposed ? f() : (offChatVisible = f)));
      listen('config-changed', () => {
        wanderer.stop();
        reload().then(() => wanderer.schedule());
      }).then((f) => (disposed ? f() : (offConfig = f)));
      if (view === 'mascot')
        listen<{ x: number; y: number }>('mascot-pointer', (e) => {
          pointer = e.payload;
        }).then((f) => (disposed ? f() : (offPointer = f)));
    }
    wanderer.schedule();
    const timer = setInterval(() => {
      machine.tick(Date.now(), config.sleepMinutes);
      syncPose();
    }, 100);
    return () => {
      off?.();
      offConfig?.();
      offPointer?.();
      offVisible?.();
      offChatVisible?.();
      wanderer.stop();
      disposed = true;
      cancelPress();
      clearInterval(timer);
    };
  });
</script>

{#if view === 'mascot'}<div
    class="mascot-window"
    data-state={state}
    data-pose={pose}
    style:opacity={config.opacity}
    style:transform={`scale(${config.scale})`}
    style:transform-origin="top left"
    onpointerenter={() => {
      machine.hover(true);
      syncPose();
    }}
    onpointerleave={() => {
      machine.hover(false);
      syncPose();
    }}
    role="button"
    tabindex="0"
    aria-label="Mở chat Michiyo"
    onkeydown={(e) => {
      if (e.key === 'Enter' && config.clickOpensChat && desktop)
        call('open_panel', { label: 'chat' }).catch((e) => (error = String(e)));
    }}
    onpointerdown={(e) => {
      if (e.button !== 0) return;
      wanderer.stop();
      wanderer.clearLanding();
      wanderer.schedule(12000);
      cancelPress();
      start = { x: e.screenX, y: e.screenY };
      moved = false;
      pressing = true;
      petting = false;
      const r = e.currentTarget.getBoundingClientRect();
      const y = (e.clientY - r.top) / r.height;
      headPress = y < 0.56;
      chestPress = y < 0.5;
      e.currentTarget.setPointerCapture(e.pointerId);
      if (headPress && config.touchReactions)
        holdTimer = setTimeout(() => {
          if (pressing && !moved) {
            petting = true;
            react('petting');
          }
        }, 450);
    }}
    onpointermove={(e) => {
      const r = e.currentTarget.getBoundingClientRect();
      pointer = {
        x: ((e.clientX - r.left) / r.width) * 160,
        y: ((e.clientY - r.top) / r.height) * 160,
      };
      if (
        pressing &&
        e.buttons === 1 &&
        Math.hypot(e.screenX - start.x, e.screenY - start.y) >
          (headPress && config.touchReactions ? 24 : 4) &&
        !moved
      ) {
        moved = true;
        cancelPress();
        e.currentTarget.releasePointerCapture(e.pointerId);
        react(chestPress ? 'dragging' : 'carried');
        if (desktop)
          getCurrentWindow()
            .startDragging()
            .catch((e) => (error = String(e)))
            .finally(async () => {
              while (
                await call<boolean>('mascot_mouse_down').catch(() => false)
              )
                await new Promise((resolve) => setTimeout(resolve, 60));
              react('settling');
              wanderer.schedule();
            });
      }
    }}
    onpointerup={(e) => {
      if (e.button !== 0) return;
      const wasPressing = pressing;
      cancelPress();
      if (e.currentTarget.hasPointerCapture(e.pointerId))
        e.currentTarget.releasePointerCapture(e.pointerId);
      if (moved && !desktop) react('settling');
      else if (!moved && wasPressing && !petting) {
        react('touched');
        if (config.clickOpensChat && desktop)
          call('open_panel', { label: 'chat' }).catch(
            (e) => (error = String(e)),
          );
      }
    }}
    onpointercancel={() => {
      cancelPress();
      if (moved && !desktop) react('settling');
    }}
    oncontextmenu={(e) => {
      e.preventDefault();
      if (desktop) call('mascot_menu').catch((e) => (error = String(e)));
    }}
  >
    {#await import('./mascot/Companion.svelte') then module}<svelte:component
        this={module.default}
        state={pose}
        {facing}
        {motionDistance}
        {pointer}
        tracking={config.cursorTracking}
        eyeTracking={true}
        skin={config.skin}
        speed={config.animationSpeed}
        reduced={config.reducedMotion ||
          matchMedia('(prefers-reduced-motion: reduce)').matches}
      />{/await}
  </div>
{:else if view === 'main'}{#await import('./shell/Shell.svelte') then module}<svelte:component
      this={module.default}
      bind:config
      {save}
    />{/await}
{:else if view === 'menu'}{#await import('./mascot/ContextMenu.svelte') then module}<svelte:component
      this={module.default}
    />{/await}{:else}<main
    class:preview={!desktop}
    class:bubble={desktop && view === 'chat'}
  >
    {#if !desktop}<div class="preview-bar">
        <span>MICHIYO · BẢN XEM TRƯỚC</span>
        <div>
          {#each [['main', 'Chat & Workspace'], ['settings', 'Cài đặt']] as item}<button
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
            compact={desktop ||
              new URLSearchParams(location.search).has('compact')}
          />{/await}{:else}{#await import('./chat/Chat.svelte') then module}<svelte:component
            this={module.default}
            {config}
            compact={desktop ||
              new URLSearchParams(location.search).has('compact')}
          />{/await}{/if}
    </div>
    {#if error}<p class="error">{error}</p>{/if}
  </main>{/if}
