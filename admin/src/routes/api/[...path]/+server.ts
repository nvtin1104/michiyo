import type { RequestHandler } from './$types';

// Only these routes are exposed through the Pages service binding. The Worker
// verifies the Access JWT itself; no administrative credential enters the browser.
const allowed: Record<string, RegExp[]> = {
  GET: [/^admin\/(devices|api-keys|provider-keys|usage|sync\/(summary|records))$/],
  POST: [/^pair\/start$/, /^admin\/api-keys$/],
  PUT: [/^admin\/provider-keys\/(anthropic|openai|openrouter)$/],
  PATCH: [/^admin\/devices\/[a-zA-Z0-9-]+$/],
  DELETE: [/^admin\/(devices|api-keys)\/[a-zA-Z0-9-]+$/, /^admin\/provider-keys\/(anthropic|openai|openrouter)$/],
};

const handle: RequestHandler = async ({ request, params, platform, url }) => {
  if (!allowed[request.method]?.some((pattern) => pattern.test(params.path))) return new Response('Not found', { status: 404 });
  if (request.method !== 'GET' && request.headers.get('origin') !== url.origin) return new Response('Forbidden', { status: 403 });
  const service = platform?.env?.MICHIYO_API;
  if (!service) return Response.json({ error: 'service_unavailable' }, { status: 503 });
  const jwt = request.headers.get('Cf-Access-Jwt-Assertion');
  if (!jwt) return Response.json({ error: 'unauthorized' }, { status: 401 });
  const headers = new Headers({ 'Cf-Access-Jwt-Assertion': jwt });
  if (request.headers.has('content-type')) headers.set('Content-Type', request.headers.get('content-type')!);
  const upstream = await service.fetch(new Request(`https://michiyo.internal/v1/${params.path}${url.search}`, {
    method: request.method,
    headers,
    body: request.method === 'GET' ? undefined : await request.text(),
  }));
  const responseHeaders = new Headers({ 'Cache-Control': 'no-store', 'Content-Type': upstream.headers.get('Content-Type') ?? 'application/json' });
  return new Response(upstream.body, { status: upstream.status, headers: responseHeaders });
};

export const GET = handle;
export const POST = handle;
export const PUT = handle;
export const PATCH = handle;
export const DELETE = handle;
