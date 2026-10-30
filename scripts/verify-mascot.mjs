import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
const browser = await chromium.launch({ channel: 'chrome', headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 160, height: 160 },
    deviceScaleFactor: 1.25,
  });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  // IPC fixture verifies frontend interactions without using real keys or paid APIs.
  await page.addInitScript(() => {
    let counter = 0;
    const callbacks = new Map(),
      events = new Map();
    window.calls = [];
    window.emitMockEvent = (event, payload) =>
      callbacks.get(events.get(event))?.({ payload });
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'mascot' },
        currentWebview: { label: 'mascot' },
      },
      transformCallback(fn) {
        const id = ++counter;
        callbacks.set(id, fn);
        return id;
      },
      unregisterCallback(id) {
        callbacks.delete(id);
      },
      async invoke(command, args = {}) {
        if (command !== 'set_hit_mask') window.calls.push({ command, args });
        if (command === 'take_shell_chat_draft') {
          if (window.location.search.includes('chat-draft'))
            return {
              current: 'test-id',
              messages: [{ role: 'assistant', content: 'Tin nhắn trước' }],
              input: 'Văn bản đang soạn',
            };
          return null;
        }
        if (command === 'take_translation_draft') {
          if (window.location.search.includes('translation-draft'))
            return {
              text: 'Hello',
              result: {
                text: 'Xin chào',
                engine: 'test',
                source: 'en',
                target: 'vi',
              },
            };
          return null;
        }
        if (command === 'history_list') {
          if (window.location.search.includes('fail-history'))
            throw 'Không đọc được lịch sử';
          return window.historyMock || [];
        }
        if (command === 'history_save') return [args.conversation];
        if (command === 'resize_chat') return window.testMaxHeight || 720;
        if (command === 'history_delete') return [];
        if (command === 'plugin:store|load') return 1;
        if (command === 'plugin:store|get') return [null, false];
        if (command === 'secret_status')
          return { claude: !!window.hasClaudeKey };
        if (command === 'plugin:event|listen') {
          events.set(args.event, args.handler);
          return args.handler;
        }
        if (command === 'chat') {
          if (window.chatError) throw window.chatError;
          const handler = events.get('chat-delta');
          if (window.pauseBeforeFirst)
            await new Promise((resolve) => (window.releaseFirst = resolve));
          if (window.streamChunks) {
            const [first, ...remaining] = window.streamChunks;
            callbacks.get(handler)?.({ payload: { id: args.id, text: first } });
            await new Promise((resolve) => (window.finishMockStream = resolve));
            for (const text of remaining)
              callbacks.get(handler)?.({ payload: { id: args.id, text } });
            return null;
          }
          callbacks.get(handler)?.({
            payload: {
              id: args.id,
              text:
                window.longReply ||
                (args.messages.length > 1
                  ? 'Câu trả lời mới nhất.'
                  : 'Câu trả lời kiểm thử ngay trong bong bóng.'),
            },
          });
        }
        return null;
      },
    };
  });
  await page.goto('http://127.0.0.1:1420/?view=mascot');
  await page.locator('canvas').waitFor();
  await page.keyboard.press('Tab');
  assert.equal(
    await page
      .locator('.mascot-window')
      .evaluate((el) => el === document.activeElement),
    true,
  );
  assert.equal(
    await page
      .locator('.mascot-window')
      .evaluate((el) => getComputedStyle(el).outlineStyle),
    'none',
  );
  assert.equal(await page.locator('.mascot-hint').count(), 0);
  await page.waitForTimeout(1400);
  await page.evaluate(() =>
    window.emitMockEvent('mascot-pointer', { x: -100, y: 80 }),
  );
  await page.waitForTimeout(80);
  const lookingLeft = await page
    .locator('.sprite canvas')
    .screenshot({ path: 'artifacts/mascot-look-left.png' });
  await page.evaluate(() =>
    window.emitMockEvent('mascot-pointer', { x: 260, y: 80 }),
  );
  await page.waitForTimeout(80);
  const lookingRight = await page
    .locator('.sprite canvas')
    .screenshot({ path: 'artifacts/mascot-look-right.png' });
  assert.notDeepEqual(
    lookingLeft,
    lookingRight,
    'Reze eyes should follow the pointer',
  );
  await page.mouse.click(80, 80, { button: 'right' });
  assert.equal(
    await page.evaluate(
      () => window.calls.filter((c) => c.command === 'mascot_menu').length,
    ),
    1,
  );
  assert.equal(
    await page.evaluate(
      () => window.calls.filter((c) => c.command === 'open_main').length,
    ),
    0,
  );
  await page.mouse.click(80, 80);
  assert.deepEqual(
    await page.evaluate(() =>
      window.calls
        .filter((c) => c.command === 'open_panel')
        .map((c) => c.args.label),
    ),
    ['chat'],
  );
  assert.equal(
    await page.evaluate(
      () =>
        document.documentElement.scrollWidth > innerWidth ||
        document.documentElement.scrollHeight > innerHeight,
    ),
    false,
  );
  await page.setViewportSize({ width: 360, height: 340 });
  await page.goto('http://127.0.0.1:1420/?view=chat');
  const input = page.getByRole('textbox', { name: 'Tin nhắn' });
  await input.waitFor();
  await page.waitForFunction(
    () => document.activeElement?.tagName === 'TEXTAREA',
  );
  await page.waitForFunction(
    () =>
      window.emitMockEvent &&
      window.calls.some((c) => c.command === 'history_list'),
  );
  await page.waitForTimeout(100);
  await page.evaluate(() => {
    document.activeElement.blur();
    window.emitMockEvent('chat-focus');
  });
  await page.waitForFunction(
    () => document.activeElement?.tagName === 'TEXTAREA',
  );
  assert.equal(await page.locator('.compact-stream').isVisible(), false);
  assert.equal(await page.locator('.compact-composer button').count(), 0);
  const composerStyle = await page.locator('.compact-composer').evaluate((el) => {
    const style = getComputedStyle(el);
    return { background: style.backgroundColor, top: style.borderTopWidth, bottom: style.borderBottomWidth, radius: style.borderRadius, shadow: style.boxShadow };
  });
  assert.deepEqual(composerStyle, { background: 'rgba(0, 0, 0, 0)', top: '0px', bottom: '1px', radius: '0px', shadow: 'none' });
  await page.addStyleTag({ content: 'html{background:#242424!important}' });
  await page.screenshot({ path: 'artifacts/chat-empty.png' });
  await input.fill('Bản nháp');
  await input.press('Control+Enter');
  assert.equal(
    await page.evaluate(
      () =>
        window.calls.find((c) => c.command === 'open_chat_in_main')?.args.draft
          .input,
    ),
    'Bản nháp',
  );
  await input.fill('Xin chào');
  await input.press('Enter');
  await page
    .getByText('Câu trả lời kiểm thử ngay trong bong bóng.', { exact: true })
    .waitFor();
  assert.equal(
    await page.getByRole('heading', { name: /Trò chuyện/ }).count(),
    0,
  );
  assert.equal(await page.locator('.messages article').count(), 1);
  assert.equal(await page.locator('.messages article.user').count(), 0);
  assert.ok((await input.boundingBox()).height <= 36);
  await input.fill('Câu hỏi tiếp');
  await input.press('Enter');
  await page.getByText('Câu trả lời mới nhất.', { exact: true }).waitFor();
  assert.equal(
    await page
      .getByText('Câu trả lời kiểm thử ngay trong bong bóng.', { exact: true })
      .count(),
    0,
  );
  assert.equal(await page.locator('.messages article').count(), 1);
  assert.equal(
    await page.evaluate(
      () =>
        window.calls.filter((c) => c.command === 'chat').at(-1).args.messages
          .length,
    ),
    3,
  );
  await page.waitForTimeout(150);
  assert.ok(
    await page.evaluate(() =>
      window.calls.some(
        (c) => c.command === 'resize_chat' && c.args.height < 180,
      ),
    ),
  );
  const height = await page
    .locator('.page')
    .evaluate((el) => Math.ceil(el.getBoundingClientRect().height) + 8);
  await page.setViewportSize({ width: 320, height });
  assert.equal(
    await page
      .locator('.bubble .page')
      .evaluate((el) => getComputedStyle(el).backgroundColor),
    'rgba(0, 0, 0, 0)',
  );
  assert.equal(
    await page
      .locator('.bubble .page')
      .evaluate((el) => getComputedStyle(el).borderTopWidth),
    '0px',
  );
  await page.addStyleTag({ content: 'html{background:#242424!important}' });
  await page.screenshot({ path: 'artifacts/chat-bubble.png' });
  await page.evaluate(
    () =>
      (window.streamChunks = [
        'Chào bạn',
        '! Mình có thể giúp gì cho bạn hôm nay?',
      ]),
  );
  await page.evaluate(() => (window.pauseBeforeFirst = true));
  await input.fill('Nói chuyện với mình');
  await input.press('Enter');
  await page.getByText('Michiyo đang suy nghĩ…', { exact: true }).waitFor();
  const thinkingStyle = await page
    .locator('.thinking-bubble')
    .evaluate((el) => {
      const style = getComputedStyle(el);
      return {
        background: style.backgroundColor,
        border: style.borderTopWidth,
        shadow: style.boxShadow,
      };
    });
  assert.deepEqual(thinkingStyle, {
    background: 'rgba(0, 0, 0, 0)',
    border: '0px',
    shadow: 'none',
  });
  await page.screenshot({ path: 'artifacts/chat-thinking.png' });
  await page.waitForFunction(() => typeof window.releaseFirst === 'function');
  await page.evaluate(() => {
    window.pauseBeforeFirst = false;
    window.releaseFirst();
  });
  await page.getByText('Chào bạn', { exact: true }).waitFor();
  await page.getByText('Đang trả lời', { exact: true }).waitFor();
  assert.equal(await page.locator('article.streaming').count(), 1);
  assert.equal(
    await page.getByRole('button', { name: 'Dừng trả lời' }).count(),
    0,
  );
  assert.equal(
    await page.getByText('✦ Michiyo đang suy nghĩ…', { exact: true }).count(),
    0,
  );
  const streamHeight = await page
    .locator('.page')
    .evaluate((el) => Math.ceil(el.getBoundingClientRect().height) + 4);
  await page.setViewportSize({ width: 320, height: streamHeight });
  await page.screenshot({ path: 'artifacts/chat-streaming.png' });
  await page.evaluate(() => window.finishMockStream());
  await page
    .getByText('Chào bạn! Mình có thể giúp gì cho bạn hôm nay?', {
      exact: true,
    })
    .waitFor();
  await page.locator('article.streaming').waitFor({ state: 'detached' });
  assert.equal(
    await page.getByRole('button', { name: 'Sao chép câu trả lời' }).count(),
    0,
  );
  await page.evaluate(() => (window.streamChunks = null));
  const savedBefore = await page.evaluate(
    () => window.calls.filter((c) => c.command === 'history_save').length,
  );
  await page.evaluate(
    () => (window.chatError = 'Chưa có API key cho claude. Hãy mở Cài đặt.'),
  );
  await input.fill('Giữ lại tin nhắn này');
  await input.press('Enter');
  await page
    .getByText('Thêm Claude API key để trò chuyện.', { exact: true })
    .waitFor();
  assert.equal(await input.inputValue(), 'Giữ lại tin nhắn này');
  assert.equal(await page.getByRole('alert').count(), 0);
  assert.equal(
    await page
      .locator('.chat-notice')
      .evaluate((el) => el.scrollHeight > el.clientHeight),
    false,
  );
  assert.equal(
    await page.evaluate(
      () => window.calls.filter((c) => c.command === 'history_save').length,
    ),
    savedBefore,
  );
  const noticeHeight = await page
    .locator('.page')
    .evaluate((el) => Math.ceil(el.getBoundingClientRect().height) + 4);
  await page.setViewportSize({ width: 320, height: noticeHeight });
  await page.screenshot({ path: 'artifacts/chat-key-notice.png' });
  await page.evaluate(() => {
    window.hasClaudeKey = true;
    window.emitMockEvent('secrets-changed');
  });
  await page.locator('.chat-notice').waitFor({ state: 'detached' });
  await page.setViewportSize({ width: 156, height: 190 });
  await page.goto('http://127.0.0.1:1420/?view=menu');
  const menu = page.getByRole('menu');
  await menu.waitFor();
  assert.equal(await page.getByRole('menuitem').count(), 6);
  await page.waitForFunction(
    () => document.activeElement?.getAttribute('role') === 'menuitem',
  );
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  assert.equal(
    await page.evaluate(() => document.activeElement?.textContent?.trim()),
    'Cài đặt',
  );
  await page.keyboard.press('Enter');
  assert.equal(
    await page.evaluate(
      () => window.calls.find((c) => c.command === 'menu_action')?.args.action,
    ),
    'settings',
  );
  assert.equal(
    await page.evaluate(
      () =>
        document.documentElement.scrollHeight > innerHeight ||
        document.documentElement.scrollWidth > innerWidth,
    ),
    false,
  );
  await page.screenshot({ path: 'artifacts/pet-context-menu.png' });
  await page.keyboard.press('Escape');
  assert.ok(
    await page.evaluate(() =>
      window.calls.some((c) => c.command === 'plugin:window|hide'),
    ),
  );
  await page.setViewportSize({ width: 1000, height: 720 });
  await page.goto('http://127.0.0.1:1420/?view=main#/chat');
  const shellInput = page.getByRole('textbox', { name: 'Tin nhắn' });
  await shellInput.waitFor();
  await page.evaluate(
    () => (window.streamChunks = ['Một ', 'câu trả lời hoàn chỉnh']),
  );
  await shellInput.fill('Kiểm tra chuyển trang');
  await shellInput.press('Enter');
  await page.getByText('Một ', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Không gian làm việc' }).click();
  await page.evaluate(() => window.finishMockStream());
  await page.getByRole('button', { name: 'Trò chuyện', exact: true }).click();
  await page.getByText('Một câu trả lời hoàn chỉnh', { exact: true }).waitFor();
  await page.goto('http://127.0.0.1:1420/?view=main&chat-draft=1#/chat');
  await page.getByText('Tin nhắn trước', { exact: true }).waitFor();
  assert.equal(
    await page.getByRole('textbox', { name: 'Tin nhắn' }).inputValue(),
    'Văn bản đang soạn',
  );
  await page.setViewportSize({ width: 360, height: 340 });
  await page.goto('http://127.0.0.1:1420/?view=chat&fail-history=1');
  await page.getByText('Không đọc được lịch sử', { exact: true }).waitFor();
  const blockedInput = page.getByRole('textbox', { name: 'Tin nhắn' });
  await blockedInput.fill('Không gửi khi chưa đọc được lịch sử');
  assert.equal(await page.locator('.compact-composer button').count(), 0);
  await blockedInput.press('Enter');
  assert.equal(
    await page.evaluate(() => window.calls.some((c) => c.command === 'chat')),
    false,
  );
  await page.goto('http://127.0.0.1:1420/?view=chat');
  const longInput = page.getByRole('textbox', { name: 'Tin nhắn' });
  await longInput.waitFor();
  await page.evaluate(() => {
    window.longReply = 'Đây là một câu trả lời dài. '.repeat(20);
  });
  await longInput.fill('Câu hỏi vừa');
  await longInput.press('Enter');
  await page
    .getByText('Đây là một câu trả lời dài.', { exact: false })
    .waitFor();
  await page.waitForTimeout(200);
  const medium = await page
    .locator('.compact-stream')
    .evaluate((el) => ({ height: el.clientHeight, full: el.scrollHeight }));
  assert.ok(
    medium.height > 220 && medium.full <= medium.height + 1,
    'ordinary replies expand without scrolling or clipping',
  );
  await page.evaluate(() => {
    window.testMaxHeight = 260;
    window.longReply = 'Đây là một câu trả lời dài. '.repeat(150);
  });
  await page.waitForTimeout(200);
  await longInput.fill('Câu hỏi dài');
  await longInput.press('Enter');
  await page
    .getByText('Ctrl+Enter để xem toàn bộ câu trả lời', { exact: true })
    .waitFor();
  assert.equal(
    await page
      .locator('.compact-stream')
      .evaluate((el) => getComputedStyle(el).overflowY),
    'hidden',
  );
  assert.equal(
    await page.locator('.compact-stream').evaluate((el) => el.scrollTop),
    0,
  );
  await longInput.press('Control+Enter');
  await page.waitForFunction(() =>
    window.calls.some((c) => c.command === 'open_chat_in_main'),
  );
  assert.equal(
    await page.evaluate(
      () =>
        window.calls
          .find((c) => c.command === 'open_chat_in_main')
          .args.draft.messages.at(-1).content,
    ),
    await page.evaluate(() => window.longReply),
  );
  assert.deepEqual(errors, []);
  console.log(
    'PASS: right click only opens menu; left click opens chat without a key; no mascot overflow; composer autofocus and streamed answer in compact bubble (mock IPC).',
  );
} finally {
  await browser.close();
}
