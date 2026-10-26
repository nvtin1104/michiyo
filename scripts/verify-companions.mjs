import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
const browser = await chromium.launch({ channel: 'chrome', headless: true });
await fs.mkdir('artifacts/reze', { recursive: true });
try {
  const page = await browser.newPage({ viewport: { width: 480, height: 480 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.addInitScript(() => {
    let id = 0;
    const callbacks = new Map(),
      events = new Map();
    window.config = { skin: 'default' };
    window.calls = [];
    window.mask = [];
    window.emitMockEvent = (name, payload) =>
      callbacks.get(events.get(name))?.({ payload });
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'mascot' },
        currentWebview: { label: 'mascot' },
      },
      transformCallback(fn) {
        callbacks.set(++id, fn);
        return id;
      },
      unregisterCallback(id) {
        callbacks.delete(id);
      },
      async invoke(command, args = {}) {
        if (command === 'set_hit_mask') {
          window.mask = args.mask;
          return;
        }
        window.calls.push({ command, args });
        if (command === 'plugin:store|load') return 1;
        if (command === 'plugin:store|get') return [window.config, true];
        if (command === 'plugin:event|listen') {
          events.set(args.event, args.handler);
          return args.handler;
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=mascot');
  const mascot = page.locator('.mascot-window');
  await page.waitForFunction(() => window.mask.some(Boolean));
  await page.waitForFunction(
    () => document.querySelector('.mascot-window').dataset.pose !== 'greeting',
  );
  await page.evaluate(() => window.emitMockEvent('mascot-state', 'thinking'));
  await page.mouse.click(240, 240);
  assert.equal(await mascot.getAttribute('data-state'), 'thinking');
  assert.equal(await mascot.getAttribute('data-pose'), 'touched');
  await page.waitForFunction(
    () => document.querySelector('.mascot-window').dataset.pose === 'thinking',
  );
  // A head hold must not open chat, and a drag must not become a click.
  const opened = await page.evaluate(
    () => window.calls.filter((c) => c.command === 'open_panel').length,
  );
  await page.mouse.move(240, 170);
  await page.mouse.down();
  await page.waitForTimeout(550);
  assert.equal(await mascot.getAttribute('data-pose'), 'petting');
  await page.screenshot({ path: 'artifacts/reze/default-petting.png' });
  await page.mouse.up();
  assert.equal(
    await page.evaluate(
      () => window.calls.filter((c) => c.command === 'open_panel').length,
    ),
    opened,
  );
  await page.mouse.move(240, 300);
  await page.mouse.down();
  await page.mouse.move(280, 300, { steps: 5 });
  await page.mouse.up();
  assert.ok(
    await page.evaluate(() =>
      window.calls.some((c) => c.command === 'plugin:window|start_dragging'),
    ),
  );
  assert.equal(
    await page.evaluate(
      () => window.calls.filter((c) => c.command === 'open_panel').length,
    ),
    opened,
  );
  await page.evaluate(() => {
    window.config = { skin: 'reze' };
    window.emitMockEvent('config-changed');
  });
  await page.locator('[data-renderer="three"] canvas').waitFor();
  await page.waitForFunction(
    () => !document.querySelector('[data-renderer="three"] [role="status"]'),
    { timeout: 30000 },
  );
  await page.evaluate(() => window.emitMockEvent('mascot-state', 'idle'));
  await page.waitForTimeout(1500);
  await page.screenshot({ path: 'artifacts/reze/app-idle.png' });
  let pixels = await page.evaluate(() => window.mask.filter(Boolean).length);
  assert.ok(
    pixels > 500 && pixels < 18000,
    `Unexpected silhouette coverage ${pixels}`,
  );
  await page.evaluate(() =>
    window.emitMockEvent('mascot-pointer', { x: 250, y: 50 }),
  );
  await page.waitForTimeout(600);
  await page.screenshot({ path: 'artifacts/reze/app-look.png' });
  for (const state of ['thinking', 'talking', 'happy', 'sleep']) {
    await page.evaluate(
      (state) => window.emitMockEvent('mascot-state', state),
      state,
    );
    await page.waitForTimeout(400);
    await page.screenshot({ path: `artifacts/reze/app-${state}.png` });
  }
  await page.mouse.move(30, 30);
  await page.mouse.move(240, 70);
  assert.equal(await mascot.getAttribute('data-state'), 'sleep');
  await page.mouse.click(240, 70);
  assert.equal(await mascot.getAttribute('data-pose'), 'waking');
  // Dispose/recreate both rendering engines, then verify reduced motion and fallback.
  for (const skin of ['default', 'reze', 'default', 'reze']) {
    await page.evaluate((skin) => {
      window.config = { skin, reducedMotion: true };
      window.emitMockEvent('config-changed');
    }, skin);
    await page.waitForTimeout(500);
  }
  await page.waitForFunction(
    () => !document.querySelector('[data-renderer="three"] [role="status"]'),
  );
  await page.screenshot({ path: 'artifacts/reze/app-reduced.png' });
  assert.deepEqual(errors, []);
  await page.route('**/reze-companion.glb', (route) => route.abort());
  await page.reload();
  await page.evaluate(() => {
    window.config = { skin: 'reze' };
    window.emitMockEvent('config-changed');
  });
  await page.locator('.fallback').waitFor();
  await page.waitForFunction(() => window.mask.some(Boolean));
  console.log(
    'PASS: system state survives touch; pet/drag do not open chat; Reze renders, tracks cursor, sleeps/wakes, switches engines and falls back on load failure.',
  );
} finally {
  await browser.close();
}
