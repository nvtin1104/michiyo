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
    let events = [],
      due = [
        {
          reminder_id: 'r1',
          title: 'Uống thuốc',
          kind: 'event',
          occurrence_start: Date.now(),
        },
      ],
      next = 0,
      callback = 0;
    window.__calls = [];
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
        if (command === 'reminders_pending') return due;
        if (command === 'reminders_snooze' || command === 'reminders_dismiss') {
          window.__calls.push([command, args]);
          due = due.filter((d) => d.reminder_id !== args.id);
          return null;
        }
        if (command === 'set_timezone') {
          window.__tz = args.offset;
          return null;
        }
        if (command === 'events_range') {
          window.__range = args;
          return events
            .filter((e) => e.starts_at >= args.from && e.starts_at < args.until)
            .map((e) => ({
              ...e,
              occurrence_start: e.starts_at,
              occurrence_end: e.ends_at,
            }));
        }
        if (command === 'events_save') {
          const input = args.input;
          const event = {
            ...input,
            id: input.id || String(++next),
            rev: (input.rev || 0) + 1,
          };
          events = [event, ...events.filter((e) => e.id !== event.id)];
          window.__calls.push(['save', input]);
          return event;
        }
        if (command === 'events_set_done') {
          events = events.map((e) =>
            e.id === args.id ? { ...e, rev: e.rev + 1, done: args.done } : e,
          );
          window.__calls.push(['done', args.done]);
          return null;
        }
        if (command === 'events_delete') {
          events = events.filter((e) => e.id !== args.id);
          return null;
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=main#/calendar');
  await page.getByRole('heading', { name: /Lịch/ }).waitFor();
  assert.equal(
    await page.evaluate(() => typeof window.__range.tzOffset),
    'number',
  );

  await page.getByText('Uống thuốc').waitFor();
  await page.getByRole('button', { name: 'Hoãn 10 phút' }).click();
  await page.getByText('Uống thuốc').waitFor({ state: 'detached' });
  assert.deepEqual(await page.evaluate(() => window.__calls[0]), [
    'reminders_snooze',
    { id: 'r1', minutes: 10 },
  ]);
  assert.equal(typeof (await page.evaluate(() => window.__tz)), 'number');

  await page.getByRole('button', { name: '+ Tạo sự kiện' }).click();
  await page.getByLabel('Tiêu đề', { exact: true }).fill('Họp nhóm');
  await page.getByLabel('Nhắc nhở').selectOption('15');
  await page.getByLabel('Lặp lại').selectOption('FREQ=WEEKLY');
  await page.getByRole('button', { name: 'Lưu', exact: true }).click();
  await page.getByRole('button', { name: /Họp nhóm ↻/ }).waitFor();
  const saved = await page.evaluate(() => window.__calls[1][1]);
  assert.equal(saved.remind_before, 15);
  assert.equal(saved.rrule, 'FREQ=WEEKLY');
  assert.equal(saved.kind, 'event');

  await page.getByRole('button', { name: '+ Tạo sự kiện' }).click();
  await page.getByLabel('Tiêu đề', { exact: true }).fill('Nộp báo cáo');
  await page.getByLabel('Loại').selectOption('todo');
  await page.getByRole('button', { name: 'Lưu', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Hoàn thành Nộp báo cáo' }).check();
  assert.equal(await page.evaluate(() => window.__calls.at(-1)[1]), true);

  await page.getByRole('button', { name: 'Tuần', exact: true }).click();
  await page.getByRole('button', { name: /Họp nhóm/ }).waitFor();
  await page.getByRole('button', { name: 'Ngày', exact: true }).click();
  await page.getByRole('button', { name: /Họp nhóm/ }).click();
  await page.getByRole('button', { name: 'Xóa', exact: true }).click();
  await page
    .getByRole('button', { name: /Họp nhóm/ })
    .waitFor({ state: 'detached' });
  assert.deepEqual(errors, []);
  console.log(
    'PASS: calendar create recurring event, todo done, week/day views and delete (mock IPC).',
  );
} finally {
  await browser.close();
}
