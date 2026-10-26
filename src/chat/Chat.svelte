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
  let input = '';
  let busy = false;
  let error = '';
  let job = '';
  let scroll: HTMLDivElement;
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
    focus();
    (async () => {
      if (!desktop) return;
      const store = await load('history.json');
      conversations = (await store.get<Conversation[]>('conversations')) || [];
      if (conversations[0]) {
        current = conversations[0].id;
        messages = conversations[0].messages;
      }
      off = await listen<{ id: string; text: string }>('chat-delta', (e) => {
        if (e.payload.id === job) {
          messages[messages.length - 1].content += e.payload.text;
          messages = [...messages];
          queueMicrotask(() => scroll?.scrollTo(0, scroll.scrollHeight));
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
    };
  });
  async function send() {
    if (!input.trim() || busy) return;
    error = '';
    messages = [...messages, { role: 'user', content: input }];
    input = '';
    const request = messages.filter((m) => m.content).map((m) => ({ ...m }));
    messages = [...messages, { role: 'assistant', content: '' }];
    job = crypto.randomUUID();
    busy = true;
    try {
      await call('chat', { id: job, messages: request });
    } catch (e) {
      error = String(e);
    } finally {
      if (!messages.at(-1)?.content) messages = messages.slice(0, -1);
      busy = false;
      await persist();
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

{#if compact}<div class="bubble-heading">
    <span>✦ Michiyo</span><button
      aria-label="Đóng trò chuyện"
      onclick={() => desktop && getCurrentWindow().hide()}>×</button
    >
  </div>{:else}<header>
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
<div class="messages" bind:this={scroll}>
  {#if !messages.length && compact}<p class="bubble-greeting">
      Mình ở đây. Bạn muốn hỏi gì?
    </p>{:else if !messages.length}<div class="welcome">
      <div class="star">✧</div>
      <h2>
        {t('Một chút giúp đỡ,', $locale)}<br />{t('một ngày nhẹ hơn.', $locale)}
      </h2>
      <p>{t('Mình là Michiyo. Bạn đang nghĩ gì?', $locale)}</p>
      <div class="suggestions">
        {#each ['Giúp tôi viết một email', 'Giải thích một ý tưởng', 'Lên kế hoạch hôm nay'] as suggestion}<button
            onclick={() => (input = suggestion)}
            >{t(suggestion, $locale)} ↗</button
          >{/each}
      </div>
    </div>{/if}{#each messages as message}<article
      class:user={message.role === 'user'}
    >
      <small>{message.role === 'user' ? 'BẠN' : 'MICHIYO'}</small>
      <div class="markdown">
        {@html DOMPurify.sanitize(
          marked.parse(message.content, { async: false }) as string,
          { FORBID_TAGS: ['a', 'img', 'video', 'audio', 'iframe'] },
        )}
      </div>
      {#if message.content}<button
          class="copy"
          onclick={() => call('copy_text', { text: message.content })}
          >{t('Sao chép', $locale)}</button
        >{/if}
    </article>{/each}{#if busy}<p class="status">
      ✦ Michiyo đang suy nghĩ…
    </p>{/if}
</div>
{#if error}<p class="error" role="alert">
    {error}<button onclick={() => call('open_panel', { label: 'settings' })}
      >{t('Mở Cài đặt', $locale)}</button
    >
  </p>{/if}
<form
  onsubmit={(e) => {
    e.preventDefault();
    send();
  }}
>
  <textarea
    bind:this={composer}
    aria-label={t('Tin nhắn', $locale)}
    bind:value={input}
    placeholder={compact
      ? 'Hỏi Michiyo…'
      : t('Viết điều bạn muốn chia sẻ…', $locale)}
    oninput={() => desktop && emit('mascot-state', 'listening')}
    onkeydown={(e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        send();
      }
    }}></textarea>
  <div class="composer-footer">
    <small>{config.model} · Enter để gửi</small>{#if busy}<button
        type="button"
        onclick={() => call('stop_chat', { id: job })}>■ Dừng</button
      >{:else}<button class="primary" disabled={!input.trim()}>Gửi ↗</button
      >{/if}
  </div>
</form>
