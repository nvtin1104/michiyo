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
  let boomTimer: ReturnType<typeof setTimeout>;
  let booming = false;
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

  function triggerBoom() {
    clearTimeout(boomTimer);
    booming = false;
    if (
      config.reducedMotion ||
      matchMedia('(prefers-reduced-motion: reduce)').matches
    ) {
      react('greeting');
      return;
    }
    requestAnimationFrame(() => {
      booming = true;
      react('greeting');
      boomTimer = setTimeout(() => {
        booming = false;
      }, 820);
    });
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
    let offBoom: undefined | (() => void);
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
      listen('mascot-boom', () => {
        wanderer.stop();
        triggerBoom();
        wanderer.schedule(4500);
      }).then((f) => (disposed ? f() : (offBoom = f)));
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
      offBoom?.();
      wanderer.stop();
      disposed = true;
      cancelPress();
      clearTimeout(boomTimer);
      clearInterval(timer);
    };
  });
</script>

{#if view === 'mascot'}<div
    class="mascot-window"
    class:booming
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
    {#if booming}
      <div class="reze-boom-fx" aria-hidden="true">
        <span class="boom-flash"></span>
        <span class="boom-ring ring-outer"></span>
        <span class="boom-ring ring-inner"></span>
        <span class="boom-spark s1"></span>
        <span class="boom-spark s2"></span>
        <span class="boom-spark s3"></span>
        <span class="boom-spark s4"></span>
        <span class="boom-spark s5"></span>
        <span class="boom-spark s6"></span>
        <span class="boom-word">BOOM!</span>
      </div>
    {/if}
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

<style>
  .mascot-window {
    overflow: hidden;
  }
  .mascot-window.booming :global(.sprite) {
    animation: reze-emerge 780ms cubic-bezier(0.22, 1, 0.36, 1) both;
  }
  .reze-boom-fx {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 4;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .boom-flash {
    position: absolute;
    width: 112px;
    height: 112px;
    border-radius: 50%;
    background: radial-gradient(
      circle,
      #fffbe8 0%,
      #ff9f43 36%,
      #e84118 68%,
      transparent 75%
    );
    animation: boom-flash 520ms ease-out both;
  }
  .boom-ring {
    position: absolute;
    border-radius: 50%;
    box-sizing: border-box;
  }
  .ring-outer {
    width: 126px;
    height: 126px;
    border: 4px solid #ff5e57;
    box-shadow:
      0 0 14px #ff3f34,
      inset 0 0 10px #ffa801;
    animation: boom-ring 680ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .ring-inner {
    width: 88px;
    height: 88px;
    border: 3px dashed #c56cf0;
    animation: boom-ring 620ms 60ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .boom-spark {
    position: absolute;
    width: 7px;
    height: 7px;
    background: #ffd32a;
    box-shadow: 0 0 6px #ff3f34;
    border-radius: 2px;
    animation: boom-spark 660ms ease-out both;
  }
  .boom-spark.s1 {
    --tx: -52px;
    --ty: -44px;
  }
  .boom-spark.s2 {
    --tx: 54px;
    --ty: -38px;
    background: #ff5e57;
  }
  .boom-spark.s3 {
    --tx: -58px;
    --ty: 18px;
    background: #c56cf0;
  }
  .boom-spark.s4 {
    --tx: 56px;
    --ty: 24px;
  }
  .boom-spark.s5 {
    --tx: -22px;
    --ty: -60px;
    background: #ff9f43;
  }
  .boom-spark.s6 {
    --tx: 26px;
    --ty: 52px;
    background: #ff5e57;
  }
  .boom-word {
    position: absolute;
    top: 12px;
    font-weight: 900;
    font-size: 15px;
    letter-spacing: 0.12em;
    color: #fff6d6;
    text-shadow:
      0 2px 0 #d63031,
      0 0 10px #ff3f34;
    animation: boom-word 660ms cubic-bezier(0.2, 0.9, 0.3, 1) both;
  }
  @keyframes boom-flash {
    0% {
      opacity: 0;
      transform: scale(0.15);
    }
    25% {
      opacity: 1;
      transform: scale(1.05);
    }
    100% {
      opacity: 0;
      transform: scale(1.35);
    }
  }
  @keyframes boom-ring {
    0% {
      opacity: 1;
      transform: scale(0.18);
    }
    70% {
      opacity: 0.85;
    }
    100% {
      opacity: 0;
      transform: scale(1.15);
    }
  }
  @keyframes boom-spark {
    0% {
      opacity: 1;
      transform: translate(0, 0) scale(1.1) rotate(0deg);
    }
    100% {
      opacity: 0;
      transform: translate(var(--tx), var(--ty)) scale(0.2) rotate(160deg);
    }
  }
  @keyframes boom-word {
    0% {
      opacity: 0;
      transform: scale(0.3) translateY(18px) rotate(-8deg);
    }
    30% {
      opacity: 1;
      transform: scale(1.12) translateY(0) rotate(3deg);
    }
    100% {
      opacity: 0;
      transform: scale(0.95) translateY(-10px) rotate(0deg);
    }
  }
  @keyframes reze-emerge {
    0% {
      opacity: 0;
      transform: scale(0.25);
      filter: brightness(2.8) drop-shadow(0 0 14px #ff5e57);
    }
    45% {
      opacity: 1;
      transform: scale(1.08);
      filter: brightness(1.35) drop-shadow(0 0 8px #ffa801);
    }
    100% {
      opacity: 1;
      transform: scale(1);
      filter: none;
    }
  }
</style>
