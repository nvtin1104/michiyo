import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
const browser = await chromium.launch({ channel: 'chrome', headless: true });
await fs.mkdir('artifacts/companions', { recursive: true });
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
        // Keep the mocked native drag open long enough to see the drag pose.
        if (command === 'plugin:window|start_dragging')
          return new Promise((done) => setTimeout(done, 1200));
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
  const rezeLoaded = page.waitForResponse(
    (r) => r.url().includes('/skins/reze/spritesheet.png') && r.ok(),
  );
  await page.goto('http://127.0.0.1:1420/?view=mascot');
  await rezeLoaded;
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
    () => window.calls.filter((c) => c.command === 'open_main').length,
  );
  await page.mouse.move(240, 170);
  await page.mouse.down();
  await page.waitForTimeout(550);
  assert.equal(await mascot.getAttribute('data-pose'), 'petting');
  await page.screenshot({ path: 'artifacts/companions/default-petting.png' });
  await page.mouse.up();
  assert.equal(
    await page.evaluate(
      () => window.calls.filter((c) => c.command === 'open_main').length,
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
      () => window.calls.filter((c) => c.command === 'open_main').length,
    ),
    opened,
  );
  // Reze is bundled, and the original mascot remains selectable.
  const moon = page.waitForResponse(
    (r) => r.url().includes('/skins/default/spritesheet.png') && r.ok(),
  );
  await page.evaluate(() => {
    window.config = { skin: 'moon-gardener' };
    window.emitMockEvent('config-changed');
  });
  await moon;
  await page.evaluate(() => {
    window.config = { skin: 'reze' };
    window.emitMockEvent('config-changed');
  });
  await page.waitForFunction(
    () => document.querySelector('.sprite')?.dataset.skin === 'reze',
  );
  assert.equal(await page.locator('[data-renderer="three"]').count(), 0);
  assert.deepEqual(errors, []);
  console.log(
    'PASS: mascot interactions and switching between bundled Reze and Moon gardener skins.',
  );
} finally {
  await browser.close();
}
