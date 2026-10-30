import { PullResponse, PushRequest, PushResponse, type SyncRecord } from './sync';

export interface CloudClientOptions {
  baseUrl: string;
  token?: string;
  fetchImpl?: typeof fetch;
}

/**
 * Platform-agnostic Data Server client shared across Web, Mobile, and tests.
 */
export class CloudClient {
  private readonly baseUrl: string;
  private token: string | undefined;
  private readonly fetchImpl: typeof fetch;

  constructor(options: CloudClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, '');
    this.token = options.token;
    this.fetchImpl = options.fetchImpl ?? fetch;
  }

  setToken(token: string | undefined): void {
    this.token = token;
  }

  private headers(extra: Record<string, string> = {}): Record<string, string> {
    const h: Record<string, string> = { ...extra };
    if (this.token) h.Authorization = `Bearer ${this.token}`;
    return h;
  }

  async pairComplete(code: string, name?: string): Promise<{ device_id: string; token: string }> {
    const res = await this.fetchImpl(`${this.baseUrl}/v1/pair/complete`, {
      method: 'POST',
      headers: this.headers({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ code, ...(name ? { name } : {}) }),
    });
    const body = (await res.json().catch(() => ({}))) as { error?: string; device_id?: string; token?: string };
    if (!res.ok || !body.token || !body.device_id) {
      throw new Error(body.error || `HTTP ${res.status}`);
    }
    this.token = body.token;
    return { device_id: body.device_id, token: body.token };
  }

  async push(records: SyncRecord[]): Promise<PushResponse> {
    const payload = PushRequest.parse({ records });
    const res = await this.fetchImpl(`${this.baseUrl}/v1/sync/push`, {
      method: 'POST',
      headers: this.headers({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(payload),
    });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) {
      throw new Error((body as { error?: string }).error || `HTTP ${res.status}`);
    }
    return PushResponse.parse(body);
  }

  async pull(cursor = '0'): Promise<PullResponse & { more?: boolean }> {
    const res = await this.fetchImpl(`${this.baseUrl}/v1/sync/pull?cursor=${encodeURIComponent(cursor)}`, {
      method: 'GET',
      headers: this.headers(),
    });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) {
      throw new Error((body as { error?: string }).error || `HTTP ${res.status}`);
    }
    const parsed = PullResponse.parse(body);
    return { ...parsed, more: Boolean((body as { more?: boolean }).more) };
  }
}
