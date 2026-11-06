import type { MascotMachine } from './machine';

export type Rect = { x: number; y: number; width: number; height: number };
export type Scene = {
  x: number;
  y: number;
  width: number;
  height: number;
  area: Rect;
  surfaces: Rect[];
};
export type Point = { x: number; y: number };
// Optional offsets are measured in source-frame pixels and mirrored with the sprite.
export function frameOffset(
  offsets: Record<string, { x?: unknown; y?: unknown }> | undefined,
  index: number,
  width: number,
  height: number,
  facing: number,
): Point {
  const offset = offsets?.[String(index)];
  const x =
    typeof offset?.x === 'number' &&
    Number.isFinite(offset.x) &&
    Math.abs(offset.x) <= width
      ? offset.x
      : 0;
  const y =
    typeof offset?.y === 'number' &&
    Number.isFinite(offset.y) &&
    Math.abs(offset.y) <= height
      ? offset.y
      : 0;
  return {
    x: x * (128 / width) * (facing < 0 ? -1 : 1),
    y: y * (128 / height),
  };
}
export function clampPoint(point: Point, scene: Scene): Point {
  const a = scene.area;
  return {
    x: Math.max(
      a.x,
      Math.min(a.x + Math.max(0, a.width - scene.width), point.x),
    ),
    y: Math.max(
      a.y,
      Math.min(a.y + Math.max(0, a.height - scene.height), point.y),
    ),
  };
}
export function surfaceTarget(scene: Scene, surface: Rect): Point | null {
  const a = scene.area;
  const left = Math.max(a.x, surface.x),
    right = Math.min(a.x + a.width, surface.x + surface.width);
  if (
    right - left < scene.width ||
    surface.y < a.y + scene.height ||
    surface.y > a.y + a.height
  )
    return null;
  const point = {
    x: (left + right - scene.width) / 2,
    y: surface.y - scene.height * 0.93,
  };
  const bounded = clampPoint(point, scene);
  return Math.abs(bounded.y - point.y) < 1 ? bounded : null;
}
export function interpolate(
  from: Point,
  to: Point,
  progress: number,
  jump: boolean,
): Point {
  const t = Math.max(0, Math.min(1, progress));
  if (t === 1) return { ...to };
  return {
    x: from.x + (to.x - from.x) * t,
    y:
      from.y +
      (to.y - from.y) * t -
      (jump
        ? Math.sin(Math.PI * t) *
          Math.min(100, Math.abs(to.x - from.x) * 0.25 + 30)
        : 0),
  };
}

// Distance is in the mascot's 160-unit logical canvas, independent of monitor DPI.
export function strideFrame(
  distance: number,
  stride: number,
  frames: number,
): number {
  if (
    !Number.isFinite(distance) ||
    !Number.isFinite(stride) ||
    stride <= 0 ||
    frames < 1
  )
    return 0;
  return Math.floor(((Math.max(0, distance) / stride) % 1) * frames);
}
export function chooseRest(
  last: 'sit' | 'wave' | null,
  random: number,
): 'sit' | 'wave' {
  if (last) return last === 'sit' ? 'wave' : 'sit';
  return random < 0.5 ? 'sit' : 'wave';
}
export function hasSurface(scene: Scene, target: Point): boolean {
  return scene.surfaces.some((surface) => {
    const point = surfaceTarget(scene, surface);
    return (
      point !== null &&
      Math.abs(point.x - target.x) < 20 &&
      Math.abs(point.y - target.y) < 20
    );
  });
}

export interface WanderCallbacks {
  machine: MascotMachine;
  canWander: () => boolean;
  isMascotView: () => boolean;
  getScene: () => Promise<Scene>;
  setPosition: (x: number, y: number) => Promise<void>;
  onUpdate: (state: { facing: number; motionDistance: number }) => void;
  onSyncPose: () => void;
}

export function createWanderController(cb: WanderCallbacks) {
  let facing = 1;
  let motionDistance = 0;
  let lastRest: 'sit' | 'wave' | null = null;
  let motionGeneration = 0;
  let motionTimer: ReturnType<typeof setTimeout>;
  let landing: Point | null = null;

  function emitState() {
    cb.onUpdate({ facing, motionDistance });
  }

  function stop() {
    motionGeneration++;
    clearTimeout(motionTimer);
    cb.machine.stopMotion();
    cb.onSyncPose();
  }

  function clearLanding() {
    landing = null;
  }

  function schedule(delay = 5000 + Math.random() * 6000) {
    clearTimeout(motionTimer);
    if (!cb.isMascotView()) return;
    motionTimer = setTimeout(wander, delay);
  }

  async function wander() {
    if (!cb.canWander()) {
      schedule();
      return;
    }
    const generation = ++motionGeneration;
    const active = () => generation === motionGeneration && cb.canWander();
    const poseFor = async (
      next: NonNullable<MascotMachine['motion']>,
      ms: number,
    ) => {
      if (!active()) return false;
      cb.machine.setMotion(next);
      cb.onSyncPose();
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
      const scene = await cb.getScene();
      if (!active() || !scene?.area) return;
      const from = { x: scene.x, y: scene.y };
      const surfaces = (scene.surfaces || [])
        .map((r) => surfaceTarget(scene, r))
        .filter(
          (p): p is Point =>
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
        emitState();
      }
      if (jump && !(await poseFor('jump_prepare', 250))) return;
      cb.machine.setMotion(jump ? 'jump' : run ? 'run' : 'walk');
      cb.onSyncPose();
      motionDistance = 0;
      emitState();
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
          const currentScene = await cb.getScene();
          if (!active()) return;
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
            cb.machine.setMotion('jump');
            cb.onSyncPose();
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
        await cb.setPosition(Math.round(point.x), Math.round(point.y));
        if (!active()) return;
        if (!jump) {
          motionDistance +=
            (Math.abs(point.x - previous.x) * 160) / scene.width;
          emitState();
        }
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
        cb.machine.stopMotion();
        cb.onSyncPose();
        schedule();
      }
    }
  }

  return { stop, schedule, clearLanding };
}
