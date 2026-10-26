import { expect, it } from 'vitest';
import { defaults } from './api';
import { validateConfig } from './validation';
it('accepts defaults and local LibreTranslate', () => {
  expect(() =>
    validateConfig({ ...defaults, libreUrl: 'http://127.0.0.1:5000' }),
  ).not.toThrow();
});
it('rejects conflicting shortcuts and invalid token budgets', () => {
  expect(() =>
    validateConfig({ ...defaults, chatShortcut: defaults.translateShortcut }),
  ).toThrow(/trùng/);
  expect(() => validateConfig({ ...defaults, maxTokens: 0 })).toThrow(
    /maxTokens/,
  );
});
it('does not send LibreTranslate credentials to remote plain HTTP', () => {
  expect(() =>
    validateConfig({ ...defaults, libreUrl: 'http://translate.example.com' }),
  ).toThrow(/HTTPS/);
});
