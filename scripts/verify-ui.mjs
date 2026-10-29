import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
const browser = await chromium.launch({
  executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe',
  headless: true,
});
const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
await page.goto('http://127.0.0.1:1420/');
await page.getByRole('heading', { name: 'Trò chuyện' }).waitFor();
await page.getByRole('button', { name: 'Giúp tôi viết một email' }).click();
assert.match(
  await page.getByRole('textbox', { name: 'Tin nhắn' }).inputValue(),
  /email/,
);
await page.getByRole('button', { name: 'Cài đặt', exact: true }).click();
await page.locator('nav button').filter({ hasText: 'API' }).click();
assert.equal(await page.locator('input[type=password]').count(), 3);
await page
  .locator('input[type=password]')
  .first()
  .fill('test-only-not-a-real-key');
await mkdir('artifacts', { recursive: true });
await page.screenshot({ path: 'artifacts/settings.png', fullPage: true });
await page.locator('nav button').filter({ hasText: 'Chung' }).click();
await page.getByRole('combobox', { name: 'Ngôn ngữ giao diện' }).click();
await page.getByRole('option', { name: 'English', exact: true }).click();
await page.getByRole('heading', { name: /^Settings/ }).waitFor();
assert.equal(
  await page.evaluate(() =>
    localStorage
      .getItem('preview-config')
      ?.includes('test-only-not-a-real-key'),
  ),
  false,
);
await page.reload();
await page.getByRole('button', { name: 'Settings', exact: true }).click();
await page.getByRole('heading', { name: /^Settings/ }).waitFor();
assert.match(
  await page
    .getByRole('combobox', { name: 'Interface language' })
    .textContent(),
  /English/,
);
await page.getByRole('combobox', { name: 'Interface language' }).click();
await page.getByRole('option', { name: 'Tiếng Việt', exact: true }).click();
await page.getByRole('heading', { name: /^Cài đặt/ }).waitFor();

// Navigate back to Chat
await page.getByRole('button', { name: 'Trò chuyện' }).click();
await page.getByRole('heading', { name: 'Trò chuyện' }).waitFor();

// Verify companion mascot canvas is visible above input
await page.locator('.companion-dock canvas').waitFor();

// Test workspace navigation
await page.getByRole('button', { name: 'Không gian làm việc' }).click();
await page.getByRole('heading', { name: /Một ngày cùng Michiyo/ }).waitFor();
await page
  .getByRole('navigation', { name: 'Điều hướng chính' })
  .getByRole('button', { name: 'Ghi chú' })
  .click();
await page.getByRole('heading', { name: 'Ghi chú' }).waitFor();

// Test quick back to chat from sidebar
await page.getByRole('button', { name: '← Trò chuyện' }).click();
const draft = page.getByRole('textbox', { name: 'Tin nhắn' });
await draft.fill('Bản nháp 1v1 gửi Michiyo');
assert.equal(await page.evaluate(() => location.hash), '#/chat');

// Go to workspace to test sidebar toggle
await page.getByRole('button', { name: 'Không gian làm việc' }).click();
await page.getByRole('button', { name: 'Đóng thanh bên' }).click();
assert.equal(
  await page
    .getByRole('navigation', { name: 'Điều hướng chính' })
    .isVisible(),
  false,
);
await page.getByRole('button', { name: 'Mở thanh bên' }).click();
await page.getByRole('button', { name: '← Trò chuyện' }).click();
assert.equal(await draft.inputValue(), 'Bản nháp 1v1 gửi Michiyo');

await page.goto('http://127.0.0.1:1420/?view=mascot');
await page.locator('canvas').waitFor();
await page.waitForTimeout(600);
await page.screenshot({ path: 'artifacts/mascot.png' });
assert.deepEqual(errors, []);
console.log(
  'UI checks passed: chat 1v1, mascot dock in chat, workspace navigation, settings tabs, and standalone mascot.',
);
await browser.close();
