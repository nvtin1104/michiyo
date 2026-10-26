import { afterEach, expect, it, vi } from 'vitest';
import type { SDKMessage } from '@anthropic-ai/claude-agent-sdk';
import {
  runAgent,
  optionsFor,
  friendlyError,
  type Request,
  type Event,
} from './runtime';
const request: Request = {
  action: 'chat',
  apiKey: 'unit-test-key-not-real',
  model: 'claude-haiku-5-5',
  system: 'Be friendly',
  maxTokens: 128,
  messages: [{ role: 'user', content: 'Xin chào' }],
};
const message = (value: unknown) => value as SDKMessage;
const options = (controller = new AbortController()) =>
  optionsFor(request, controller, 'workspace', 'isolated-config', 'claude.exe');
afterEach(() => vi.unstubAllEnvs());
it('streams real deltas once and waits for the result event', async () => {
  const events: Event[] = [];
  await runAgent(
    request,
    options(),
    (e) => events.push(e),
    async function* () {
      yield message({
        type: 'stream_event',
        event: {
          type: 'content_block_delta',
          delta: { type: 'text_delta', text: 'Chào ' },
        },
      });
      expect(events).toEqual([{ type: 'delta', text: 'Chào ' }]);
      yield message({
        type: 'stream_event',
        event: {
          type: 'content_block_delta',
          delta: { type: 'text_delta', text: 'bạn 🌱' },
        },
      });
      yield message({
        type: 'assistant',
        message: { content: [{ type: 'text', text: 'Chào bạn 🌱' }] },
      });
      yield message({
        type: 'result',
        subtype: 'success',
        is_error: false,
        result: 'Chào bạn 🌱',
      });
    },
  );
  expect(events).toEqual([
    { type: 'delta', text: 'Chào ' },
    { type: 'delta', text: 'bạn 🌱' },
    { type: 'done', text: 'Chào bạn 🌱' },
  ]);
});
it('uses a successful result when the SDK did not emit partial events', async () => {
  const events: Event[] = [];
  await runAgent(
    { ...request, action: 'translate' },
    options(),
    (e) => events.push(e),
    async function* () {
      yield message({
        type: 'result',
        subtype: 'success',
        is_error: false,
        result: 'Hello',
      });
    },
  );
  expect(events).toEqual([{ type: 'done', text: 'Hello' }]);
});
it('fails closed on a stream ending without result', async () => {
  const events: Event[] = [];
  await runAgent(
    request,
    options(),
    (e) => events.push(e),
    async function* () {
      yield message({
        type: 'stream_event',
        event: {
          type: 'content_block_delta',
          delta: { type: 'text_delta', text: 'Partial' },
        },
      });
    },
  );
  expect(events.at(-1)?.type).toBe('error');
});
it('returns cancelled instead of completed after abort', async () => {
  const controller = new AbortController();
  const events: Event[] = [];
  await runAgent(
    request,
    options(controller),
    (e) => events.push(e),
    async function* () {
      controller.abort();
      yield message({ type: 'result', subtype: 'success', result: 'ignored' });
    },
  );
  expect(events).toEqual([{ type: 'cancelled' }]);
});
it('disables tools, filesystem settings and inherited credentials', async () => {
  vi.stubEnv('ANTHROPIC_BASE_URL', 'https://untrusted.example');
  vi.stubEnv('ANTHROPIC_AUTH_TOKEN', 'inherited-token');
  vi.stubEnv('NODE_OPTIONS', '--inspect');
  const settings = options();
  expect(settings.tools).toEqual([]);
  expect(settings.settingSources).toEqual([]);
  expect(settings.skills).toEqual([]);
  expect(settings.persistSession).toBe(false);
  expect(settings.strictMcpConfig).toBe(true);
  expect(settings.env?.ANTHROPIC_API_KEY).toBe(request.apiKey);
  for (const key of [
    'ANTHROPIC_BASE_URL',
    'ANTHROPIC_AUTH_TOKEN',
    'NODE_OPTIONS',
  ])
    expect(settings.env?.[key]).toBeUndefined();
  expect(await (settings.canUseTool as Function)('Bash', {}, {})).toMatchObject(
    { behavior: 'deny' },
  );
});
it('never echoes raw errors that could contain a credential', () => {
  const error = friendlyError(
    new Error(`401 invalid api key ${request.apiKey}`),
  );
  expect(error).toMatch(/API key/);
  expect(error).not.toContain(request.apiKey);
});
