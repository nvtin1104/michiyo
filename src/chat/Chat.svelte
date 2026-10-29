<script lang="ts">
  import { locale, t } from '../lib/i18n';
  import { onMount, tick, onDestroy } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen, emit } from '@tauri-apps/api/event';
  import { newId } from '@michiyo/shared/ids';
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { call, desktop, type Config } from '../lib/api';
  import Companion from '../mascot/Companion.svelte';
  import { MascotMachine, type Pose } from '../mascot/machine';

  export let config: Config;
  export let compact = false;
  export let onNavigate: ((route: string) => void) | undefined = undefined;

  let composer: HTMLTextAreaElement;
  const focus = () => tick().then(() => composer?.focus());

  type Message = {
    role: 'user' | 'assistant';
    content: string;
    time?: string;
  };

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
  let historyReady = false;
  $: needsKey = /Chưa có API key cho claude/i.test(error);
  let job = '';
  let scroll: HTMLDivElement;
  let placement = 'left';

  // Mascot machine & companion state above input
  const mascotMachine = new MascotMachine();
  let mascotPose: Pose = 'idle';
  let companionMood = 'Michiyo đang lắng nghe bạn…';

  function syncMascot() {
    mascotPose = mascotMachine.pose;
  }

  function reactMascot(kind: Parameters<MascotMachine['react']>[0]) {
    mascotMachine.react(kind);
    syncMascot();
  }

  function handleMascotClick() {
    reactMascot('touched');
    companionMood = 'Hehe, xin chào bạn! 🌸';
    setTimeout(() => {
      if (!busy) companionMood = 'Michiyo đang lắng nghe bạn…';
    }, 2500);
  }

  function formatTime(d = new Date()) {
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  async function takeShellDraft() {
    if (compact || busy || !desktop) return;
    try {
      const draft = await call<{
        current: string;
        messages: Message[];
        input: string;
      } | null>('take_shell_chat_draft');
      if (draft) {
        if (draft.messages?.length) messages = draft.messages;
        if (draft.input) input = draft.input;
        focus();
      }
    } catch (e) {
      error = String(e);
    }
  }

  function resizeComposer() {
    if (!composer) return;
    if (compact) {
      composer.style.height = '0px';
      composer.style.height = `${Math.min(72, Math.max(28, composer.scrollHeight))}px`;
    } else {
      composer.style.height = 'auto';
      composer.style.height = `${Math.min(160, Math.max(54, composer.scrollHeight))}px`;
    }
  }

  // Single continuous thread persistence
  const MAIN_CONVERSATION_ID = 'main_continuous_chat';

  async function persist() {
    if (!desktop) {
      try {
        localStorage.setItem('michiyo_messages', JSON.stringify(messages));
      } catch {}
      return;
    }
    const c = {
      id: MAIN_CONVERSATION_ID,
      title: 'Hội thoại cùng Michiyo',
      messages: messages.map((m) => ({ role: m.role, content: m.content })),
    };
    try {
      await call('history_save', {
        conversation: c,
        limit: config.historyLimit || 200,
      });
    } catch (e) {
      error = String(e);
    }
  }

  let mascotTimer: ReturnType<typeof setInterval>;

  onMount(() => {
    let off: undefined | (() => void);
    let offFollow: undefined | (() => void);
    let offFocus: undefined | (() => void);
    let offPlacement: undefined | (() => void);
    let offSecrets: undefined | (() => void);
    let observer: ResizeObserver | undefined;
    let resizeTimer: ReturnType<typeof setTimeout>;

    mascotTimer = setInterval(() => {
      mascotMachine.tick(Date.now(), config?.sleepMinutes || 5);
      syncMascot();
    }, 250);

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
      if (!desktop) {
        try {
          const saved = localStorage.getItem('michiyo_messages');
          if (saved) messages = JSON.parse(saved);
        } catch {}
        historyReady = true;
        return;
      }
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

      try {
        const conversations = await call<
          { id: string; title: string; messages: Message[] }[]
        >('history_list');
        const main =
          conversations.find((c) => c.id === MAIN_CONVERSATION_ID) ||
          conversations[0];
        if (main?.messages?.length) {
          messages = main.messages;
        }
        historyReady = true;
      } catch (e) {
        error = String(e);
      }

      off = await listen<{ id: string; text: string }>('chat-delta', (e) => {
        if (e.payload.id === job) {
          const follow =
            !scroll ||
            scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight < 50;
          if (messages.length && messages[messages.length - 1].role === 'assistant') {
            messages[messages.length - 1].content += e.payload.text;
            messages = [...messages];
          }
          mascotMachine.transition('talking');
          syncMascot();
          companionMood = 'Michiyo đang trả lời bạn…';
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
      if (!compact) {
        offFocus = await listen('focus-composer', () => {
          takeShellDraft();
          focus();
        });
        await takeShellDraft();
      }
    })();

    return () => {
      off?.();
      offFollow?.();
      offFocus?.();
      offPlacement?.();
      offSecrets?.();
      observer?.disconnect();
      clearInterval(mascotTimer);
    };
  });

  onDestroy(() => {
    clearInterval(mascotTimer);
  });

  async function send() {
    const text = input.trim();
    if (!text || busy || !historyReady) return;
    input = '';
    error = '';
    resizeComposer();
    const time = formatTime();
    messages = [
      ...messages,
      { role: 'user', content: text, time },
      { role: 'assistant', content: '', time },
    ];
    busy = true;
    job = newId();
    mascotMachine.transition('thinking');
    syncMascot();
    companionMood = 'Michiyo đang suy nghĩ…';
    tick().then(() => scroll?.scrollTo(0, scroll.scrollHeight));

    if (!desktop) {
      setTimeout(() => {
        const reply =
          'Đây là bản xem trước web preview. Trên ứng dụng Windows, Michiyo sẽ xử lý và trả lời trực tiếp qua Claude AI.';
        messages[messages.length - 1].content = reply;
        messages = [...messages];
        busy = false;
        mascotMachine.transition('happy');
        syncMascot();
        companionMood = 'Michiyo ở đây cùng bạn ✨';
        persist();
        setTimeout(() => {
          mascotMachine.transition('idle');
          syncMascot();
          companionMood = 'Michiyo đang lắng nghe bạn…';
        }, 2200);
      }, 500);
      return;
    }

    try {
      const historyPayload = messages.slice(0, -1).map((m) => ({
        role: m.role,
        content: m.content,
      }));
      const answer = await call<string | null>('chat', {
        messages: historyPayload,
        id: job,
      });
      if (answer) {
        messages[messages.length - 1].content = answer;
        messages = [...messages];
      }
      await persist();
      mascotMachine.transition('happy');
      syncMascot();
      companionMood = 'Michiyo ở đây cùng bạn ✨';
      setTimeout(() => {
        mascotMachine.transition('idle');
        syncMascot();
        companionMood = 'Michiyo đang lắng nghe bạn…';
      }, 2200);
    } catch (e) {
      error = String(e);
      input = text;
      messages = messages.slice(0, -2);
      resizeComposer();
      mascotMachine.transition('idle');
      syncMascot();
      companionMood = 'Có chút trục trặc, bạn thử lại nhé!';
    } finally {
      busy = false;
      focus();
    }
  }

  async function clearHistory() {
    if (!confirm('Bạn có muốn xóa toàn bộ lịch sử trò chuyện này không?'))
      return;
    messages = [];
    input = '';
    error = '';
    if (desktop) {
      try {
        await call('history_delete', { id: MAIN_CONVERSATION_ID });
      } catch (e) {
        error = String(e);
      }
    } else {
      localStorage.removeItem('michiyo_messages');
    }
    companionMood = 'Đã làm mới cuộc trò chuyện rồi bạn nhé! 🌿';
    mascotMachine.transition('happy');
    syncMascot();
    focus();
  }

  async function copyText(text: string) {
    if (!text) return;
    if (desktop) {
      await call('copy_text', { text }).catch(() => {});
    } else {
      await navigator.clipboard.writeText(text).catch(() => {});
    }
    companionMood = 'Đã sao chép nội dung vào khay nhớ tạm! ✨';
    setTimeout(() => {
      if (!busy) companionMood = 'Michiyo đang lắng nghe bạn…';
    }, 2000);
  }

  async function saveAnswer(text: string) {
    if (!desktop) return;
    try {
      await call('notes_save', {
        id: newId(),
        title: text.slice(0, 32).trim() || 'Ghi chú từ trò chuyện',
        body: text,
        tags: ['trò-chuyện'],
        pinned: false,
      });
      noteStatus = 'Đã lưu câu trả lời vào Ghi chú';
      setTimeout(() => (noteStatus = ''), 3000);
    } catch (e) {
      error = String(e);
    }
  }
</script>

{#if !compact}
  <h1 class="sr-only">{t('Trò chuyện', $locale)}</h1>
{/if}

<div
  class="messages chat-stream-container"
  class:compact-stream={compact}
  class:beside-left={compact && placement === 'left'}
  bind:this={scroll}
>
  {#if !messages.length && !compact}
    <div class="welcome-box">
      <div class="welcome-badge">🌸</div>
      <h2>Hôm nay bạn cần Michiyo hỗ trợ gì không?</h2>
      <p>Cứ tự nhiên chia sẻ, hỏi đáp hoặc tâm sự cùng Michiyo như một người bạn nhé.</p>
      <div class="suggestions">
        {#each ['Lên kế hoạch hôm nay giúp mình', 'Giúp tôi viết một email', 'Giải thích một ý tưởng phức tạp', 'Gợi ý các thói quen tốt'] as suggestion}
          <button
            type="button"
            onclick={() => {
              input = suggestion;
              focus();
            }}
          >
            {t(suggestion, $locale)} ↗
          </button>
        {/each}
      </div>
    </div>
  {/if}

  {#each visibleMessages as message}
    <div
      class="msg-row"
      class:user-row={message.role === 'user'}
      class:bot-row={message.role === 'assistant'}
    >
      {#if message.role === 'assistant'}
        <div class="avatar bot-avatar" title="Michiyo">
          <span>🌸</span>
        </div>
      {/if}

      <div class="bubble-wrap">
        {#if compact && message.role === 'assistant'}
          <div class="response-meta">
            <span>Michiyo</span>
            {#if busy}
              <span class="stream-label">
                <i aria-hidden="true"></i>{$locale === 'en' ? 'Replying' : 'Đang trả lời'}
              </span>
            {:else}
              <button
                type="button"
                aria-label="Sao chép câu trả lời"
                onclick={() => copyText(message.content)}
              >
                {t('Sao chép', $locale)}
              </button>
            {/if}
          </div>
        {/if}
        <article
          class="msg-bubble"
          class:user={message.role === 'user'}
          class:streaming={busy && message === messages.at(-1)}
        >
          {#if message.role === 'user'}
            <div class="user-text">{message.content}</div>
          {:else}
            <div class="markdown">
              {@html DOMPurify.sanitize(
                marked.parse(message.content, { async: false }) as string,
                { FORBID_TAGS: ['a', 'img', 'video', 'audio', 'iframe'] },
              )}
            </div>
          {/if}
        </article>

        <div class="msg-footer">
          {#if message.time}
            <span class="msg-time">{message.time}</span>
          {/if}
          {#if message.content && message.role === 'assistant' && !compact}
            <div class="msg-actions">
              <button
                type="button"
                title="Sao chép"
                onclick={() => copyText(message.content)}
              >
                {t('Sao chép', $locale)}
              </button>
              {#if desktop}
                <button
                  type="button"
                  disabled={busy}
                  title="Lưu vào ghi chú"
                  onclick={() => saveAnswer(message.content)}
                >
                  Lưu ghi chú
                </button>
              {/if}
            </div>
          {/if}
        </div>
      </div>

      {#if message.role === 'user'}
        <div class="avatar user-avatar" title="Bạn">
          <span>👤</span>
        </div>
      {/if}
    </div>
  {/each}

  {#if busy && !messages.at(-1)?.content}
    <div class="msg-row bot-row thinking-row">
      <div class="avatar bot-avatar">🌸</div>
      <div class="bubble-wrap">
        <div class="msg-bubble thinking-bubble">
          <span class="dot"></span>
          <span class="dot"></span>
          <span class="dot"></span>
          <span class="thinking-label">Michiyo đang suy nghĩ…</span>
        </div>
      </div>
    </div>
  {/if}
</div>

{#if noteStatus}
  <div class="toast-note" role="status">{noteStatus}</div>
{/if}

{#if error}
  <div
    class="chat-notice"
    class:notice-error={!needsKey}
    role={needsKey ? 'status' : 'alert'}
  >
    <span>
      {needsKey
        ? $locale === 'en'
          ? 'Add a Claude API key to chat.'
          : 'Thêm Claude API key để trò chuyện.'
        : error}
    </span>
    <div class="notice-actions">
      <button
        type="button"
        onclick={() => {
          if (!compact && onNavigate) onNavigate('settings');
          else
            call(
              compact ? 'open_panel' : 'open_main',
              compact ? { label: 'settings' } : { route: 'settings' },
            );
        }}
      >
        {needsKey
          ? $locale === 'en'
            ? 'Add key'
            : 'Thêm key'
          : t('Mở Cài đặt', $locale)}
      </button>
      <button
        type="button"
        class="dismiss-notice"
        aria-label="Đóng thông báo"
        onclick={() => (error = '')}>×</button
      >
    </div>
  </div>
{/if}

<!-- Companion Dock: Mascot directly above input -->
{#if !compact}
  <div class="companion-dock" role="region" aria-label="Bé bot Michiyo">
    <button
      type="button"
      class="companion-touch-zone"
      title="Nhấp để tương tác cùng Michiyo"
      onclick={handleMascotClick}
      onpointerenter={() => {
        mascotMachine.hover(true);
        syncMascot();
      }}
      onpointerleave={() => {
        mascotMachine.hover(false);
        syncMascot();
      }}
    >
      <div class="companion-frame">
        <Companion
          state={mascotPose}
          speed={config.animationSpeed}
          reduced={config.reducedMotion}
          skin={config.skin}
        />
      </div>
    </button>
    <div class="companion-dialog">
      <div class="companion-bubble">
        <span class="companion-speaker">Michiyo</span>
        <span class="companion-mood">{companionMood}</span>
      </div>
    </div>
  </div>
{/if}

<form
  class="chat-composer"
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
      : t('Nhập tin nhắn tâm sự hoặc câu hỏi cùng Michiyo…', $locale)}
    oninput={() => {
      resizeComposer();
      mascotMachine.transition('listening');
      syncMascot();
      companionMood = 'Michiyo đang lắng nghe…';
      if (desktop && !busy) emit('mascot-state', 'listening');
    }}
    onkeydown={(e) => {
      if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
        e.preventDefault();
        send();
      }
    }}
  ></textarea>

  <div class="composer-footer">
    {#if !compact}
      <div class="model-badge">
        <span class="badge-dot"></span>
        <small>{config.model} · Nhấn Enter để gửi</small>
      </div>
    {/if}

    <div class="composer-buttons">
      {#if busy}
        <button
          type="button"
          class="stop-btn"
          aria-label="Dừng trả lời"
          onclick={() => call('stop_chat', { id: job })}
        >
          {compact ? '■' : '■ Dừng'}
        </button>
      {:else}
        <button
          type="submit"
          class="primary send-btn"
          aria-label="Gửi tin nhắn"
          disabled={!input.trim() || !historyReady}
        >
          <span>{compact ? '↑' : 'Gửi'}</span>
          {#if !compact}
            <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M22 2 11 13M22 2l-7 20-4-9-9-4 20-7z" />
            </svg>
          {/if}
        </button>
      {/if}
      {#if compact}
        <button
          type="button"
          aria-label="Mở trong app"
          disabled={busy}
          onclick={() =>
            call('open_chat_in_main', {
              draft: { current: MAIN_CONVERSATION_ID, messages, input },
            }).catch((e) => (error = String(e)))}>↗</button
        >
        <button
          type="button"
          aria-label="Đóng trò chuyện"
          onclick={() => desktop && getCurrentWindow().hide()}>×</button
        >
      {/if}
    </div>
  </div>
</form>

<style>
  /* Stream & Messages */
  .chat-stream-container {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 6px 18px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    scrollbar-width: thin;
    scrollbar-color: #cdd9cc transparent;
  }

  /* Welcome view */
  .welcome-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    margin: auto 0;
    padding: 30px 16px;
  }
  .welcome-badge {
    font-size: 38px;
    margin-bottom: 12px;
    filter: drop-shadow(0 4px 10px rgba(0, 0, 0, 0.08));
  }
  .welcome-box h2 {
    font-size: 23px;
    font-weight: 650;
    color: #1f3325;
    margin: 0 0 8px;
    letter-spacing: -0.02em;
  }
  .welcome-box p {
    font-size: 14px;
    color: #637768;
    max-width: 460px;
    margin: 0 0 24px;
    line-height: 1.5;
  }
  .suggestions {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 8px;
    width: min(100%, 640px);
  }
  .suggestions button {
    padding: 12px 16px;
    text-align: left;
    background: #fff;
    border: 1px solid #dce5dc;
    border-radius: 12px;
    font-size: 13px;
    color: #2e4334;
    cursor: pointer;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.02);
    transition: all 0.15s ease;
  }
  .suggestions button:hover {
    border-color: #3b6b4c;
    background: #f7faf7;
    transform: translateY(-1px);
  }

  /* 1v1 Human-to-Human Message Rows */
  .msg-row {
    display: flex;
    gap: 10px;
    align-items: flex-end;
    max-width: 86%;
  }
  .msg-row.user-row {
    margin-left: auto;
    flex-direction: row;
    justify-content: flex-end;
  }
  .msg-row.bot-row {
    margin-right: auto;
    justify-content: flex-start;
  }
  .avatar {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    font-size: 16px;
    user-select: none;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.06);
  }
  .bot-avatar {
    background: #eff6ee;
    border: 1px solid #d2e3d2;
  }
  .user-avatar {
    background: #30543e;
    color: #fff;
    border: 1px solid #284734;
  }
  .bubble-wrap {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .user-row .bubble-wrap {
    align-items: flex-end;
  }
  .bot-row .bubble-wrap {
    align-items: flex-start;
  }

  /* Chat Bubbles */
  .msg-bubble {
    padding: 12px 17px;
    font-size: 14.5px;
    line-height: 1.55;
    word-break: break-word;
    border-radius: 18px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.03);
  }
  .user-row .msg-bubble {
    background: #2f533d;
    color: #f7faf7;
    border-radius: 18px 18px 4px 18px;
  }
  .user-row .user-text {
    white-space: pre-wrap;
  }
  .bot-row .msg-bubble {
    background: #ffffff;
    color: #24342a;
    border: 1px solid #e1e7e1;
    border-radius: 18px 18px 18px 4px;
  }
  .msg-footer {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 5px;
    padding: 0 4px;
  }
  .msg-time {
    font-size: 11px;
    color: #8fa092;
  }
  .msg-actions {
    display: flex;
    gap: 6px;
    opacity: 0.75;
    transition: opacity 0.15s ease;
  }
  .msg-row:hover .msg-actions {
    opacity: 1;
  }
  .msg-actions button {
    background: transparent;
    border: 0;
    padding: 2px 6px;
    font-size: 11px;
    color: #637766;
    cursor: pointer;
    border-radius: 4px;
  }
  .msg-actions button:hover {
    background: #e9eee8;
    color: #26382a;
  }

  /* Thinking Indicator */
  .thinking-bubble {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 12px 18px;
    background: #ffffff;
    border: 1px solid #e1e7e1;
    border-radius: 18px 18px 18px 4px;
  }
  .dot {
    width: 6px;
    height: 6px;
    background: #50785c;
    border-radius: 50%;
    animation: bounce 1.4s infinite ease-in-out both;
  }
  .dot:nth-child(1) { animation-delay: -0.32s; }
  .dot:nth-child(2) { animation-delay: -0.16s; }
  @keyframes bounce {
    0%, 80%, 100% { transform: scale(0); opacity: 0.4; }
    40% { transform: scale(1); opacity: 1; }
  }
  .thinking-label {
    font-size: 13px;
    color: #6b7e70;
    margin-left: 4px;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border-width: 0;
  }

  /* Companion Dock directly above input - unconstrained & natural */
  .companion-dock {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    padding: 0 4px 6px;
    user-select: none;
  }
  .companion-touch-zone {
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
    outline: none;
    display: flex;
    align-items: flex-end;
    transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .companion-touch-zone:hover {
    transform: translateY(-4px) scale(1.06);
  }
  .companion-frame {
    width: 88px;
    height: 88px;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    position: relative;
    /* Không dùng border tròn và không dùng overflow hidden để tránh bị cắt bot */
  }
  .companion-frame :global(canvas) {
    width: 88px !important;
    height: 88px !important;
    pointer-events: none;
    filter: drop-shadow(0 5px 12px rgba(45, 75, 55, 0.16));
  }
  .companion-dialog {
    flex: 1;
    min-width: 0;
    margin-bottom: 8px;
  }
  .companion-bubble {
    display: inline-flex;
    flex-direction: column;
    background: #ffffff;
    border: 1px solid #dce5dc;
    border-radius: 12px 14px 14px 4px;
    padding: 7px 14px;
    box-shadow: 0 3px 10px rgba(0, 0, 0, 0.03);
    max-width: 100%;
    position: relative;
  }
  .companion-bubble::before {
    content: '';
    position: absolute;
    left: -5px;
    bottom: 9px;
    width: 8px;
    height: 8px;
    background: #ffffff;
    border-left: 1px solid #dce5dc;
    border-bottom: 1px solid #dce5dc;
    transform: rotate(45deg);
  }
  .companion-speaker {
    font-size: 11px;
    font-weight: 700;
    color: #355841;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .companion-mood {
    font-size: 13px;
    color: #44594a;
    margin-top: 2px;
  }

  /* Chat Composer Form */
  .chat-composer {
    background: #ffffff;
    border: 1px solid #d5dfd5;
    border-radius: 16px;
    padding: 10px 14px 8px;
    box-shadow: 0 4px 16px rgba(41, 68, 50, 0.06);
    display: flex;
    flex-direction: column;
    gap: 8px;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .chat-composer:focus-within {
    border-color: #3e6d4e;
    box-shadow: 0 4px 20px rgba(41, 68, 50, 0.11);
  }
  .chat-composer textarea {
    width: 100%;
    min-height: 52px;
    max-height: 160px;
    resize: none;
    border: 0;
    outline: none;
    font-family: inherit;
    font-size: 14.5px;
    line-height: 1.5;
    color: #213326;
    background: transparent;
    padding: 0;
  }
  .compact-composer textarea {
    min-height: 28px !important;
    height: 28px;
    max-height: 72px;
    font-size: 13.5px;
  }
  .chat-composer textarea::placeholder {
    color: #8d9e90;
  }
  .composer-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: 4px;
    border-top: 1px solid #edf2ec;
  }
  .model-badge {
    display: flex;
    align-items: center;
    gap: 6px;
    color: #819484;
    font-size: 11.5px;
  }
  .badge-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #479e61;
  }
  .composer-buttons {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }
  .send-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 18px;
    border-radius: 10px;
    background: #2d503b;
    color: #fff;
    font-size: 13.5px;
    font-weight: 600;
    border: 0;
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .send-btn svg {
    width: 14px;
    height: 14px;
  }
  .send-btn:hover:not(:disabled) {
    background: #244130;
  }
  .send-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .stop-btn {
    padding: 7px 14px;
    border-radius: 10px;
    background: #d44848;
    color: #fff;
    border: 0;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  /* Compact mode overrides */
  .compact-composer {
    padding: 8px 10px;
  }
  .compact-stream {
    padding: 6px;
  }

  /* Toast & Notices */
  .toast-note {
    background: #2f543e;
    color: #fff;
    padding: 8px 14px;
    border-radius: 8px;
    font-size: 12.5px;
    margin-top: 8px;
    align-self: center;
  }
  .chat-notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-radius: 10px;
    font-size: 13px;
    background: #fff6e6;
    border: 1px solid #f2dab3;
    color: #7a5416;
    margin-bottom: 8px;
  }
  .chat-notice.notice-error {
    background: #fdf0f0;
    border-color: #f3cccc;
    color: #9c2727;
  }
  .notice-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .notice-actions button {
    padding: 4px 10px;
    border-radius: 6px;
    font-size: 12px;
    border: 1px solid currentColor;
    background: transparent;
    cursor: pointer;
  }
  .dismiss-notice {
    font-size: 16px !important;
    border: 0 !important;
    padding: 0 4px !important;
  }
</style>
