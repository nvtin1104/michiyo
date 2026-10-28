import { sha256Hex } from './crypto';
import type { Env } from './env';

const MAX_BYTES = 96_000;
const encoder = new TextEncoder();

function same(a: Uint8Array, b: Uint8Array): boolean {
  let difference = a.length ^ b.length;
  for (let i = 0; i < Math.max(a.length, b.length); i++) difference |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return difference === 0;
}

export function verifySubscription(url: URL, env: Env): Response {
  const supplied = url.searchParams.get('hub.verify_token') ?? '';
  const challenge = url.searchParams.get('hub.challenge');
  if (!env.IG_VERIFY_TOKEN || url.searchParams.get('hub.mode') !== 'subscribe' || challenge === null ||
      !same(encoder.encode(supplied), encoder.encode(env.IG_VERIFY_TOKEN))) return new Response('Forbidden', { status: 403 });
  return new Response(challenge, { headers: { 'Content-Type': 'text/plain; charset=utf-8', 'Cache-Control': 'no-store' } });
}

async function readBounded(request: Request): Promise<Uint8Array | null> {
  if (!request.body) return new Uint8Array();
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    length += value.length;
    if (length > MAX_BYTES) { await reader.cancel(); return null; }
    chunks.push(value);
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return bytes;
}

export async function receiveWebhook(request: Request, env: Env): Promise<Response> {
  if (!env.IG_APP_SECRET || !env.IG_QUEUE) return new Response('Unavailable', { status: 503 });
  const signature = request.headers.get('X-Hub-Signature-256') ?? '';
  if (!/^sha256=[0-9a-f]{64}$/i.test(signature)) return new Response('Forbidden', { status: 403 });
  const body = await readBounded(request);
  if (!body) return new Response('Payload too large', { status: 413 });
  const key = await crypto.subtle.importKey('raw', encoder.encode(env.IG_APP_SECRET), { name: 'HMAC', hash: 'SHA-256' }, false, ['verify']);
  const digest = new Uint8Array(signature.slice(7).match(/../g)!.map((hex) => parseInt(hex, 16)));
  if (!(await crypto.subtle.verify('HMAC', key, digest, body as BufferSource))) return new Response('Forbidden', { status: 403 });
  let payload: string;
  try {
    payload = new TextDecoder('utf-8', { fatal: true, ignoreBOM: false }).decode(body);
    const value: unknown = JSON.parse(payload);
    if (!value || typeof value !== 'object' || (value as { object?: unknown }).object !== 'instagram' || !Array.isArray((value as { entry?: unknown }).entry)) throw new Error('invalid');
  } catch { return new Response('Invalid payload', { status: 400 }); }
  try { await env.IG_QUEUE.send({ payload, receivedAt: Date.now() }); }
  catch { return new Response('Unavailable', { status: 503 }); }
  return new Response('EVENT_RECEIVED', { status: 200 });
}

export async function storeDelivery(env: Env, delivery: { payload: string; receivedAt: number }): Promise<void> {
  const digest = await sha256Hex(delivery.payload);
  await env.DB.prepare('INSERT OR IGNORE INTO ig_webhook_inbox (digest, received_at, payload) VALUES (?, ?, ?)')
    .bind(digest, delivery.receivedAt, delivery.payload).run();
}
