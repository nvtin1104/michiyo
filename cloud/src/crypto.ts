const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function toBase64Url(bytes: Uint8Array): string {
  let binary = '';
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export function fromBase64Url(text: string): Uint8Array {
  const padded = text.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(text.length / 4) * 4, '=');
  const binary = atob(padded);
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

export async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', encoder.encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** 256-bit random secret with a recognisable prefix (`md_` device, `mk_` API key). */
export function randomToken(prefix: string): string {
  return prefix + toBase64Url(crypto.getRandomValues(new Uint8Array(32)));
}

const CODE_ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789'; // 32 symbols: no 0/O/1/I

/** Eight-symbol pairing code; 32 symbols means `byte & 31` is unbiased. */
export function pairingCode(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  return [...bytes].map((b) => CODE_ALPHABET[b & 31]).join('');
}

export function normalizeCode(code: string): string {
  return code.replace(/[\s-]/g, '').toUpperCase();
}

async function aesKey(secret: string): Promise<CryptoKey> {
  const raw = Uint8Array.from(atob(secret), (c) => c.charCodeAt(0));
  if (raw.length !== 32) throw new Error('ENCRYPTION_KEY must be 32 bytes, base64 encoded');
  return crypto.subtle.importKey('raw', raw, 'AES-GCM', false, ['encrypt', 'decrypt']);
}

/** `v1.<iv>.<ciphertext>`; a fresh random IV per value. */
export async function encrypt(secret: string, plaintext: string): Promise<string> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const data = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, await aesKey(secret), encoder.encode(plaintext));
  return `v1.${toBase64Url(iv)}.${toBase64Url(new Uint8Array(data))}`;
}

export async function decrypt(secret: string, stored: string): Promise<string> {
  const [version, iv, data] = stored.split('.');
  if (version !== 'v1' || !iv || !data) throw new Error('unsupported ciphertext');
  const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: fromBase64Url(iv) as BufferSource }, await aesKey(secret), fromBase64Url(data) as BufferSource);
  return decoder.decode(plain);
}
