<script lang="ts">
  import { onMount } from 'svelte';
  import { Application, Graphics, Sprite, Texture, Rectangle } from 'pixi.js';
  import type { Pose } from './machine';
  import { frameOffset, strideFrame } from './motion';
  import { call, desktop } from '../lib/api';
  import { listen } from '@tauri-apps/api/event';
  export let facing = 1;
  export let motionDistance: number | undefined = undefined;
  export let state: Pose = 'idle';
  export let speed = 1;
  export let reduced = false;
  export let skin = 'default';
  export let pointer = { x: 80, y: 80 };
  export let tracking = true;
  export let eyeTracking = false;
  let host: HTMLDivElement;
  let ready = false;
  let changeSkin: (name: string) => Promise<void> = async () => {};
  let refreshPose: (
    pose: Pose,
    reduced: boolean,
    facing: number,
    distance: number | undefined,
    pointer: { x: number; y: number },
    tracking: boolean,
    eyeTracking: boolean,
  ) => void = () => {};
  $: if (ready) changeSkin(skin);
  $: if (ready)
    refreshPose(
      state,
      reduced,
      facing,
      motionDistance,
      pointer,
      tracking,
      eyeTracking,
    );
  onMount(() => {
    let app: Application;
    let initialized = false;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    let visibility: () => void;
    let frame = 0;
    let currentSkin = '';
    let manifest: {
      frameWidth: number;
      frameHeight: number;
      frameOffsets?: Record<string, { x?: unknown; y?: unknown }>;
      states: Record<
        string,
        { frames: number[]; fps: number; stride?: number }
      >;
    };
    let sheet: Texture;
    let image: HTMLImageElement;
    let sprite: Sprite;
    let eyes: Graphics;
    let lastFrame = -1;
    let loadId = 0;
    let visible = true;
    let offVisible: (() => void) | undefined;
    let maskBusy = false;
    let lastMaskUpdate = 0;
    let transitionFrame = 0;
    let previousState = state;
    let nextBlink = performance.now() + 2000 + Math.random() * 5000;
    const maskCanvas = document.createElement('canvas');
    maskCanvas.width = 160;
    maskCanvas.height = 160;
    const ctx = maskCanvas.getContext('2d', { willReadFrequently: true })!;
    const draw = (advance = true, schedule = true) => {
      if (disposed || document.hidden || !visible || !sheet) return;
      const animation =
        manifest.states[state] ||
        (state === 'carried' && manifest.states.dragging) ||
        manifest.states.idle;
      let index =
        (state === 'walk' || state === 'run') &&
        manifest.states[state] &&
        motionDistance !== undefined
          ? animation.frames[
              strideFrame(
                motionDistance,
                Number.isFinite(animation.stride) && animation.stride! > 0
                  ? animation.stride!
                  : state === 'run'
                    ? 48
                    : 32,
                animation.frames.length,
              )
            ]
          : animation.frames[
              Math.floor(advance ? frame++ : Math.max(0, frame - 1)) %
                animation.frames.length
            ];
      if (
        currentSkin === 'default' &&
        ['hover', 'idle'].includes(state) &&
        tracking &&
        !reduced &&
        Math.hypot(pointer.x - 80, pointer.y - 80) < 650
      ) {
        const dx = pointer.x - 80,
          dy = pointer.y - 80;
        if (Math.max(Math.abs(dx), Math.abs(dy)) > 18)
          index =
            Math.abs(dy) > Math.abs(dx) ? (dy < 0 ? 20 : 21) : dx > 0 ? 11 : 1;
      }
      if (reduced) index = animation.frames[0];
      if (currentSkin === 'default' && state === 'idle' && !reduced) {
        const now = performance.now();
        if (now >= nextBlink) {
          index = 2;
          if (now > nextBlink + 150)
            nextBlink = now + 2000 + Math.random() * 5000;
        }
      }
      const fw = manifest.frameWidth,
        fh = manifest.frameHeight;
      const columns = Math.floor(image.width / fw);
      const x = (index % columns) * fw,
        y = Math.floor(index / columns) * fh;
      if (index !== lastFrame) {
        const previousTexture = sprite.texture;
        sprite.texture = new Texture({
          source: sheet.source,
          frame: new Rectangle(x, y, fw, fh),
        });
        if (previousTexture !== Texture.EMPTY) previousTexture.destroy(false);
        lastFrame = index;
      }
      sprite.scale.x = Math.abs(sprite.scale.x) * (facing < 0 ? -1 : 1);
      const offset = frameOffset(manifest.frameOffsets, index, fw, fh, facing);
      sprite.x = 80 + offset.x;
      sprite.y = 85 + offset.y;
      sprite.rotation = 0;
      eyes.clear();
      if (
        currentSkin === 'reze' &&
        eyeTracking &&
        tracking &&
        !reduced &&
        index === 0 &&
        ['idle', 'hover'].includes(state)
      ) {
        const dx =
          Math.max(-1, Math.min(1, (pointer.x - 80) / 100)) *
          (facing < 0 ? -1 : 1);
        const dy = Math.max(-1, Math.min(1, (pointer.y - 80) / 100));
        for (const eyeX of [179, 231]) {
          eyes.ellipse(eyeX - 160, 113 - 160, 5, 6).fill(0x68a94b);
          eyes
            .ellipse(eyeX - 160 + dx * 2.2, 113 - 160 + dy * 1.5, 2.2, 3.5)
            .fill(0x213c27);
          eyes
            .circle(eyeX - 161 + dx * 2.2, 111 - 160 + dy * 1.5, 0.9)
            .fill(0xe7f5c9);
        }
      }
      if (!reduced) {
        const wave = Math.sin(performance.now() / 180);
        if (state === 'happy' || state === 'greeting')
          sprite.y -= Math.abs(wave) * 3;
        if (state === 'petting' || state === 'touched')
          sprite.rotation = wave * 0.045;
        if (state === 'dragging' || state === 'carried')
          sprite.rotation = wave * 0.08;
        if (state === 'sleep') sprite.y += 2;
      }
      app.render();
      if (
        desktop &&
        new URLSearchParams(location.search).get('view') === 'mascot' &&
        !maskBusy &&
        performance.now() - lastMaskUpdate >= 100
      ) {
        maskBusy = true;
        lastMaskUpdate = performance.now();
        ctx.clearRect(0, 0, 160, 160);
        ctx.imageSmoothingEnabled = false;
        ctx.drawImage(app.canvas, 0, 0, 160, 160);
        const pixels = ctx.getImageData(0, 0, 160, 160).data;
        call('set_hit_mask', {
          mask: Array.from({ length: 25600 }, (_, i) => pixels[i * 4 + 3] > 16),
        })
          .catch(() => {})
          .finally(() => (maskBusy = false));
      }
      if (previousState !== state && !reduced) {
        previousState = state;
        cancelAnimationFrame(transitionFrame);
        const start = performance.now();
        const tween = (now: number) => {
          if (disposed || !visible || document.hidden) return;
          const progress = Math.min(1, (now - start) / 180);
          sprite.alpha = 0.7 + 0.3 * (1 - Math.pow(1 - progress, 3));
          app.render();
          if (progress < 1) transitionFrame = requestAnimationFrame(tween);
        };
        transitionFrame = requestAnimationFrame(tween);
      }
      if (schedule)
        timer = setTimeout(
          () => draw(),
          1000 /
            (reduced || state === 'sleep'
              ? 1
              : Math.min(12, Math.max(1, animation.fps * speed))),
        );
    };
    let refreshedState = state;
    refreshPose = (nextState) => {
      const changed = nextState !== refreshedState;
      if (changed) frame = 0;
      refreshedState = nextState;
      if (changed) clearTimeout(timer);
      if (sheet && !disposed) draw(changed, changed);
    };
    changeSkin = async (name) => {
      if (name === currentSkin) return;
      const id = ++loadId;
      try {
        let source = '/skins/default/spritesheet.png';
        let data;
        if (!['default', 'moon-gardener', 'reze'].includes(name) && desktop) {
          const loaded = await call<{
            manifest: typeof manifest;
            image: string;
          }>('load_skin', { name });
          data = loaded.manifest;
          source = loaded.image;
        } else {
          const bundled = name === 'reze' ? 'reze' : 'default';
          source = `/skins/${bundled}/spritesheet.png`;
          data = await fetch(`/skins/${bundled}/manifest.json`).then((r) =>
            r.json(),
          );
        }
        const img = new Image();
        img.src = source;
        await img.decode();
        if (disposed || id !== loadId) return;
        const fw = data.frameWidth,
          fh = data.frameHeight;
        if (
          !Number.isInteger(fw) ||
          !Number.isInteger(fh) ||
          fw < 1 ||
          fh < 1 ||
          fw > 512 ||
          fh > 512 ||
          img.width % fw ||
          img.height % fh ||
          img.width > 4096 ||
          img.height > 4096
        )
          throw new Error('Frame skin không hợp lệ');
        for (const name of [
          'idle',
          'hover',
          'listening',
          'thinking',
          'talking',
          'happy',
          'error',
          'sleep',
        ]) {
          const a = data.states[name];
          if (
            !a ||
            !a.frames?.length ||
            !a.frames.every(
              (n: number) =>
                Number.isInteger(n) &&
                n >= 0 &&
                n < ((img.width / fw) * img.height) / fh,
            ) ||
            !Number.isFinite(a.fps)
          )
            throw new Error('State skin không hợp lệ');
        }
        // Optional animations in older or custom skins may be absent or malformed.
        for (const [name, value] of Object.entries(data.states)) {
          const a = value as { frames?: unknown; fps?: unknown };
          if (
            !Array.isArray(a.frames) ||
            !a.frames.length ||
            !a.frames.every(
              (n) =>
                Number.isInteger(n) &&
                n >= 0 &&
                n < (img.width / fw) * (img.height / fh),
            ) ||
            typeof a.fps !== 'number' ||
            !Number.isFinite(a.fps) ||
            a.fps <= 0
          )
            delete data.states[name];
        }
        image = img;
        manifest = data;
        const oldSheet = sheet;
        sheet = Texture.from(img);
        sheet.source.scaleMode = 'nearest';
        sprite.scale.set(128 / fw, 128 / fh);
        currentSkin = name;
        lastFrame = -1;
        clearTimeout(timer);
        draw();
        oldSheet?.destroy(true);
      } catch (e) {
        console.error('Không tải được skin', e);
        if (name !== 'default') await changeSkin('default');
      }
    };
    (async () => {
      app = new Application();
      await app.init({
        width: 160,
        height: 160,
        backgroundAlpha: 0,
        antialias: false,
        autoStart: false,
        resolution: devicePixelRatio,
        autoDensity: true,
        preference: 'webgl',
      });
      initialized = true;
      if (disposed) {
        app.destroy(true);
        return;
      }
      host.appendChild(app.canvas);
      sprite = new Sprite();
      sprite.anchor.set(0.5);
      sprite.position.set(80, 85);
      eyes = new Graphics();
      sprite.addChild(eyes);
      app.stage.addChild(sprite);
      ready = true;
      await changeSkin(skin);
      if (disposed) return;
      visibility = () => {
        clearTimeout(timer);
        if (!document.hidden) draw();
      };
      document.addEventListener('visibilitychange', visibility);
      if (desktop) {
        const unlisten = await listen<boolean>('mascot-visible', (e) => {
          visible = e.payload;
          clearTimeout(timer);
          cancelAnimationFrame(transitionFrame);
          if (visible) draw();
        });
        if (disposed) unlisten();
        else offVisible = unlisten;
      }
    })();
    return () => {
      disposed = true;
      clearTimeout(timer);
      cancelAnimationFrame(transitionFrame);
      offVisible?.();
      if (visibility)
        document.removeEventListener('visibilitychange', visibility);
      if (initialized) app.destroy(true);
      sheet?.destroy(true);
    };
  });
</script>

<div class="sprite" data-skin={skin} bind:this={host}></div>
