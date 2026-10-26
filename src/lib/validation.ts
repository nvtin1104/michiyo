import type { Config } from './api';
export function validateConfig(c: Config) {
  for (const [key, min, max] of [
    ['maxTokens', 1, 16384],
    ['historyLimit', 1, 100],
    ['sleepMinutes', 1, 120],
  ] as const) {
    if (!Number.isInteger(c[key]) || c[key] < min || c[key] > max)
      throw new Error(`${key}: nhập số nguyên từ ${min} đến ${max}`);
  }
  if (
    ![1, 2, 3].includes(c.scale) ||
    c.opacity < 0.3 ||
    c.opacity > 1 ||
    !Number.isFinite(c.opacity)
  )
    throw new Error('Kích thước hoặc độ trong suốt không hợp lệ');
  if (
    new Set(
      [c.translateShortcut, c.chatShortcut, c.toggleShortcut].map((v) =>
        v.toLowerCase().trim(),
      ),
    ).size !== 3
  )
    throw new Error('Các phím tắt không được trùng nhau');
  if (
    !c.providers.length ||
    c.providers.some(
      (p) => !['claude', 'google', 'libre', 'mymemory'].includes(p),
    )
  )
    throw new Error('Danh sách engine không hợp lệ');
  let url: URL;
  try {
    url = new URL(c.libreUrl);
  } catch {
    throw new Error('LibreTranslate URL không hợp lệ');
  }
  if (
    url.protocol !== 'https:' &&
    !(
      url.protocol === 'http:' &&
      ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname)
    )
  )
    throw new Error('LibreTranslate cần HTTPS hoặc localhost');
}
