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
    const note = {
      id: 'n1',
      rev: 1,
      title: 'Kế hoạch tuần',
      body: 'Mai họp lúc 9h, nhớ đọc sách mỗi ngày.',
      tags: [],
      pinned: false,
      updated_at: 1,
      deleted_at: null,
      device_id: 'd',
    };
    window.__writes = [];
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
        if (command === 'notes_list') return [note];
        if (command === 'ai_summarize')
          return '- Họp lúc 9h\n- Đọc sách hằng ngày';
        if (command === 'ai_suggest_actions') {
          window.__nowLocal = args.nowLocal;
          return [
            {
              kind: 'event',
              title: 'Họp',
              body: null,
              when: '2026-11-02T09:00',
              frequency: null,
            },
            {
              kind: 'habit',
              title: 'Đọc sách',
              body: null,
              when: null,
              frequency: 'daily',
            },
          ];
        }
        if (['events_save', 'habits_save', 'notes_save'].includes(command)) {
          window.__writes.push([command, args.input]);
          return { ...args.input, id: 'x', rev: 1 };
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=main#/notes');
  await page.getByRole('button', { name: 'Kế hoạch tuần' }).click();

  await page.getByRole('button', { name: 'Gợi ý hành động' }).click();
  await page.getByRole('list', { name: 'Gợi ý hành động' }).waitFor();
  assert.match(
    await page.evaluate(() => window.__nowLocal),
    /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/,
  );
  // Suggestions alone write nothing.
  assert.equal(await page.evaluate(() => window.__writes.length), 0);

  await page.getByRole('button', { name: 'Thêm Họp' }).click();
  await page.getByText('Đã thêm: Họp').waitFor();
  const [command, input] = await page.evaluate(() => window.__writes[0]);
  assert.equal(command, 'events_save');
  assert.equal(input.kind, 'event');
  assert.equal(input.starts_at, new Date('2026-11-02T09:00').getTime());

  await page.getByRole('button', { name: 'Bỏ qua Đọc sách' }).click();
  assert.equal(await page.evaluate(() => window.__writes.length), 1);

  await page.getByRole('button', { name: 'Tóm tắt bằng AI' }).click();
  await page.getByText('Họp lúc 9h', { exact: false }).first().waitFor();
  await page.getByRole('button', { name: 'Chèn vào đầu ghi chú' }).click();
  const body = await page.getByLabel('Nội dung Markdown').inputValue();
  assert.ok(body.startsWith('> **Tóm tắt**\n> - Họp lúc 9h'));
  assert.ok(body.endsWith('Mai họp lúc 9h, nhớ đọc sách mỗi ngày.'));
  // Inserting only fills the editor; the note is saved when the user presses Lưu.
  assert.equal(await page.evaluate(() => window.__writes.length), 1);
  assert.deepEqual(errors, []);
  console.log(
    'PASS: AI summary insert and action suggestions require explicit confirmation before any write (mock IPC).',
  );
} finally {
  await browser.close();
}
