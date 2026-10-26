<script lang="ts">
  import { onMount } from 'svelte';
  import { Application, Sprite, Texture, Rectangle } from 'pixi.js';
  import type { Pose } from './machine';
  import { call, desktop } from '../lib/api';
  import { listen } from '@tauri-apps/api/event';
  export let state: Pose = 'idle';
  export let speed = 1;
  export let reduced = false;
  export let skin = 'default';
  export let pointer = { x: 80, y: 80 };
  export let tracking = true;
  let host: HTMLDivElement;
  let ready = false;
  let changeSkin: (name: string) => Promise<void> = async () => {};
  let refreshPose: (pose: Pose, reduced: boolean) => void = () => {};
  $: if (ready) changeSkin(skin);
  $: if (ready) refreshPose(state, reduced);
  onMount(() => {
    let app: Application;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    let visibility: () => void;
    let frame = 0;
    let currentSkin = '';
    let manifest: {
      frameWidth: number;
      frameHeight: number;
      states: Record<string, { frames: number[]; fps: number }>;
    };
    let sheet: Texture;
    let image: HTMLImageElement;
    let sprite: Sprite;
    let lastFrame = -1;
    let loadId = 0;
    let visible = true;
    let offVisible: (() => void) | undefined;
    let maskBusy = false;
    let transitionFrame = 0;
    let previousState = state;
    let nextBlink = performance.now() + 2000 + Math.random() * 5000;
    const maskCanvas = document.createElement('canvas');
    maskCanvas.width = 160;
    maskCanvas.height = 160;
    const ctx = maskCanvas.getContext('2d', { willReadFrequently: true })!;
    const draw = () => {
      if (disposed || document.hidden || !visible || !sheet) return;
      const animation = manifest.states[state] || manifest.states.idle;
      let index =
        animation.frames[Math.floor(frame++) % animation.frames.length];
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
      sprite.x = 80;
      sprite.y = 85;
      sprite.rotation = 0;
      if (!reduced) {
        const wave = Math.sin(performance.now() / 180);
        if (state === 'happy' || state === 'greeting')
          sprite.y -= Math.abs(wave) * 3;
        if (state === 'petting' || state === 'touched')
          sprite.rotation = wave * 0.045;
        if (state === 'dragging') sprite.rotation = wave * 0.08;
        if (state === 'sleep') sprite.y += 2;
      }
      app.render();
      ctx.clearRect(0, 0, 160, 160);
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(app.canvas, 0, 0, 160, 160);
      const pixels = ctx.getImageData(0, 0, 160, 160).data;
      if (desktop && !maskBusy) {
        maskBusy = true;
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
      timer = setTimeout(
        draw,
        1000 /
          (reduced || state === 'sleep'
            ? 1
            : Math.min(12, Math.max(1, animation.fps * speed))),
      );
    };
    refreshPose = () => {
      frame = 0;
      clearTimeout(timer);
      if (sheet && !disposed) draw();
    };
    changeSkin = async (name) => {
      if (name === currentSkin) return;
      const id = ++loadId;
      try {
        let source = '/skins/default/spritesheet.png';
        let data;
        if (name !== 'default' && desktop) {
          const loaded = await call<{
            manifest: typeof manifest;
            image: string;
          }>('load_skin', { name });
          data = loaded.manifest;
          source = loaded.image;
        } else {
          data = await fetch('/skins/default/manifest.json').then((r) =>
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
      if (disposed) {
        app.destroy(true);
        return;
      }
      host.appendChild(app.canvas);
      sprite = new Sprite();
      sprite.anchor.set(0.5);
      sprite.position.set(80, 85);
      app.stage.addChild(sprite);
      ready = true;
      await changeSkin(skin);
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
      app?.destroy(true);
      sheet?.destroy(true);
    };
  });
</script>

<div class="sprite" bind:this={host}></div>
