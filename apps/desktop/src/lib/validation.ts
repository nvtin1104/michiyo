import type { Config } from './api';
const isLocal = (value: string) => {
  try {
    return ['localhost', '127.0.0.1', '[::1]'].includes(
      new URL(value).hostname,
    );
  } catch {
    return false;
  }
};
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
      [c.chatShortcut, c.toggleShortcut, c.shellShortcut].map((v) =>
        v.toLowerCase().trim(),
      ),
    ).size !== 3
  )
    throw new Error('Các phím tắt không được trùng nhau');
  if (!['offline', 'online', 'hybrid'].includes(c.mode))
    throw new Error('Chế độ không hợp lệ');
  if (c.theme && !['dark', 'light'].includes(c.theme))
    throw new Error('Giao diện không hợp lệ');
  for (const [label, value] of [
    ['Địa chỉ model', c.openaiBase],
    ['Địa chỉ model cục bộ', c.localBase],
  ] as const) {
    let parsed: URL;
    try {
      parsed = new URL(value);
    } catch {
      throw new Error(`${label} không hợp lệ`);
    }
    if (
      parsed.protocol !== 'https:' &&
      !(
        parsed.protocol === 'http:' &&
        ['localhost', '127.0.0.1', '[::1]'].includes(parsed.hostname)
      )
    )
      throw new Error(`${label} cần HTTPS hoặc localhost`);
  }
  if (c.cloudUrl.trim()) {
    let cloud: URL;
    try {
      cloud = new URL(c.cloudUrl);
    } catch {
      throw new Error('Địa chỉ Michiyo Cloud không hợp lệ');
    }
    if (
      cloud.protocol !== 'https:' &&
      !(
        cloud.protocol === 'http:' &&
        ['localhost', '127.0.0.1', '[::1]'].includes(cloud.hostname)
      )
    )
      throw new Error('Địa chỉ Michiyo Cloud cần HTTPS hoặc localhost');
  }
  if (c.mode === 'offline' && !c.localModel.trim() && !isLocal(c.openaiBase))
    throw new Error('Chế độ Offline cần model chạy trên máy này');
}
