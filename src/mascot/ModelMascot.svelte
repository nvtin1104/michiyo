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
  export let onfailure: () => void;
  let host: HTMLDivElement;
  let loading = true;
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
      if (state !== previous || reduced !== wasReduced) {
        const next = actions.get(state) || actions.get('idle');
        if (next && (next !== active || reduced !== wasReduced)) {
          if (reduced) {
            mixer.stopAllAction();
            next.reset().stopFading().setEffectiveWeight(1).play();
          } else {
            active?.fadeOut(0.22);
            next.reset().fadeIn(0.22).play();
          }
          active = next;
        }
        previous = state;
        wasReduced = reduced;
      }
      mixer.update(reduced ? 0 : dt * speed);
      const trackingAllowed =
        tracking && !reduced && !['sleep', 'dragging'].includes(state);
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
        for (const clip of gltf.animations)
          actions.set(clip.name, mixer.clipAction(clip));
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
</div>

<style>
  .model {
    image-rendering: auto;
  }
  .model :global(canvas) {
    image-rendering: auto;
  }
  span {
    position: absolute;
    top: 65px;
    width: 100%;
    text-align: center;
    font-size: 11px;
  }
</style>
