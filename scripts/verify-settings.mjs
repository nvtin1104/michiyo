import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 720, height: 540 },
    deviceScaleFactor: 1.25,
  });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.addInitScript(() => {
    let id = 0;
    const data = new Map();
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
        if (command === 'plugin:store|set') data.set(args.key, args.value);
        if (command === 'plugin:event|listen') return ++id;
        if (command === 'secret_status') return {};
        if (command === 'list_skins')
          return { names: ['default'], directory: 'C:/example/skins' };
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=settings');
  await page.getByRole('heading', { name: 'Cài đặt', exact: true }).waitFor();
  await page.locator('nav button').filter({ hasText: 'Chat' }).click();
  const model = page.getByRole('combobox', { name: 'Model', exact: true });
  await model.press('ArrowDown');
  await page.getByRole('listbox').waitFor();
  await model.press('ArrowDown');
  await model.press('Enter');
  assert.match(await model.textContent(), /sonnet/);
  await page.waitForTimeout(650);
  assert.equal(
    await model.evaluate((el) => el === document.activeElement),
    true,
  );
  await model.press('Enter');
  await model.press('Escape');
  assert.equal(await page.getByRole('listbox').count(), 0);
  assert.equal(
    await model.evaluate((el) => el === document.activeElement),
    true,
  );
  assert.equal(
    await page
      .locator('.settings-content')
      .evaluate((el) => el.scrollHeight > el.clientHeight),
    false,
  );
  const prompt = page.getByLabel('System prompt');
  await prompt.click();
  assert.equal(
    await prompt.evaluate((el) => getComputedStyle(el).outlineStyle),
    'none',
  );
  await mkdir('artifacts', { recursive: true });
  await page.screenshot({ path: 'artifacts/settings-compact.png' });
  await model.click();
  await page.screenshot({ path: 'artifacts/settings-options.png' });
  await page.locator('.settings-header').click();
  assert.equal(await page.getByRole('listbox').count(), 0);
  for (const name of ['Chung', 'API', 'Dịch', 'Phím tắt', 'Giao diện']) {
    await page.locator('nav button').filter({ hasText: name }).click();
    assert.equal(
      await page
        .locator('footer')
        .evaluate((el) => el.getBoundingClientRect().bottom <= innerHeight),
      true,
    );
    assert.equal(
      await page.evaluate(
        () => document.documentElement.scrollWidth > innerWidth,
      ),
      false,
    );
  }
  await page.locator('nav button').filter({ hasText: 'Dịch' }).click();
  const source = page.getByRole('combobox', { name: 'Nguồn', exact: true });
  await source.click();
  await source.press('End');
  await source.press('Enter');
  assert.match(await source.textContent(), /Deutsch/);
  assert.deepEqual(errors, []);
  console.log(
    'PASS: compact Chat fits 720×540; custom select keyboard/outside/Escape; focus survives save; single focus border; all tabs keep footer visible without horizontal overflow (mock IPC).',
  );
} finally {
  await browser.close();
}
