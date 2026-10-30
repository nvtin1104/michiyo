import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  clampPoint,
  interpolate,
  surfaceTarget,
  strideFrame,
  chooseRest,
  hasSurface,
  frameOffset,
  type Scene,
} from './motion';
import { MascotMachine } from './machine';
const scene: Scene = {
  x: -600,
  y: 500,
  width: 200,
  height: 200,
  area: { x: -1000, y: 0, width: 1000, height: 800 },
  surfaces: [],
};
describe('motion', () => {
  it('mirrors frame offsets with facing and ignores invalid metadata', () => {
    const offsets = { '0': { x: -40, y: 5 } };
    expect(frameOffset(offsets, 0, 320, 320, 1)).toEqual({ x: -16, y: 2 });
    expect(frameOffset(offsets, 0, 320, 320, -1)).toEqual({ x: 16, y: 2 });
    expect(
      frameOffset({ '0': { x: Infinity, y: 1000 } }, 0, 320, 320, 1),
    ).toEqual({ x: 0, y: 0 });
    expect(frameOffset(undefined, 0, 320, 320, 1)).toEqual({ x: 0, y: 0 });
  });
  it('advances stride by distance and wraps at a full cycle', () => {
    expect([0, 8, 16, 24, 32].map((d) => strideFrame(d, 32, 4))).toEqual([
      0, 1, 2, 3, 0,
    ]);
    expect(strideFrame(10, 0, 4)).toBe(0);
  });
  it('alternates rest activities and detects lost landing surfaces', () => {
    expect(chooseRest('sit', 0)).toBe('wave');
    expect(chooseRest('wave', 1)).toBe('sit');
    const surface = { x: -900, y: 300, width: 500, height: 100 };
    expect(
      hasSurface({ ...scene, surfaces: [surface] }, { x: -750, y: 114 }),
    ).toBe(true);
    expect(hasSurface(scene, { x: -750, y: 114 })).toBe(false);
  });
  it('clamps physical coordinates on monitors with negative origin', () => {
    expect(clampPoint({ x: 10, y: 900 }, scene)).toEqual({ x: -200, y: 600 });
  });
  it('only selects surfaces large enough and inside the work area', () => {
    expect(
      surfaceTarget(scene, { x: -900, y: 300, width: 500, height: 100 }),
    ).toEqual({ x: -750, y: 114 });
    expect(
      surfaceTarget(scene, { x: -900, y: 300, width: 100, height: 100 }),
    ).toBeNull();
    expect(
      surfaceTarget(scene, { x: -900, y: 10, width: 500, height: 100 }),
    ).toBeNull();
  });
  it('lands at its destination after a jump', () => {
    expect(interpolate({ x: 0, y: 100 }, { x: 200, y: 50 }, 1, true)).toEqual({
      x: 200,
      y: 50,
    });
    expect(
      interpolate({ x: 0, y: 100 }, { x: 200, y: 50 }, 0.5, true).y,
    ).toBeLessThan(75);
  });
  it('interaction and busy states override autonomous motion', () => {
    const m = new MascotMachine();
    m.setMotion('run');
    expect(m.pose).toBe('run');
    m.react('dragging', 0);
    m.tick(100000, 5);
    expect(m.pose).toBe('dragging');
    m.react('settling', 100000);
    expect(m.pose).toBe('settling');
    m.transition('thinking');
    m.setMotion('walk');
    expect(m.motion).toBeNull();
  });
});

describe('bundled Reze motion assets', () => {
  it('uses distinct frames for walking, running and jump phases', () => {
    const skin = JSON.parse(
      readFileSync('public/skins/reze/manifest.json', 'utf8'),
    );
    const names = ['walk', 'run', 'jump_prepare', 'jump', 'land', 'stand'];
    const frames = names.flatMap((name) => skin.states[name].frames);
    expect(new Set(frames).size).toBe(frames.length);
    expect(skin.states.walk.frames.length).toBeGreaterThanOrEqual(4);
    expect(skin.states.run.frames.length).toBeGreaterThanOrEqual(4);
    expect(skin.states.walk.stride).toBeGreaterThan(0);
    expect(skin.states.run.stride).toBeGreaterThan(0);
  });
});
