import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 1000, height: 720 },
  });
  await page.addInitScript(() => {
    let activities = [
        {
          id: '1',
          kind: 'note_saved',
          occurred_at: new Date('2026-10-28T12:00:00').getTime(),
          data: { title: 'Nhật ký' },
        },
      ],
      cb = 0;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { label: 'main' },
      },
      transformCallback() {
        return ++cb;
      },
      unregisterCallback() {},
      async invoke(command, args = {}) {
        if (command === 'plugin:store|load') return 1;
        if (command === 'plugin:store|get') return [null, false];
        if (command === 'plugin:event|listen') return 1;
        if (command === 'notes_list') return [];
        if (command === 'activity_list')
          return activities.filter(
            (x) =>
              (!args.kind || x.kind === args.kind) &&
              (args.from === null || x.occurred_at >= args.from) &&
              (args.until === null || x.occurred_at < args.until),
          );
        if (command === 'activity_delete') {
          activities = activities.filter((x) => !args.ids.includes(x.id));
          return null;
        }
        if (command === 'activity_delete_range') {
          activities = activities.filter(
            (x) => x.occurred_at < args.from || x.occurred_at >= args.until,
          );
          return 1;
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=main#/history');
  await page.getByText('Đã lưu ghi chú · Nhật ký').waitFor();
  await page.getByLabel('Loại').selectOption('note_deleted');
  await page.getByRole('button', { name: 'Lọc' }).click();
  await page.getByText('Chưa có hoạt động phù hợp.').waitFor();
  await page.getByLabel('Loại').selectOption('');
  await page.getByLabel('Từ ngày').fill('2026-10-28');
  await page.getByLabel('Đến ngày').fill('2026-10-28');
  await page.getByRole('button', { name: 'Lọc' }).click();
  await page.getByText('Đã lưu ghi chú · Nhật ký').waitFor();
  page.once('dialog', (dialog) => dialog.accept());
  await page.getByRole('button', { name: 'Xóa khoảng ngày' }).click();
  await page.getByText('Chưa có hoạt động phù hợp.').waitFor();
  assert.equal(await page.getByText('Đã lưu ghi chú · Nhật ký').count(), 0);
  console.log('PASS: activity filters and date-range deletion (mock IPC).');
} finally {
  await browser.close();
}
