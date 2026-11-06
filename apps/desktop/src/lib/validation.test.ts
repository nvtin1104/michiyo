import { expect, it } from 'vitest';
import { defaults } from './api';
import { validateConfig } from './validation';

it('accepts defaults', () => {
  expect(() => validateConfig(defaults)).not.toThrow();
});

it('rejects conflicting shortcuts and invalid token budgets', () => {
  expect(() =>
    validateConfig({ ...defaults, chatShortcut: defaults.shellShortcut }),
  ).toThrow(/trùng/);
  expect(() => validateConfig({ ...defaults, maxTokens: 0 })).toThrow(
    /maxTokens/,
  );
});
