import { describe, expect, it, vi } from 'vitest';
import { GET, POST, DELETE } from './[...path]/+server';

function context(method: string, path: string, options: { origin?: string; jwt?: string; service?: boolean } = {}) {
  const fetch = vi.fn(async (request: Request) => Response.json({ url: request.url, jwt: request.headers.get('Cf-Access-Jwt-Assertion') }));
  const headers = new Headers();
  if (options.origin) headers.set('Origin', options.origin);
  if (options.jwt) headers.set('Cf-Access-Jwt-Assertion', options.jwt);
  const url = new URL(`https://admin.example/api/${path}`);
  return { event: { request: new Request(url, { method, headers }), params: { path }, url, platform: options.service ? { env: { MICHIYO_API: { fetch } } } : undefined } as never, fetch };
}

describe('admin service bridge', () => {
  it('requires an Access assertion and forwards it only to allowed routes', async () => {
    const missing = context('GET', 'admin/devices', { service: true });
    expect((await GET(missing.event)).status).toBe(401);
    expect(missing.fetch).not.toHaveBeenCalled();
    const good = context('GET', 'admin/devices', { service: true, jwt: 'assertion' });
    const response = await GET(good.event);
    expect(await response.json()).toEqual({ url: 'https://michiyo.internal/v1/admin/devices', jwt: 'assertion' });
    expect(response.headers.get('cache-control')).toBe('no-store');
    const bad = context('GET', 'ai/chat', { service: true, jwt: 'assertion' });
    expect((await GET(bad.event)).status).toBe(404);
  });
  it('rejects cross-origin writes and does not expose bulk deletion', async () => {
    const cross = context('POST', 'pair/start', { service: true, jwt: 'assertion', origin: 'https://evil.example' });
    expect((await POST(cross.event)).status).toBe(403);
    expect(cross.fetch).not.toHaveBeenCalled();
    const wipe = context('DELETE', 'admin/sync', { service: true, jwt: 'assertion', origin: 'https://admin.example' });
    expect((await DELETE(wipe.event)).status).toBe(404);
    const unavailable = context('GET', 'admin/devices', { jwt: 'assertion' });
    expect((await GET(unavailable.event)).status).toBe(503);
  });
});
