import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';

const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 1000, height: 720 },
  });
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.addInitScript(() => {
    let callback = 0;
    let habits = [
      { id: 'h1', title: 'Đọc sách', streak: 4, done_today: false },
      { id: 'h2', title: 'Tập thể dục', streak: 1, done_today: true },
    ];
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { label: 'main' },
      },
      transformCallback() {
        return ++callback;
      },
      unregisterCallback() {},
      async invoke(command, args = {}) {
        if (command === 'plugin:store|load') return 1;
        if (command === 'plugin:store|get') return [null, false];
        if (command === 'plugin:event|listen') return 1;
        if (command === 'notes_list')
          return [
            { id: 'n1', title: 'Ý tưởng', pinned: true, tags: [], body: '' },
            { id: 'n2', title: 'Nháp', pinned: false, tags: [], body: '' },
          ];
        if (command === 'events_range')
          return [
            {
              id: 'e1',
              title: 'Họp nhóm',
              kind: 'event',
              done: false,
              occurrence_start: args.from + 9 * 36e5,
            },
          ];
        if (command === 'habits_list') return habits;
        if (command === 'habits_check_in') {
          window.__checkin = args;
          habits = habits.map((h) =>
            h.id === args.id ? { ...h, done_today: true } : h,
          );
          return null;
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=main#/home');
  await page.getByText('Họp nhóm').waitFor();
  await page.getByText('📌 Ý tưởng').waitFor();
  assert.equal(await page.getByText('Nháp').count(), 0);
  assert.equal(await page.getByText('Tập thể dục').count(), 0);
  await page.getByRole('button', { name: 'Check-in Đọc sách' }).click();
  await page.getByText('Đã hoàn thành hết').waitFor();
  assert.match(
    await page.evaluate(() => window.__checkin.date),
    /^\d{4}-\d{2}-\d{2}$/,
  );
  assert.deepEqual(errors, []);
  console.log(
    'PASS: home shows today agenda, unchecked habits with quick check-in, and pinned notes (mock IPC).',
  );
} finally {
  await browser.close();
}
