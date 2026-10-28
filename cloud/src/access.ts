import { fromBase64Url } from './crypto';

type Jwk = JsonWebKey & { kid?: string };
type Certs = { keys: Jwk[] };

const decoder = new TextDecoder();
const cache = new Map<string, { at: number; certs: Certs }>();
const CACHE_MS = 60 * 60 * 1000;

async function loadCerts(team: string, fetcher: typeof fetch): Promise<Certs> {
  const hit = cache.get(team);
  if (hit && Date.now() - hit.at < CACHE_MS) return hit.certs;
  const response = await fetcher(`https://${team}/cdn-cgi/access/certs`);
  if (!response.ok) throw new Error('cannot load Access certificates');
  const certs = (await response.json()) as Certs;
  cache.set(team, { at: Date.now(), certs });
  return certs;
}

export function clearCertCache(): void {
  cache.clear();
}

/**
 * Verifies a Cloudflare Access JWT (RS256): signature, issuer, audience and expiry.
 * Returns the caller's email, or null for anything that does not check out.
 */
export async function verifyAccessJwt(
  token: string,
  options: { team: string; aud: string },
  fetcher: typeof fetch = fetch,
  now: number = Date.now(),
): Promise<string | null> {
  try {
    const [header, payload, signature] = token.split('.');
    if (!header || !payload || !signature) return null;
    const head = JSON.parse(decoder.decode(fromBase64Url(header)));
    if (head.alg !== 'RS256') return null;
    const certs = await loadCerts(options.team, fetcher);
    const jwk = certs.keys.find((k) => k.kid === head.kid);
    if (!jwk) return null;
    const key = await crypto.subtle.importKey('jwk', jwk, { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' }, false, ['verify']);
    const signed = new TextEncoder().encode(`${header}.${payload}`);
    const valid = await crypto.subtle.verify('RSASSA-PKCS1-v1_5', key, fromBase64Url(signature) as BufferSource, signed);
    if (!valid) return null;
    const claims = JSON.parse(decoder.decode(fromBase64Url(payload)));
    const audiences: unknown[] = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
    if (!audiences.includes(options.aud)) return null;
    if (claims.iss !== `https://${options.team}`) return null;
    if (typeof claims.exp !== 'number' || claims.exp * 1000 <= now) return null;
    if (typeof claims.nbf === 'number' && claims.nbf * 1000 > now) return null;
    return typeof claims.email === 'string' ? claims.email : 'access-user';
  } catch {
    return null;
  }
}
