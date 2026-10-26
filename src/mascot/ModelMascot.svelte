<script lang="ts">
  import { onMount } from 'svelte';
  import * as THREE from 'three';
  import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
  import { listen } from '@tauri-apps/api/event';
  import { call, desktop } from '../lib/api';
  import type { Pose } from './machine';
  import manifest from '../../public/skins/reze/manifest.json';
  export let state: Pose = 'idle';
  export let speed = 1;
  export let reduced = false;
  export let pointer = { x: 80, y: 80 };
  export let tracking = true;
  export let sound = true;
  export let onfailure: () => void;
  let host: HTMLDivElement;
  let loading = true;
  let spark = false;
  onMount(() => {
    let disposed = false,
      visible = true,
      frame = 0,
      last = 0,
      maskAt = 0;
    let renderer: THREE.WebGLRenderer | undefined;
    let model: THREE.Group | undefined;
    let off: (() => void) | undefined;
    let mixer: THREE.AnimationMixer;
    let active: THREE.AnimationAction | undefined;
    let previous = '';
    let previousState = state;
    // Idle variety: occasionally swap the base idle for a move, sit or the pin pull.
    const oneShots = new Set(manifest.oneShots);
    const seated = new Set(manifest.seated);
    let mood = 'idle';
    let moodUntil = performance.now() + 4000 + Math.random() * 4000;
    let sparkTimer: ReturnType<typeof setTimeout> | undefined;
    const sounds = new Map<string, HTMLAudioElement>();
    function play(kind: keyof typeof manifest.sounds) {
      if (!sound || reduced) return;
      const list = manifest.sounds[kind];
      const src = `/skins/reze/${list[Math.floor(Math.random() * list.length)]}`;
      let audio = sounds.get(src);
      if (!audio) sounds.set(src, (audio = new Audio(src)));
      audio.volume = 0.55;
      audio.currentTime = 0;
      audio.play().catch(() => {});
    }
    function duration(name: string) {
      return ((actions.get(name)?.getClip().duration || 2) * 1000) / speed;
    }
    function startMood(name: string, now: number, loud = false) {
      mood = name;
      moodUntil =
        now +
        (oneShots.has(name)
          ? duration(name) + 300
          : seated.has(name)
            ? 12000 + Math.random() * 12000
            : 6000 + Math.random() * 5000);
      clearTimeout(sparkTimer);
      if (name === 'boom')
        sparkTimer = setTimeout(
          () => {
            spark = true;
            if (loud) play('boom');
            sparkTimer = setTimeout(() => (spark = false), 650);
          },
          duration(name) * 0.42,
        );
    }
    function chooseMood(now: number) {
      if (mood !== 'idle') {
        mood = 'idle';
        moodUntil = now + 4000 + Math.random() * 6000;
        return;
      }
      const moves = Object.entries(manifest.idleMoves);
      let roll = Math.random() * moves.reduce((sum, [, w]) => sum + w, 0);
      for (const [name, weight] of moves)
        if ((roll -= weight) <= 0) return startMood(name, now);
    }
    function clipFor(now: number) {
      if (state !== previousState) {
        if (state === 'touched') {
          play('touched');
          if (Math.random() < 0.25) startMood('boom', now, true);
        }
        previousState = state;
      }
      if (reduced) return state;
      if (state === 'touched' && mood === 'boom') return mood;
      if (state === 'idle' || state === 'hover') {
        if (now >= moodUntil) chooseMood(now);
        return mood === 'idle' ? state : mood;
      }
      mood = 'idle';
      moodUntil = now + 5000 + Math.random() * 5000;
      return state;
    }
    let wasReduced = reduced;
    let head: THREE.Object3D | undefined;
    const headPose = new THREE.Quaternion();
    let hasHeadPose = false;
    let resize: ResizeObserver | undefined;
    let lookX = 0,
      lookY = 0;
    const actions = new Map<string, THREE.AnimationAction>();
    const scene = new THREE.Scene();
    const h = manifest.camera.halfHeight;
    const camera = new THREE.OrthographicCamera(-h, h, h, -h, 0.1, 20);
    camera.position.set(0, manifest.camera.targetHeight, 5);
    camera.lookAt(0, manifest.camera.targetHeight, 0);
    scene.add(new THREE.HemisphereLight(0xffffff, 0x9294ab, 2.4));
    const key = new THREE.DirectionalLight(0xffffff, 2);
    key.position.set(2, 3, 4);
    scene.add(key);
    const maskCanvas = document.createElement('canvas');
    maskCanvas.width = maskCanvas.height = 160;
    const ctx = maskCanvas.getContext('2d', { willReadFrequently: true })!;
    let maskBusy = false;
    function release(root: THREE.Object3D) {
      const textures = new Set<THREE.Texture>();
      root.traverse((o) => {
        if (!(o instanceof THREE.Mesh)) return;
        o.geometry.dispose();
        for (const mat of Array.isArray(o.material)
          ? o.material
          : [o.material]) {
          for (const value of Object.values(mat))
            if (value instanceof THREE.Texture) textures.add(value);
          mat.dispose();
        }
      });
      textures.forEach((t) => t.dispose());
    }
    function draw(now: number) {
      frame = 0;
      if (disposed || !visible || document.hidden || !model || !renderer)
        return;
      frame = requestAnimationFrame(draw);
      const interval = reduced || state === 'sleep' ? 200 : 1000 / 30;
      if (now - last < interval) return;
      const dt = Math.min((now - last) / 1000, 0.2);
      last = now;
      if (head && hasHeadPose) head.quaternion.copy(headPose);
      const clip = clipFor(now);
      if (clip !== previous || reduced !== wasReduced) {
        const next = actions.get(clip) || actions.get('idle');
        if (next && (next !== active || reduced !== wasReduced)) {
          if (reduced) {
            mixer.stopAllAction();
            next.reset().stopFading().setEffectiveWeight(1).play();
          } else {
            // Sitting down or standing up needs a slower blend than a gesture.
            const fade = seated.has(clip) || seated.has(previous) ? 0.7 : 0.25;
            active?.fadeOut(fade);
            next.reset().fadeIn(fade).play();
          }
          active = next;
        }
        previous = clip;
        wasReduced = reduced;
      }
      mixer.update(reduced ? 0 : dt * speed);
      const trackingAllowed =
        tracking &&
        !reduced &&
        !['sleep', 'dragging', 'carried'].includes(state) &&
        !seated.has(previous);
      const near = Math.hypot(pointer.x - 80, pointer.y - 50) < 650;
      const tx =
        trackingAllowed && near
          ? THREE.MathUtils.clamp((pointer.x - 80) / 300, -0.35, 0.35)
          : 0;
      const ty =
        trackingAllowed && near
          ? THREE.MathUtils.clamp((pointer.y - 45) / 350, -0.18, 0.18)
          : 0;
      lookX = THREE.MathUtils.damp(lookX, tx, 7, dt);
      lookY = THREE.MathUtils.damp(lookY, ty, 7, dt);
      // Restore before mixing: unchanged animation bindings may skip writes.
      // Otherwise a static head pose would accumulate gaze every frame.
      if (head) {
        headPose.copy(head.quaternion);
        hasHeadPose = true;
        head.rotateY(lookX);
        head.rotateX(lookY);
      }
      renderer.render(scene, camera);
      if (desktop && !maskBusy && now - maskAt > 180) {
        maskAt = now;
        ctx.clearRect(0, 0, 160, 160);
        ctx.drawImage(renderer.domElement, 0, 0, 160, 160);
        const pixels = ctx.getImageData(0, 0, 160, 160).data;
        maskBusy = true;
        call('set_hit_mask', {
          mask: Array.from({ length: 25600 }, (_, i) => pixels[i * 4 + 3] > 16),
        })
          .catch(() => {})
          .finally(() => (maskBusy = false));
      }
    }
    function resume() {
      cancelAnimationFrame(frame);
      last = 0;
      if (!disposed) frame = requestAnimationFrame(draw);
    }
    (async () => {
      try {
        renderer = new THREE.WebGLRenderer({
          alpha: true,
          antialias: true,
          preserveDrawingBuffer: false,
        });
        renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
        renderer.setSize(160, 160);
        renderer.setClearColor(0, 0);
        renderer.outputColorSpace = THREE.SRGBColorSpace;
        host.appendChild(renderer.domElement);
        resize = new ResizeObserver(() => {
          if (!renderer || disposed) return;
          const width = Math.max(
            160,
            Math.min(640, host.getBoundingClientRect().width),
          );
          renderer.setSize(width, width, false);
        });
        resize.observe(host);
        const gltf = await new GLTFLoader().loadAsync(
          `/skins/reze/${manifest.model}`,
        );
        if (disposed) {
          release(gltf.scene);
          return;
        }
        model = gltf.scene;
        scene.add(model);
        head = model.getObjectByName(manifest.headBone);
        mixer = new THREE.AnimationMixer(model);
        for (const clip of gltf.animations) {
          const action = mixer.clipAction(clip);
          if (oneShots.has(clip.name)) {
            action.setLoop(THREE.LoopOnce, 1);
            action.clampWhenFinished = true;
          }
          actions.set(clip.name, action);
        }
        loading = false;
        if (desktop) {
          const unlisten = await listen<boolean>('mascot-visible', (e) => {
            visible = e.payload;
            resume();
          });
          if (disposed) {
            unlisten();
            return;
          }
          off = unlisten;
        }
        resume();
      } catch (e) {
        console.error('Không tải được Reze', e);
        if (!disposed) onfailure();
      }
    })();
    document.addEventListener('visibilitychange', resume);
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      clearTimeout(sparkTimer);
      sounds.forEach((a) => a.pause());
      off?.();
      document.removeEventListener('visibilitychange', resume);
      resize?.disconnect();
      if (model) {
        mixer.stopAllAction();
        mixer.uncacheRoot(model);
        release(model);
      }
      renderer?.dispose();
      renderer?.forceContextLoss();
      renderer?.domElement.remove();
    };
  });
</script>

<div class="sprite model" bind:this={host} data-renderer="three">
  {#if loading}<span role="status">Đang tải Reze…</span>{/if}
  {#if spark}<i class="spark" aria-hidden="true"></i>{/if}
</div>

<style>
  .model {
    position: relative;
    image-rendering: auto;
  }
  .model :global(canvas) {
    image-rendering: auto;
  }
  .spark {
    position: absolute;
    left: 22%;
    top: 20%;
    width: 18%;
    aspect-ratio: 1;
    pointer-events: none;
    border-radius: 50%;
    background: radial-gradient(
      circle,
      #fff7c2 0 18%,
      #ffb347 32%,
      #ff5e3a 52%,
      transparent 70%
    );
    animation: boom 0.65s ease-out forwards;
  }
  @keyframes boom {
    from {
      transform: scale(0.2);
      opacity: 1;
    }
    60% {
      opacity: 0.9;
    }
    to {
      transform: scale(1.8);
      opacity: 0;
    }
  }
  span {
    position: absolute;
    top: 65px;
    width: 100%;
    text-align: center;
    font-size: 11px;
  }
</style>
