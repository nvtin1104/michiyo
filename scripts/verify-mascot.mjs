import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 160, height: 160 },
    deviceScaleFactor: 1.25,
  });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  // IPC fixture verifies frontend interactions without using real keys or paid APIs.
  await page.addInitScript(() => {
    let counter = 0;
    const callbacks = new Map(),
      events = new Map();
    window.calls = [];
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'mascot' },
        currentWebview: { label: 'mascot' },
      },
      transformCallback(fn) {
        const id = ++counter;
        callbacks.set(id, fn);
        return id;
      },
      unregisterCallback(id) {
        callbacks.delete(id);
      },
      async invoke(command, args = {}) {
        if (command !== 'set_hit_mask') window.calls.push({ command, args });
        if (command === 'plugin:store|load') return 1;
        if (command === 'plugin:store|get') return [null, false];
        if (command === 'secret_status') return { claude: false };
        if (command === 'plugin:event|listen') {
          events.set(args.event, args.handler);
          return args.handler;
        }
        if (command === 'chat') {
          const handler = events.get('chat-delta');
          callbacks.get(handler)?.({
            payload: {
              id: args.id,
              text: 'Câu trả lời kiểm thử ngay trong bong bóng.',
            },
          });
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=mascot');
  await page.locator('canvas').waitFor();
  await page.mouse.click(80, 80, { button: 'right' });
  assert.equal(
    await page.evaluate(
      () => window.calls.filter((c) => c.command === 'mascot_menu').length,
    ),
    1,
  );
  assert.equal(
    await page.evaluate(
      () => window.calls.filter((c) => c.command === 'open_panel').length,
    ),
    0,
  );
  await page.mouse.click(80, 80);
  assert.deepEqual(
    await page.evaluate(() =>
      window.calls
        .filter((c) => c.command === 'open_panel')
        .map((c) => c.args.label),
    ),
    ['chat'],
  );
  assert.equal(
    await page.evaluate(
      () =>
        document.documentElement.scrollWidth > innerWidth ||
        document.documentElement.scrollHeight > innerHeight,
    ),
    false,
  );
  await page.setViewportSize({ width: 360, height: 340 });
  await page.goto('http://127.0.0.1:1420/?view=chat');
  const input = page.getByRole('textbox', { name: 'Tin nhắn' });
  await input.waitFor();
  await page.waitForFunction(
    () => document.activeElement?.tagName === 'TEXTAREA',
  );
  await input.fill('Xin chào');
  await input.press('Enter');
  await page
    .getByText('Câu trả lời kiểm thử ngay trong bong bóng.', { exact: true })
    .waitFor();
  assert.equal(
    await page.getByRole('heading', { name: /Trò chuyện/ }).count(),
    0,
  );
  await page.screenshot({ path: 'artifacts/chat-bubble.png' });
  assert.deepEqual(errors, []);
  console.log(
    'PASS: right click only opens menu; left click opens chat without a key; no mascot overflow; composer autofocus and streamed answer in compact bubble (mock IPC).',
  );
} finally {
  await browser.close();
}
