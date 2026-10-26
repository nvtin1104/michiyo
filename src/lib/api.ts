import { invoke } from '@tauri-apps/api/core';
export const desktop =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
export async function call<T>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!desktop)
    throw new Error(
      'Chức năng này cần mở bằng ứng dụng desktop (npm run tauri dev).',
    );
  return invoke<T>(command, args);
}
export const defaults = {
  locale: 'vi',
  autostart: false,
  scale: 1,
  anchor: 'bottom-right',
  opacity: 1,
  model: 'claude-haiku-5-5',
  systemPrompt:
    'Bạn là Michiyo, trợ lý cá nhân thân thiện. Trả lời rõ ràng bằng ngôn ngữ của người dùng.',
  maxTokens: 2048,
  historyLimit: 20,
  source: 'auto',
  target: 'vi',
  alternate: 'en',
  providers: ['mymemory', 'libre', 'google', 'claude'],
  fallback: true,
  domain: 'Tổng quát',
  tone: 'Tự nhiên',
  clipboardWatch: false,
  libreUrl: 'https://libretranslate.com',
  translateShortcut: 'Ctrl+Alt+T',
  chatShortcut: 'Ctrl+Alt+C',
  toggleShortcut: 'Ctrl+Alt+M',
  skin: 'default',
  animationSpeed: 1,
  reducedMotion: false,
  sleepMinutes: 5,
  live2d: false,
};
export type Config = typeof defaults;
