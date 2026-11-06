import { expect, it } from 'vitest';
import { MascotMachine } from './machine';
it('returns to idle after a short happy reaction', () => {
  const m = new MascotMachine();
  m.transition('happy');
  expect(m.tick(m.lastActivity + 2300, 5)).toBe('idle');
});
it('sleeps after inactivity and wakes on interaction', () => {
  const m = new MascotMachine();
  expect(m.tick(m.lastActivity + 60001, 1)).toBe('sleep');
  expect(m.transition('hover')).toBe('hover');
});
it('never sleeps during a pending response', () => {
  const m = new MascotMachine();
  m.transition('thinking');
  expect(m.tick(m.lastActivity + 999999, 1)).toBe('thinking');
});
it('touch and hover preserve the underlying system state', () => {
  const m = new MascotMachine();
  m.transition('thinking');
  m.react('touched', 1000);
  expect(m.pose).toBe('touched');
  expect(m.hover(false)).toBe('thinking');
  m.tick(2200, 5);
  expect(m.pose).toBe('thinking');
});
it('passive pointer movement does not wake a sleeping companion', () => {
  const m = new MascotMachine();
  m.transition('sleep');
  expect(m.hover(true)).toBe('sleep');
  m.react('touched', 1000);
  expect(m.pose).toBe('waking');
  expect(m.state).toBe('idle');
});
it('a completion received during petting is revealed after the reaction', () => {
  const m = new MascotMachine();
  m.transition('thinking');
  m.react('petting');
  m.transition('happy');
  expect(m.pose).toBe('petting');
  m.tick(Date.now() + 1900, 5);
  expect(m.pose).toBe('happy');
});
it('listening expires and repeated touches are throttled', () => {
  const m = new MascotMachine();
  m.transition('listening');
  expect(m.tick(m.lastActivity + 5001, 1)).toBe('idle');
  m.transition('listening');
  m.react('touched', 1000);
  m.react('touched', 1400);
  m.tick(2150, 5);
  expect(m.pose).toBe('listening');
});
it('a shoulder carry lasts as long as a drag', () => {
  const m = new MascotMachine();
  m.react('carried', 1000);
  m.tick(10000, 5);
  expect(m.pose).toBe('carried');
  m.react('settling', 10000);
  expect(m.pose).toBe('settling');
});
