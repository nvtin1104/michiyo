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
    let habits = [],
      next = 0,
      callback = 0;
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
        if (command === 'notes_list') return [];
        if (command === 'habits_list') {
          window.__lastToday = args.today;
          return habits;
        }
        if (command === 'habits_save') {
          const input = args.input;
          const habit = {
            id: input.id || String(++next),
            rev: (input.rev || 0) + 1,
            title: input.title,
            frequency: input.frequency,
            streak: 0,
            done_today: false,
            dates: [],
          };
          habits = [habit, ...habits.filter((h) => h.id !== habit.id)];
          window.__saved = input;
          return habit;
        }
        if (command === 'habits_check_in') {
          habits = habits.map((h) =>
            h.id === args.id
              ? {
                  ...h,
                  done_today: args.done,
                  streak: args.done ? 1 : 0,
                  dates: args.done ? [args.date] : [],
                }
              : h,
          );
          window.__checkin = args;
          return null;
        }
        if (command === 'habits_delete') {
          habits = habits.filter((h) => h.id !== args.id);
          return null;
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=main#/habits');
  await page.getByRole('button', { name: '+ Thói quen mới' }).click();
  await page.getByLabel('Tên thói quen').fill('Đọc sách');
  await page.getByLabel('Tần suất').selectOption('weekly');
  await page.getByLabel('Số lần mỗi tuần').fill('4');
  await page.getByRole('button', { name: 'Lưu', exact: true }).click();
  await page.getByText('4 lần mỗi tuần').waitFor();
  assert.equal(await page.evaluate(() => window.__saved.frequency), 'weekly:4');
  assert.match(await page.evaluate(() => window.__lastToday), /^\d{4}-\d{2}-\d{2}$/);

  const toggle = page.getByRole('button', { name: 'Check-in Đọc sách' });
  await toggle.click();
  await page.getByText('🔥 1').waitFor();
  assert.equal(await page.evaluate(() => window.__checkin.done), true);
  assert.equal(await toggle.getAttribute('aria-pressed'), 'true');
  await toggle.click();
  await page.getByText('🔥 0').waitFor();
  assert.equal(await page.evaluate(() => window.__checkin.done), false);

  await page.getByRole('button', { name: 'Xóa', exact: true }).click();
  await page.getByText('Chưa có thói quen nào').waitFor();
  assert.deepEqual(errors, []);
  console.log('PASS: habits create, check-in toggle, streak display and delete (mock IPC).');
} finally {
  await browser.close();
}
