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
