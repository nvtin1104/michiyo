import { createInterface } from 'node:readline';
import { mkdtemp, mkdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runAgent, optionsFor, type Request, type Event } from './runtime';

const lines = createInterface({ input: process.stdin, terminal: false });
const controller = new AbortController();
let started = false;
const emit = (event: Event) =>
  process.stdout.write(JSON.stringify(event) + '\n');
lines.on('line', async (line) => {
  if (started) {
    if (line === '{"cancel":true}') controller.abort();
    return;
  }
  started = true;
  let folder = '';
  try {
    const request = JSON.parse(line) as Request;
    if (
      !['chat', 'translate', 'check'].includes(request.action) ||
      typeof request.apiKey !== 'string' ||
      !request.apiKey ||
      typeof request.model !== 'string' ||
      typeof request.system !== 'string' ||
      !Array.isArray(request.messages) ||
      !request.messages.length ||
      request.messages.some(
        (m) =>
          !['user', 'assistant'].includes(m.role) ||
          typeof m.content !== 'string',
      ) ||
      !Number.isInteger(request.maxTokens) ||
      request.maxTokens < 1 ||
      request.maxTokens > 16384
    )
      throw new Error('Invalid request');
    folder = await mkdtemp(join(tmpdir(), 'michiyo-agent-'));
    const cwd = join(folder, 'workspace'),
      config = join(folder, 'config');
    await Promise.all([mkdir(cwd), mkdir(config)]);
    const cli = join(
      dirname(fileURLToPath(import.meta.url)),
      process.platform === 'win32' ? 'claude.exe' : 'claude',
    );
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, 120_000);
    try {
      await runAgent(
        request,
        optionsFor(request, controller, cwd, config, cli),
        (event) =>
          emit(
            event.type === 'cancelled' && timedOut
              ? {
                  type: 'error',
                  message: 'Claude Agent SDK phản hồi quá lâu. Hãy thử lại.',
                }
              : event,
          ),
      );
    } finally {
      clearTimeout(timer);
    }
  } catch {
    emit({
      type: 'error',
      message:
        'Không khởi động được Claude Agent SDK. Hãy kiểm tra cài đặt ứng dụng.',
    });
  } finally {
    if (folder)
      await rm(folder, { recursive: true, force: true }).catch(() => {});
    lines.close();
    process.stdin.pause();
  }
});
lines.on('close', () => {
  if (!started) process.exitCode = 1;
});
