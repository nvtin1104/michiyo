<script lang="ts">
  import { locale, t } from '../lib/i18n';
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen, emit } from '@tauri-apps/api/event';
  import { newId } from '@michiyo/shared/ids';

  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { call, desktop, type Config } from '../lib/api';
  export let config: Config;
  export let compact = false;
  export let onNavigate: ((route: string) => void) | undefined = undefined;
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
  let noteStatus = '';
  let historyReady = !desktop;
  $: needsKey = /Chưa có API key cho claude/i.test(error);
  let job = '';
  let scroll: HTMLDivElement;
  let placement = 'left';
  async function takeShellDraft() {
    if (compact || busy || !desktop) return;
    try {
      const draft = await call<{
        current: string;
        messages: Message[];
        input: string;
      } | null>('take_shell_chat_draft');
      if (draft) {
        current = draft.current;
        messages = draft.messages;
        input = draft.input;
        focus();
      }
    } catch (e) {
      error = String(e);
    }
  }
  function resizeComposer() {
    if (!composer || !compact) return;
    composer.style.height = '0px';
    composer.style.height = `${Math.min(72, Math.max(28, composer.scrollHeight))}px`;
  }
  async function persist() {
    if (!desktop) return;
    let c = conversations.find((c) => c.id === current);
    if (!c) {
      c = {
        id: current || newId(),
        title: messages[0]?.content.slice(0, 35) || 'Cuộc trò chuyện mới',
        messages: [],
      };
      current = c.id;
      conversations.unshift(c);
    }
    c.messages = messages.map((m) => ({ ...m }));
    try {
      conversations = await call<Conversation[]>('history_save', {
        conversation: c,
        limit: config.historyLimit,
      });
    } catch (e) {
      error = String(e);
    }
  }
  onMount(() => {
    let off: undefined | (() => void);
    let offFollow: undefined | (() => void);
    let offFocus: undefined | (() => void);
    let offPlacement: undefined | (() => void);
    let offSecrets: undefined | (() => void);
    let offHistory: undefined | (() => void);
    let offNavigate: undefined | (() => void);
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
      let historyLoaded = false;
      try {
        conversations = await call<Conversation[]>('history_list');
        historyLoaded = true;
      } catch (e) {
        error = String(e);
      }
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
      if (compact) {
        offFollow = await listen('ask-translation', take);
        await take();
      }
      offHistory = await listen('history-changed', async () => {
        if (busy || !historyReady) return;
        try {
          const selectedId = current;
          const latest = await call<Conversation[]>('history_list');
          if (busy || current !== selectedId) return;
          conversations = latest;
          if (current) {
            const selected = latest.find(
              (conversation) => conversation.id === current,
            );
            if (selected) messages = selected.messages;
            else {
              current = '';
              messages = [];
            }
          }
        } catch (e) {
          error = String(e);
        }
      });
      if (!compact) {
        offNavigate = await listen<string>('shell-navigate', (event) => {
          if (event.payload === 'chat') takeShellDraft();
        });
        await takeShellDraft();
      }
      offFocus = await listen('chat-focus', focus);
      historyReady = historyLoaded;
      focus();
    })();
    return () => {
      off?.();
      offFollow?.();
      offFocus?.();
      offPlacement?.();
      offSecrets?.();
      offHistory?.();
      offNavigate?.();
      observer?.disconnect();
      clearTimeout(resizeTimer);
    };
  });
  async function send() {
    if (!input.trim() || busy || !historyReady) return;
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
      if (messages !== previous) await persist();
      busy = false;
      if (!compact) await takeShellDraft();
    }
  }
  async function saveAnswer(content: string) {
    noteStatus = '';
    try {
      await call('notes_save', {
        input: {
          id: null,
          rev: null,
          title: content.trim().slice(0, 60) || 'Trả lời của Michiyo',
          body: content,
          tags: [],
          pinned: false,
        },
      });
      noteStatus = 'Đã lưu câu trả lời thành ghi chú.';
    } catch (e) {
      error = String(e);
    }
  }
  async function clear() {
    if (desktop && current) {
      try {
        conversations = await call<Conversation[]>('history_delete', {
          id: current,
        });
      } catch (e) {
        error = String(e);
        return;
      }
    } else {
      conversations = conversations.filter((c) => c.id !== current);
    }
    messages = [];
    current = '';
  }
</script>

{#if !compact}<header class="chat-header">
    <div>
      <h1>{t('Trò chuyện', $locale)}</h1>
    </div>
    <button
      type="button"
      disabled={busy}
      onclick={() =>
        { current = ''; messages = []; input = ''; error = ''; focus(); }}
      >+ Cuộc trò chuyện mới</button
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
        <h2>Hôm nay tôi có thể giúp gì?</h2>
        <p>Hỏi Michiyo về bất cứ điều gì bạn đang làm.</p>
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
        {#if message.content && !compact && message.role === 'assistant'}<button
            type="button"
            disabled={busy}
            onclick={() => saveAnswer(message.content)}
            >Lưu thành ghi chú</button
          >{/if}
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
{#if noteStatus}<p role="status">{noteStatus}</p>{/if}
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
        onclick={() => {
          if (!compact && onNavigate) onNavigate('settings');
          else call(compact ? 'open_panel' : 'open_main', compact ? { label: 'settings' } : { route: 'settings' });
        }}
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
        disabled={!input.trim() || !historyReady}
        >{compact ? '↑' : 'Gửi ↗'}</button
      >{/if}
    {#if compact}<button
        type="button"
        aria-label="Mở trong app"
        disabled={busy}
        onclick={() =>
          call('open_chat_in_main', {
            draft: { current, messages, input },
          }).catch((e) => (error = String(e)))}>↗</button
      ><button
        type="button"
        aria-label="Đóng trò chuyện"
        onclick={() => desktop && getCurrentWindow().hide()}>×</button
      >{/if}
  </div>
</form>
