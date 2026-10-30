import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';

const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({ viewport: { width: 720, height: 540 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.addInitScript(() => {
    let id = 0;
    const data = new Map();
    window.__saved = [];
    window.__syncNow = 0;
    const status = {
      enabled: true,
      pending: 3,
      last_sync_at: null,
      last_error: null,
      syncing: false,
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'settings' },
        currentWebview: { label: 'settings' },
      },
      transformCallback() {
        return ++id;
      },
      unregisterCallback() {},
      async invoke(command, args = {}) {
        if (command === 'plugin:store|load') return 1;
        if (command === 'plugin:store|get')
          return [data.get(args.key), data.has(args.key)];
        if (command === 'plugin:store|set') {
          data.set(args.key, args.value);
          window.__saved.push(args.value);
        }
        if (command === 'plugin:event|listen') return ++id;
        if (command === 'secret_status') return {};
        if (command === 'list_skins')
          return { names: ['default'], directory: 'C:/example/skins' };
        if (command === 'sync_status') return { ...status };
        if (command === 'sync_now') {
          window.__syncNow++;
          status.pending = 0;
          status.last_sync_at = Date.now();
          return { ...status };
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=settings');
  await page.getByRole('heading', { name: 'Cài đặt', exact: true }).waitFor();
  await page.locator('nav button').filter({ hasText: 'Đồng bộ' }).click();
  await page.getByText('Thay đổi chờ gửi: 3').waitFor();
  assert.match(
    await page.locator('section').first().innerText(),
    /không bao giờ được đồng bộ/,
  );

  await page.getByRole('button', { name: 'Đồng bộ ngay' }).click();
  await page.getByText('Thay đổi chờ gửi: 0').waitFor();
  assert.equal(await page.evaluate(() => window.__syncNow), 1);

  await page.getByRole('checkbox', { name: 'Ghi chú' }).uncheck();
  await page.getByText('Đã lưu thay đổi').waitFor();
  const saved = await page.evaluate(() => window.__saved.at(-1));
  assert.deepEqual(saved.syncScopes, {
    notes: false,
    habits: true,
    calendar: true,
    activity: true,
  });
  assert.deepEqual(errors, []);
  console.log(
    'PASS: sync tab shows status, runs a manual sync and saves scope choices (mock IPC).',
  );
} finally {
  await browser.close();
}
