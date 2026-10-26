<script lang="ts">
  import { locale, t } from '../lib/i18n';
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen, emit } from '@tauri-apps/api/event';
  import { load } from '@tauri-apps/plugin-store';

  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { call, desktop, type Config } from '../lib/api';
  export let config: Config;
  export let compact = false;
  let composer: HTMLTextAreaElement;
  const focus = () => tick().then(() => composer?.focus());
  type Message = { role: 'user' | 'assistant'; content: string };
  type Conversation = { id: string; title: string; messages: Message[] };
  let conversations: Conversation[] = [];
  let current = '';
  let messages: Message[] = [];
  $: visibleMessages = compact
    ? messages
        .filter((message) => message.role === 'assistant')
        .slice(-1)
        .filter((message) => message.content)
    : messages;
  let input = '';
  let busy = false;
  let error = '';
  $: needsKey = /Chưa có API key cho claude/i.test(error);
  let job = '';
  let scroll: HTMLDivElement;
  let placement = 'left';
  function resizeComposer() {
    if (!composer || !compact) return;
    composer.style.height = '0px';
    composer.style.height = `${Math.min(72, Math.max(28, composer.scrollHeight))}px`;
  }
  async function persist() {
    if (!desktop) return;
    const store = await load('history.json');
    let c = conversations.find((c) => c.id === current);
    if (!c) {
      c = {
        id: current || crypto.randomUUID(),
        title: messages[0]?.content.slice(0, 35) || 'Cuộc trò chuyện mới',
        messages: [],
      };
      current = c.id;
      conversations.unshift(c);
    }
    c.messages = messages.map((m) => ({ ...m }));
    conversations = conversations.slice(0, config.historyLimit);
    await store.set('conversations', conversations);
    await store.save();
  }
  onMount(() => {
    let off: undefined | (() => void);
    let offFollow: undefined | (() => void);
    let offFocus: undefined | (() => void);
    let offPlacement: undefined | (() => void);
    let offSecrets: undefined | (() => void);
    let observer: ResizeObserver | undefined;
    let resizeTimer: ReturnType<typeof setTimeout>;
    if (compact && desktop) {
      tick().then(() => {
        const page = composer?.closest('.page');
        if (!page) return;
        observer = new ResizeObserver(() => {
          clearTimeout(resizeTimer);
          resizeTimer = setTimeout(
            () =>
              call('resize_chat', {
                height: Math.ceil(page.getBoundingClientRect().height) + 4,
              }).catch(() => {}),
            60,
          );
        });
        observer.observe(page);
      });
    }
    focus();
    (async () => {
      if (!desktop) return;
      offSecrets = await listen('secrets-changed', async () => {
        if (/Chưa có API key cho claude/i.test(error)) {
          const status = await call<{ claude: boolean }>('secret_status');
          if (status.claude) error = '';
        }
      });
      if (compact)
        offPlacement = await listen<string>(
          'chat-placement',
          (event) => (placement = event.payload),
        );
      const store = await load('history.json');
      conversations = (await store.get<Conversation[]>('conversations')) || [];
      if (conversations[0]) {
        current = conversations[0].id;
        messages = conversations[0].messages;
      }
      off = await listen<{ id: string; text: string }>('chat-delta', (e) => {
        if (e.payload.id === job) {
          const follow =
            !scroll ||
            scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight < 40;
          messages[messages.length - 1].content += e.payload.text;
          messages = [...messages];
          if (follow)
            tick().then(() => scroll?.scrollTo(0, scroll.scrollHeight));
        }
      });
      const take = async () => {
        const prompt = await call<string | null>('take_chat_prompt');
        if (prompt) input = prompt;
      };
      offFollow = await listen('ask-translation', take);
      await take();
      offFocus = await listen('chat-focus', focus);
      focus();
    })();
    return () => {
      off?.();
      offFollow?.();
      offFocus?.();
      offPlacement?.();
      offSecrets?.();
      observer?.disconnect();
      clearTimeout(resizeTimer);
    };
  });
  async function send() {
    if (!input.trim() || busy) return;
    error = '';
    const previous = messages;
    const submitted = input;
    messages = [...messages, { role: 'user', content: input }];
    input = '';
    tick().then(resizeComposer);
    const request = messages.filter((m) => m.content).map((m) => ({ ...m }));
    messages = [...messages, { role: 'assistant', content: '' }];
    job = crypto.randomUUID();
    busy = true;
    try {
      await call('chat', { id: job, messages: request });
    } catch (e) {
      error = String(e);
      if (!messages.at(-1)?.content) {
        messages = previous;
        if (!input) input = submitted;
        await tick();
        resizeComposer();
      }
    } finally {
      if (messages !== previous && !messages.at(-1)?.content)
        messages = messages.slice(0, -1);
      busy = false;
      if (messages !== previous) await persist();
    }
  }
  async function clear() {
    messages = [];
    conversations = conversations.filter((c) => c.id !== current);
    current = '';
    if (desktop) {
      const store = await load('history.json');
      await store.set('conversations', conversations);
      await store.save();
    }
  }
</script>

{#if !compact}<header>
    <div>
      <small>YOUR LITTLE COMPANION</small>
      <h1>{t('Trò chuyện', $locale)} <span>✦</span></h1>
    </div>
    <button
      class="icon"
      onclick={() => call('open_panel', { label: 'settings' })}>⚙</button
    >
  </header>
  <div class="toolbar">
    <select
      aria-label={t('Lịch sử', $locale)}
      disabled={busy}
      value={current}
      onchange={(e) => {
        current = e.currentTarget.value;
        messages = conversations.find((c) => c.id === current)?.messages || [];
      }}
      ><option value="">{t('Cuộc trò chuyện mới', $locale)}</option
      >{#each conversations as c}<option value={c.id}>{c.title}</option
        >{/each}</select
    ><button disabled={busy} onclick={clear}>{t('Xóa', $locale)}</button>
  </div>
{/if}
{#if !compact || visibleMessages.length || busy}<div
    class="messages"
    class:beside-left={compact && placement === 'left'}
    bind:this={scroll}
  >
    {#if !messages.length && !compact}<div class="welcome">
        <div class="star">✧</div>
        <h2>
          {t('Một chút giúp đỡ,', $locale)}<br />{t(
            'một ngày nhẹ hơn.',
            $locale,
          )}
        </h2>
        <p>{t('Mình là Michiyo. Bạn đang nghĩ gì?', $locale)}</p>
        <div class="suggestions">
          {#each ['Giúp tôi viết một email', 'Giải thích một ý tưởng', 'Lên kế hoạch hôm nay'] as suggestion}<button
              onclick={() => (input = suggestion)}
              >{t(suggestion, $locale)} ↗</button
            >{/each}
        </div>
      </div>{/if}{#each visibleMessages as message}<article
        class:user={message.role === 'user'}
        class:streaming={busy && message === messages.at(-1)}
      >
        {#if compact}<div class="response-meta">
            <span>Michiyo</span>{#if busy}<span class="stream-label"
                ><i></i>{$locale === 'en' ? 'Replying' : 'Đang trả lời'}</span
              >{:else}<button
                type="button"
                aria-label="Sao chép câu trả lời"
                onclick={() => call('copy_text', { text: message.content })}
                >{t('Sao chép', $locale)}</button
              >{/if}
          </div>{/if}
        {#if !compact}<small
            >{message.role === 'user' ? 'BẠN' : 'MICHIYO'}</small
          >{/if}
        <div class="markdown">
          {@html DOMPurify.sanitize(
            marked.parse(message.content, { async: false }) as string,
            { FORBID_TAGS: ['a', 'img', 'video', 'audio', 'iframe'] },
          )}
        </div>
        {#if message.content && !compact}<button
            class="copy"
            onclick={() => call('copy_text', { text: message.content })}
            >{t('Sao chép', $locale)}</button
          >{/if}
      </article>{/each}{#if busy && !messages.at(-1)?.content}<p
        class="status thinking-status"
        role="status"
      >
        <span class="typing-dots" aria-hidden="true"><i></i><i></i><i></i></span
        > Michiyo đang suy nghĩ…
      </p>{/if}
  </div>{/if}
{#if error}<div
    class="chat-notice"
    class:notice-error={!needsKey}
    role={needsKey ? 'status' : 'alert'}
  >
    <span
      >{needsKey
        ? $locale === 'en'
          ? 'Add a Claude API key to chat.'
          : 'Thêm Claude API key để trò chuyện.'
        : error}</span
    >
    <div class="notice-actions">
      <button
        type="button"
        onclick={() => call('open_panel', { label: 'settings' })}
        >{needsKey
          ? $locale === 'en'
            ? 'Add key'
            : 'Thêm key'
          : t('Mở Cài đặt', $locale)}</button
      ><button
        type="button"
        class="dismiss-notice"
        aria-label="Đóng thông báo"
        onclick={() => (error = '')}>×</button
      >
    </div>
  </div>{/if}
<form
  class:compact-composer={compact}
  onsubmit={(e) => {
    e.preventDefault();
    send();
  }}
>
  <textarea
    rows={compact ? 1 : 2}
    bind:this={composer}
    aria-label={t('Tin nhắn', $locale)}
    bind:value={input}
    placeholder={compact
      ? 'Hỏi Michiyo…'
      : t('Viết điều bạn muốn chia sẻ…', $locale)}
    oninput={() => {
      resizeComposer();
      if (desktop && !busy) emit('mascot-state', 'listening');
    }}
    onkeydown={(e) => {
      if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
        e.preventDefault();
        send();
      }
    }}></textarea>
  <div class="composer-footer">
    {#if !compact}<small>{config.model} · Enter để gửi</small
      >{/if}{#if busy}<button
        type="button"
        aria-label="Dừng trả lời"
        onclick={() => call('stop_chat', { id: job })}
        >{compact ? '■' : '■ Dừng'}</button
      >{:else}<button
        class="primary"
        aria-label="Gửi tin nhắn"
        disabled={!input.trim()}>{compact ? '↑' : 'Gửi ↗'}</button
      >{/if}
    {#if compact}<button
        type="button"
        aria-label="Đóng trò chuyện"
        onclick={() => desktop && getCurrentWindow().hide()}>×</button
      >{/if}
  </div>
</form>
