export interface Env {
  DB: D1Database;
  AI: Ai;
  /** Base64, 32 bytes. Encrypts provider keys at rest. */
  ENCRYPTION_KEY: string;
  /** Cloudflare Access: `<team>.cloudflareaccess.com` and the application's AUD tag. */
  ACCESS_TEAM_DOMAIN?: string;
  ACCESS_AUD?: string;
  /** Local development only; never set in production. */
  DEV_ADMIN_TOKEN?: string;
  ENVIRONMENT?: string;
  AI_GATEWAY_ID?: string;
  DEFAULT_CHAT_MODEL?: string;
  ALLOWED_CHAT_MODELS?: string;
  TELEGRAM_WEBHOOK_SECRET?: string;
  TELEGRAM_BOT_TOKEN?: string;
  /** Comma-separated Telegram private chat IDs, never empty in an enabled bot. */
  TELEGRAM_ALLOWED_CHAT_IDS?: string;
  TELEGRAM_QUEUE?: Queue<{ updateId: number; payload: string; receivedAt: number }>;
}

export const DEFAULT_MODEL = '@cf/meta/llama-3.1-8b-instruct';
export const EMBED_MODEL = '@cf/baai/bge-m3';

export function allowedModels(env: Env): string[] {
  const list = (env.ALLOWED_CHAT_MODELS ?? '')
    .split(',')
    .map((m) => m.trim())
    .filter(Boolean);
  return list.length ? list : [env.DEFAULT_CHAT_MODEL || DEFAULT_MODEL];
}
