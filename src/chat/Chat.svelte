<script lang="ts">
  import { locale, t } from '../lib/i18n';
  import { onMount, tick, onDestroy } from 'svelte';
  import { listen, emit } from '@tauri-apps/api/event';
  import { newId } from '@michiyo/shared/ids';
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { call, desktop, type Config } from '../lib/api';
  import Companion from '../mascot/Companion.svelte';
  import { MascotMachine, type Pose } from '../mascot/machine';

  export let config: Config;
  export let compact = false;
  export let active = true;
  export let onNavigate: ((route: string) => void) | undefined = undefined;
  let manageTimers: () => void = () => {};
  $: (active, manageTimers());

  let composer: HTMLTextAreaElement;
  const focus = () => tick().then(() => composer?.focus());

  type Message = {
    role: 'user' | 'assistant';
    content: string;
    time?: string;
    animate?: boolean;
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
  let compactOverflow = false;
  function updateCompactOverflow() {
    compactOverflow = !!(
      compact &&
      scroll &&
      scroll.scrollHeight > scroll.clientHeight + 1
    );
  }
  $: if (compact && messages) tick().then(updateCompactOverflow);
  let placement = 'left';

  // Mascot machine & companion state above input
  const mascotMachine = new MascotMachine();
  let mascotPose: Pose = 'idle';
  let ambientPose: Pose = 'sit';
  let ambientStep = 0;
  let atRight = false;
  let dockPosition = 0;
  let dockFrame = 0;
  let entering = false;
  let entryTimer: ReturnType<typeof setTimeout>;
  function playEntry() {
    clearTimeout(entryTimer);
    entering = false;
    if (compact || reducedMotion) return;
    tick().then(() => {
      entering = true;
      entryTimer = setTimeout(() => (entering = false), 900);
    });
  }
  function stopDockMotion() {
    cancelAnimationFrame(dockFrame);
    mascotMachine.stopMotion();
    syncMascot();
  }
  let systemReduced = false;
  $: reducedMotion = config.reducedMotion || systemReduced;
  $: displayedPose =
    mascotPose === 'idle' && !reducedMotion ? ambientPose : mascotPose;

  function syncMascot() {
    mascotPose = mascotMachine.pose;
  }

  function reactMascot(kind: Parameters<MascotMachine['react']>[0]) {
    stopDockMotion();
    mascotMachine.react(kind);
    syncMascot();
  }

  function handleMascotClick() {
    reactMascot('touched');
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
      composer.style.height = '28px';
    } else {
      composer.style.height = 'auto';
      composer.style.height = `${Math.min(120, Math.max(28, composer.scrollHeight))}px`;
    }
  }

  async function openFullChat() {
    if (!desktop || busy) return;
    try {
      await call('open_chat_in_main', {
        draft: { current: MAIN_CONVERSATION_ID, messages, input },
      });
      await call('close_chat');
    } catch (e) {
      error = String(e);
    }
  }

  // Single continuous thread persistence
  const MAIN_CONVERSATION_ID = 'main_continuous_chat';

  async function persist() {
    if (!desktop) {
      try {
        localStorage.setItem(
          'michiyo_messages',
          JSON.stringify(
            messages.map(({ role, content, time }) => ({
              role,
              content,
              time,
            })),
          ),
        );
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
    let offHistory: undefined | (() => void);
    let observer: ResizeObserver | undefined;
    let resizeTimer: ReturnType<typeof setTimeout>;
    let ambientTimer: ReturnType<typeof setInterval> | undefined;
    const motionQuery = matchMedia('(prefers-reduced-motion: reduce)');
    const motionChanged = () => {
      systemReduced = motionQuery.matches;
    };
    motionChanged();
    motionQuery.addEventListener('change', motionChanged);

    const updateTimers = () => {
      clearInterval(mascotTimer);
      clearInterval(ambientTimer);
      if (document.hidden || !active) {
        stopDockMotion();
        return;
      }
      mascotTimer = setInterval(() => {
        mascotMachine.tick(Date.now(), config?.sleepMinutes || 5);
        syncMascot();
      }, 250);
      if (!compact)
        ambientTimer = setInterval(() => {
          ambientStep = (ambientStep + 1) % 3;
          ambientPose = (['idle', 'sit', 'wave'] as const)[ambientStep];
        }, 6000);
    };
    manageTimers = updateTimers;
    updateTimers();
    document.addEventListener('visibilitychange', updateTimers);

    if (compact && desktop) {
      tick().then(() => {
        const page = composer?.closest<HTMLElement>('.page');
        if (!page) return;
        observer = new ResizeObserver(() => {
          updateCompactOverflow();
          clearTimeout(resizeTimer);
          resizeTimer = setTimeout(async () => {
            try {
              const maxHeight = await call<number>('resize_chat', {
                height: Math.ceil(page.getBoundingClientRect().height) + 4,
              });
              if (Number.isFinite(maxHeight) && maxHeight >= 40) {
                page.style.setProperty(
                  '--bubble-max-height',
                  `${maxHeight - 4}px`,
                );
                await tick();
                updateCompactOverflow();
              }
            } catch {
              /* The window may have been closed. */
            }
          }, 60);
        });
        observer.observe(page);
        if (scroll) observer.observe(scroll);
      });
    }
    focus();
    if (!compact) playEntry();

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
        const conversations =
          await call<{ id: string; title: string; messages: Message[] }[]>(
            'history_list',
          );
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

      offHistory = await listen('history-changed', async () => {
        if (busy) return;
        const before = messages;
        try {
          const conversations =
            await call<{ id: string; messages: Message[] }[]>('history_list');
          if (!busy && messages === before)
            messages =
              conversations.find((c) => c.id === MAIN_CONVERSATION_ID)
                ?.messages || [];
        } catch (e) {
          error = String(e);
        }
      });

      off = await listen<{ id: string; text: string }>('chat-delta', (e) => {
        if (e.payload.id === job) {
          const follow =
            !scroll ||
            scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight < 50;
          if (
            messages.length &&
            messages[messages.length - 1].role === 'assistant'
          ) {
            messages[messages.length - 1].content += e.payload.text;
            messages = [...messages];
          }
          mascotMachine.transition('talking');
          syncMascot();
          if (!compact && follow)
            tick().then(() => scroll?.scrollTo(0, scroll.scrollHeight));
        }
      });

      const take = async () => {
        const prompt = await call<string | null>('take_chat_prompt');
        if (prompt) input = prompt;
      };
      if (compact) {
        offFollow = await listen('ask-translation', take);
        offFocus = await listen('chat-focus', focus);
        await take();
      }
      if (!compact) {
        offFocus = await listen('focus-composer', () => {
          takeShellDraft();
          focus();
          playEntry();
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
      offHistory?.();
      observer?.disconnect();
      clearInterval(mascotTimer);
      clearInterval(ambientTimer);
      cancelAnimationFrame(dockFrame);
      clearTimeout(resizeTimer);
      clearTimeout(entryTimer);
      document.removeEventListener('visibilitychange', updateTimers);
      motionQuery.removeEventListener('change', motionChanged);
      manageTimers = () => {};
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
      { role: 'user', content: text, time, animate: true },
      { role: 'assistant', content: '', time, animate: true },
    ];
    busy = true;
    job = newId();
    mascotMachine.transition('thinking');
    syncMascot();
    if (!compact) tick().then(() => scroll?.scrollTo(0, scroll.scrollHeight));

    if (!desktop) {
      setTimeout(() => {
        const reply =
          'Đây là bản xem trước web preview. Trên ứng dụng Windows, Michiyo sẽ xử lý và trả lời trực tiếp qua Claude AI.';
        messages[messages.length - 1].content = reply;
        messages = [...messages];
        busy = false;
        mascotMachine.transition('happy');
        syncMascot();
        persist();
        setTimeout(() => {
          mascotMachine.transition('idle');
          syncMascot();
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
      setTimeout(() => {
        mascotMachine.transition('idle');
        syncMascot();
      }, 2200);
    } catch (e) {
      error = String(e);
      input = text;
      messages = messages.slice(0, -2);
      resizeComposer();
      mascotMachine.transition('error');
      syncMascot();
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
  class:compact-empty={compact && !visibleMessages.length && !busy}
  class:beside-left={compact && placement === 'left'}
  bind:this={scroll}
>
  {#if !messages.length && !compact}
    <div class="welcome-box">
      <div class="welcome-badge">🌸</div>
      <h2>Hôm nay bạn cần Michiyo hỗ trợ gì không?</h2>
      <p>
        Cứ tự nhiên chia sẻ, hỏi đáp hoặc tâm sự cùng Michiyo như một người bạn
        nhé.
      </p>
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
      <div class="bubble-wrap">
        {#if compact && message.role === 'assistant'}
          <div class="response-meta">
            <span>Michiyo</span>
            {#if busy}
              <span class="stream-label">
                <i aria-hidden="true"></i>{$locale === 'en'
                  ? 'Replying'
                  : 'Đang trả lời'}
              </span>
            {:else if !compact}
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
          class:arriving={!!message.animate &&
            !!message.content &&
            !reducedMotion}
        >
          {#if message.role === 'user'}
            <div class="user-text">{message.content}</div>
            <div class="avatar user-avatar" title="Bạn">
              <svg aria-hidden="true" viewBox="0 0 32 32" fill="none"
                ><circle cx="16" cy="12" r="5" fill="#76988c" /><path
                  d="M6.5 26c.7-5 4.1-8 9.5-8s8.8 3 9.5 8"
                  fill="#76988c"
                /></svg
              >
            </div>
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
    </div>
  {/each}

  {#if busy && !messages.at(-1)?.content}
    <div class="msg-row bot-row thinking-row">
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

{#if compact && compactOverflow}
  <div class="compact-overflow" role="status">
    {busy ? 'Đang trả lời…' : 'Ctrl+Enter để xem toàn bộ câu trả lời'}
  </div>
{/if}

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
  <div
    class="companion-dock"
    role="region"
    aria-label="Bé bot Michiyo"
    class:reduced={reducedMotion}
    class:walking={displayedPose === 'walk'}
    class:walking-left={!atRight}
    data-pose={displayedPose}
  >
    <button
      type="button"
      class="companion-touch-zone"
      style:left={`calc((100% - 88px) * ${dockPosition})`}
      title="Nhấp để tương tác cùng Michiyo"
      onclick={handleMascotClick}
      onpointerenter={() => {
        stopDockMotion();
        mascotMachine.hover(true);
        syncMascot();
      }}
      onpointerleave={() => {
        mascotMachine.hover(false);
        syncMascot();
      }}
    >
      <div class="companion-frame" class:entering>
        {#if entering}<span class="entry-burst" aria-hidden="true"></span>{/if}
        {#if active}<Companion
            state={displayedPose}
            facing={atRight ? 1 : -1}
            speed={config.animationSpeed}
            reduced={reducedMotion}
            skin={config.skin}
          />{/if}
      </div>
    </button>
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
    rows="1"
    bind:this={composer}
    aria-label={t('Tin nhắn', $locale)}
    bind:value={input}
    wrap={compact ? 'off' : 'soft'}
    title={compact
      ? 'Enter để gửi · Esc để đóng · Ctrl+Enter để mở trong app'
      : undefined}
    placeholder={compact
      ? 'Hỏi Michiyo…'
      : t('Nhập tin nhắn tâm sự hoặc câu hỏi cùng Michiyo…', $locale)}
    oninput={() => {
      if (compact) input = input.replace(/[\r\n]+/g, ' ');
      resizeComposer();
      mascotMachine.transition('listening');
      syncMascot();
      if (desktop && !busy) emit('mascot-state', 'listening');
    }}
    onkeydown={(e) => {
      if (compact && e.key === 'Escape') {
        e.preventDefault();
        if (desktop) call('close_chat').catch((e) => (error = String(e)));
        return;
      }
      if (compact && e.key === 'Enter' && e.ctrlKey && !e.isComposing) {
        e.preventDefault();
        void openFullChat();
        return;
      }
      if (compact && e.key === 'Enter' && e.shiftKey && !e.isComposing) {
        e.preventDefault();
        return;
      }
      if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
        e.preventDefault();
        send();
      }
    }}></textarea>

  {#if !compact}<div class="composer-footer">
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
              <svg
                aria-hidden="true"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M22 2 11 13M22 2l-7 20-4-9-9-4 20-7z" />
              </svg>
            {/if}
          </button>
        {/if}
      </div>
    </div>{/if}
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
  .msg-bubble.arriving {
    animation: message-arrive 220ms ease-out both;
  }
  @keyframes message-arrive {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: none;
    }
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
    align-items: flex-start;
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
  .user-avatar {
    background: #e3f0ed;
    border: 1px solid #cde2dc;
  }
  .avatar svg {
    width: 28px;
    height: 28px;
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
  .chat-stream-container:not(.compact-stream) .msg-row article.msg-bubble {
    margin: 0;
    padding: 12px 17px;
  }
  .chat-stream-container:not(.compact-stream) .user-row article.msg-bubble {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 7px 8px 7px 17px;
    background: #e3f3e8;
    color: #244433;
    border: 1px solid #cde7d5;
    border-radius: 18px 18px 4px 18px;
  }
  .user-row .user-text {
    white-space: pre-wrap;
    min-width: 0;
  }
  .chat-stream-container:not(.compact-stream) .bot-row article.msg-bubble {
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
  .compact-stream .thinking-bubble {
    padding: 0;
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
  }
  .dot {
    width: 6px;
    height: 6px;
    background: #50785c;
    border-radius: 50%;
    animation: bounce 1.4s infinite ease-in-out both;
  }
  .dot:nth-child(1) {
    animation-delay: -0.32s;
  }
  .dot:nth-child(2) {
    animation-delay: -0.16s;
  }
  @keyframes bounce {
    0%,
    80%,
    100% {
      transform: scale(0);
      opacity: 0.4;
    }
    40% {
      transform: scale(1);
      opacity: 1;
    }
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
    position: relative;
    height: 72px;
    width: 100%;
    flex: none;
    user-select: none;
  }
  .companion-touch-zone {
    position: absolute;
    bottom: -8px;
    z-index: 1;
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
    outline: none;
    display: flex;
    align-items: flex-end;
    transition: transform 0.2s ease;
  }
  .companion-touch-zone:hover {
    background: transparent;
    transform: translateY(-4px) scale(1.06);
  }
  .companion-dock.reduced .companion-touch-zone {
    transition: none;
  }
  .companion-dock.reduced .companion-touch-zone:hover {
    transform: none;
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
  .entry-burst {
    position: absolute;
    width: 62px;
    height: 62px;
    border: 4px solid #f4c777;
    border-radius: 50%;
    box-shadow:
      0 0 0 8px #fff3d7,
      0 0 18px #edbd7c;
    pointer-events: none;
    animation: entry-burst 650ms ease-out both;
  }
  .entering :global(canvas) {
    animation: entry-appear 650ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
  }
  @keyframes entry-burst {
    0% {
      opacity: 0;
      transform: scale(0.15);
    }
    28% {
      opacity: 1;
    }
    100% {
      opacity: 0;
      transform: scale(1.7);
    }
  }
  @keyframes entry-appear {
    0%,
    20% {
      opacity: 0;
      transform: scale(0.2);
    }
    70% {
      opacity: 1;
      transform: scale(1.08);
    }
    100% {
      opacity: 1;
      transform: scale(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .entry-burst,
    .entering :global(canvas) {
      animation: none;
    }
  }
  .companion-frame :global(.sprite) {
    width: 88px;
    height: 88px;
  }
  /* Chat Composer Form */
  .chat-composer {
    background: #ffffff;
    border: 1px solid #d5dfd5;
    border-radius: 16px;
    padding: 8px 8px 8px 16px;
    box-shadow: 0 4px 16px rgba(41, 68, 50, 0.06);
    display: flex;
    flex-direction: row;
    align-items: flex-end;
    gap: 8px;
    transition:
      border-color 0.15s ease,
      box-shadow 0.15s ease;
    margin-top: 0;
    flex: none;
  }
  .chat-composer:focus-within {
    border-color: #3e6d4e;
    box-shadow: 0 4px 20px rgba(41, 68, 50, 0.11);
  }
  .chat-composer textarea {
    flex: 1;
    width: 100%;
    min-width: 0;
    min-height: 28px;
    max-height: 120px;
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
    flex: none;
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
    padding: 0 2px 3px;
    align-items: center;
    border: 0;
    border-bottom: 1px solid rgba(235, 244, 238, 0.65);
    border-radius: 0;
    background: transparent;
    box-shadow: none;
  }
  .compact-composer:focus-within {
    border-color: rgba(235, 244, 238, 0.9);
    box-shadow: none;
  }
  .compact-composer textarea {
    line-height: 20px;
    padding: 4px 2px;
    color: #f5faf7;
    caret-color: #f5faf7;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.9);
  }
  .compact-composer textarea::placeholder {
    color: #d5dfd8;
    opacity: 0.8;
  }
  @media (prefers-reduced-motion: reduce) {
    .companion-touch-zone {
      transition: none;
    }
    .msg-bubble.arriving {
      animation: none;
    }
  }
  .compact-stream {
    padding: 6px;
    overflow: hidden;
  }
  .compact-stream.compact-empty {
    display: none;
  }
  .compact-overflow {
    flex: none;
    padding: 4px 9px;
    border-radius: 6px;
    background: #fafaf7;
    color: #59664f;
    font-size: 11px;
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
