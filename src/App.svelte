<script lang="ts">
  import { locale, t } from './lib/i18n';
  import { validateConfig } from './lib/validation';
  import { onMount } from 'svelte';
  import { load } from '@tauri-apps/plugin-store';
  import { listen } from '@tauri-apps/api/event';
  import {
    clampPoint,
    interpolate,
    surfaceTarget,
    chooseRest,
    hasSurface,
    type Scene,
  } from './mascot/motion';
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
  let lastRest: 'sit' | 'wave' | null = null;
  let motionGeneration = 0;
  let motionTimer: ReturnType<typeof setTimeout>;
  let mascotVisible = true;
  let landing: { x: number; y: number } | null = null;
  function stopMotion() {
    motionGeneration++;
    clearTimeout(motionTimer);
    machine.stopMotion();
    syncPose();
  }
  function scheduleMotion(delay = 5000 + Math.random() * 6000) {
    clearTimeout(motionTimer);
    if (!desktop || view !== 'mascot') return;
    motionTimer = setTimeout(wander, delay);
  }
  async function wander() {
    const allowed = () =>
      config.autonomousMovement &&
      !config.reducedMotion &&
      !matchMedia('(prefers-reduced-motion: reduce)').matches &&
      mascotVisible &&
      !document.hidden &&
      machine.canMove &&
      !pressing;
    if (!allowed()) {
      scheduleMotion();
      return;
    }
    const generation = ++motionGeneration;
    const active = () => generation === motionGeneration && allowed();
    const poseFor = async (
      next: NonNullable<typeof machine.motion>,
      ms: number,
    ) => {
      if (!active()) return false;
      machine.setMotion(next);
      syncPose();
      await new Promise((resolve) => setTimeout(resolve, ms));
      return active();
    };
    try {
      if (!landing && Math.random() < 0.25) {
        lastRest = chooseRest(lastRest, Math.random());
        if (!(await poseFor(lastRest, 2200))) return;
        if (lastRest === 'sit') await poseFor('stand', 450);
        return;
      }
      const scene = await call<Scene>('motion_scene');
      if (!active() || !scene?.area) return;
      const from = { x: scene.x, y: scene.y };
      const surfaces = (scene.surfaces || [])
        .map((r) => surfaceTarget(scene, r))
        .filter(
          (p): p is { x: number; y: number } =>
            !!p && Math.abs(p.y - from.y) < scene.area.height * 0.55,
        );
      const lostLanding = landing && !hasSurface(scene, landing);
      const support =
        landing && !lostLanding
          ? scene.surfaces.find((r) => {
              const p = surfaceTarget(scene, r);
              return (
                p &&
                Math.abs(p.x - landing!.x) < 20 &&
                Math.abs(p.y - landing!.y) < 20
              );
            })
          : undefined;
      const left = support ? Math.max(scene.area.x, support.x) : scene.area.x;
      const right = support
        ? Math.min(scene.area.x + scene.area.width, support.x + support.width)
        : scene.area.x + scene.area.width;
      let target =
        !lostLanding && surfaces.length && Math.random() < 0.35
          ? surfaces[Math.floor(Math.random() * surfaces.length)]
          : clampPoint(
              {
                x:
                  left +
                  Math.random() * Math.max(0, right - left - scene.width),
                y: support
                  ? from.y
                  : scene.area.y + scene.area.height - scene.height,
              },
              scene,
            );
      let selectedSurface = surfaces.includes(target);
      // A horizontal walk on a surface keeps the original support identity.
      const supportLanding = support ? landing : null;
      landing = null;
      const distance = Math.hypot(target.x - from.x, target.y - from.y);
      if (distance < 12) return;
      let jump = Math.abs(target.y - from.y) > 2;
      const run = !jump && distance > scene.width * 3;
      const nextFacing = target.x < from.x ? -1 : 1;
      if (nextFacing !== facing) {
        if (!(await poseFor('turn', 200))) return;
        facing = nextFacing;
      }
      if (jump && !(await poseFor('jump_prepare', 250))) return;
      machine.setMotion(jump ? 'jump' : run ? 'run' : 'walk');
      syncPose();
      motionDistance = 0;
      let origin = from;
      let duration = Math.max(800, (distance / (run ? 150 : 75)) * 1000);
      let started = performance.now();
      let previous = from;
      let verified = !selectedSurface;
      while (active()) {
        let progress = Math.min(1, (performance.now() - started) / duration);
        if (!verified && progress >= 0.7) {
          verified = true;
          const scanStarted = performance.now();
          const currentScene = await call<Scene>('motion_scene');
          if (!active()) return;
          // Surface discovery can take a second; do not skip ahead after it.
          started += performance.now() - scanStarted;
          if (!currentScene || !hasSurface(currentScene, target)) {
            origin = previous;
            target = clampPoint(
              {
                x: previous.x,
                y: scene.area.y + scene.area.height - scene.height,
              },
              scene,
            );
            selectedSurface = false;
            jump = true;
            machine.setMotion('jump');
            syncPose();
            duration = Math.max(
              500,
              Math.min(3000, (Math.abs(target.y - origin.y) / 150) * 1000),
            );
            started = performance.now();
            progress = 0;
          }
        }
        const point = clampPoint(
          interpolate(origin, target, progress, jump),
          scene,
        );
        await call('motion_position', {
          x: Math.round(point.x),
          y: Math.round(point.y),
        });
        if (!active()) return;
        if (!jump)
          motionDistance +=
            (Math.abs(point.x - previous.x) * 160) / scene.width;
        previous = point;
        if (progress >= 1) {
          if (selectedSurface) landing = target;
          else if (supportLanding && !jump) landing = supportLanding;
          if (jump) await poseFor('land', 350);
          break;
        }
        await new Promise((resolve) => setTimeout(resolve, 32));
      }
    } catch {
      // The native window or monitor can disappear during interaction.
    } finally {
      if (generation === motionGeneration) {
        machine.stopMotion();
        syncPose();
        scheduleMotion();
      }
    }
  }
  let headPress = false;
  // Grabbing above the chest lifts her up; lower down slings her over a shoulder.
  let chestPress = false;
  function syncPose() {
    state = machine.state;
    pose = machine.pose;
  }
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
  let error = '';
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
    let disposed = false;
    reload();
    react('greeting');
    if (desktop) {
      listen<State>('mascot-state', (e) => {
        if (isState(e.payload)) {
          stopMotion();
          scheduleMotion();
          machine.transition(e.payload);
          syncPose();
        }
      }).then((f) => (disposed ? f() : (off = f)));
      listen<boolean>('mascot-visible', (e) => {
        mascotVisible = e.payload;
        stopMotion();
        if (mascotVisible) scheduleMotion();
      }).then((f) => (disposed ? f() : (offVisible = f)));
      listen('config-changed', () => {
        stopMotion();
        reload().then(() => scheduleMotion());
      }).then((f) => (disposed ? f() : (offConfig = f)));
      if (view === 'mascot')
        listen<{ x: number; y: number }>('mascot-pointer', (e) => {
          pointer = e.payload;
        }).then((f) => (disposed ? f() : (offPointer = f)));
    }
    scheduleMotion();
    const timer = setInterval(() => {
      machine.tick(Date.now(), config.sleepMinutes);
      syncPose();
    }, 100);
    return () => {
      off?.();
      offConfig?.();
      offPointer?.();
      offVisible?.();
      stopMotion();
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
      if (e.key === 'Enter') call('open_main', { route: 'chat' });
    }}
    onpointerdown={(e) => {
      if (e.button !== 0) return;
      stopMotion();
      landing = null;
      scheduleMotion(12000);
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
              scheduleMotion();
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
          call('open_main', { route: 'chat' }).catch(
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
          />{/await}{:else}{#await import('./chat/Chat.svelte') then module}<svelte:component
            this={module.default}
            {config}
            compact={desktop ||
              new URLSearchParams(location.search).has('compact')}
          />{/await}{/if}
    </div>
    {#if error}<p class="error">{error}</p>{/if}
  </main>{/if}
