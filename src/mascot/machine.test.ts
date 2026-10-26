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
