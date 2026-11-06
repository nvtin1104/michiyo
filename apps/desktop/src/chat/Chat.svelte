<script lang="ts">
  import { locale, t } from '../lib/i18n';
  import { onMount, tick, onDestroy } from 'svelte';
  import { listen, emit } from '@tauri-apps/api/event';
  import { newId, type ChatMessage, type ConversationRecord } from '../lib/types';
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { call, desktop, type Config } from '../lib/api';
  import Companion from '../mascot/Companion.svelte';
  import { MascotMachine, type Pose } from '../mascot/machine';

  export let config: Config;
  export let save: (() => Promise<void>) | undefined = undefined;
  export let compact = false;
  export let active = true;
  export let onNavigate: ((route: string) => void) | undefined = undefined;
  export let onHistoryUpdated: (() => void) | undefined = undefined;
  export let currentConversationId = 'main_continuous_chat';

  let manageTimers: () => void = () => {};
  $: (active, manageTimers());

  let composer: HTMLTextAreaElement;
  const focus = () => tick().then(() => composer?.focus());

  type Message = ChatMessage;

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
  let dockEl: HTMLDivElement;
  let dockPointer = { x: 80, y: 80 };
  let dockPressing = false;
  let dockDragging = false;
  let dockPetting = false;
  let dockHeadPress = false;
  let dockStart = { x: 0, y: 0, pos: 0 };
  let dockHoldTimer: ReturnType<typeof setTimeout>;
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
    if (dockDragging || dockPetting) return;
    reactMascot('touched');
  }

  function cancelDockPress() {
    clearTimeout(dockHoldTimer);
    dockPressing = false;
  }

  function formatTime(d = new Date()) {
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  export function startNewConversation() {
    if (busy) return;
    currentConversationId = newId();
    messages = [];
    error = '';
    focus();
  }

  export async function selectConversation(id: string) {
    if (busy) return;
    currentConversationId = id;
    error = '';
    if (!desktop) {
      try {
        const raw = localStorage.getItem('michiyo_conversations');
        if (raw) {
          const list: ConversationRecord[] = JSON.parse(raw);
          const found = list.find((c) => c.id === id);
          messages = found?.messages || [];
          return;
        }
      } catch {}
      return;
    }
    try {
      const conversations = await call<ConversationRecord[]>('history_list');
      const found = conversations.find((c) => c.id === id);
      messages = found?.messages || [];
      tick().then(() => scroll?.scrollTo(0, scroll.scrollHeight));
    } catch (e) {
      error = String(e);
    }
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
        if (draft.current) currentConversationId = draft.current;
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
      composer.style.height = `${Math.min(130, Math.max(32, composer.scrollHeight))}px`;
    }
  }

  async function openFullChat() {
    if (!desktop || busy) return;
    try {
      await call('open_chat_in_main', {
        draft: { current: currentConversationId, messages, input },
      });
      await call('close_chat');
    } catch (e) {
      error = String(e);
    }
  }

  const MAIN_CONVERSATION_ID = 'main_continuous_chat';

  function deriveTitle(msgs: Message[]): string {
    const firstUser = msgs.find((m) => m.role === 'user' && m.content.trim());
    if (!firstUser) return 'Hội thoại cùng Michiyo';
    const text = firstUser.content.trim().replace(/\s+/g, ' ');
    return text.length > 38 ? `${text.slice(0, 38)}…` : text;
  }

  async function persist() {
    const clean = messages.map(({ role, content, time }) => ({
      role,
      content,
      time,
    }));
    const convId = currentConversationId || MAIN_CONVERSATION_ID;
    const title = deriveTitle(messages);

    if (!desktop) {
      try {
        localStorage.setItem('michiyo_messages', JSON.stringify(clean));
        const raw = localStorage.getItem('michiyo_conversations');
        const list: ConversationRecord[] = raw ? JSON.parse(raw) : [];
        const idx = list.findIndex((c) => c.id === convId);
        const item: ConversationRecord = { id: convId, title, messages: clean };
        if (idx >= 0) list[idx] = item;
        else list.unshift(item);
        localStorage.setItem(
          'michiyo_conversations',
          JSON.stringify(list.slice(0, config.historyLimit || 20)),
        );
        onHistoryUpdated?.();
      } catch {}
      return;
    }
    const c = {
      id: convId,
      title,
      messages: messages.map((m) => ({ role: m.role, content: m.content })),
    };
    try {
      await call('history_save', {
        conversation: c,
        limit: config.historyLimit || 200,
      });
      onHistoryUpdated?.();
    } catch (e) {
      error = String(e);
    }
  }

  async function quickSwitchModel(value: string) {
    if (value === 'local') {
      config.modelSource = 'external';
      config.chatProvider = 'local';
      if (!config.localModel) config.localModel = 'gemma3:4b';
    } else if (value === 'openai') {
      config.modelSource = 'external';
      config.chatProvider = 'openai_compat';
    } else {
      config.modelSource = 'external';
      config.chatProvider = 'claude';
      config.model = value;
    }
    try {
      await save?.();
    } catch {}
  }

  $: activeModelValue =
    config.chatProvider === 'local'
      ? 'local'
      : config.chatProvider === 'openai_compat'
        ? 'openai'
        : config.model || 'claude-haiku-5-5';

  let mascotTimer: ReturnType<typeof setInterval>;

  onMount(() => {
    let off: undefined | (() => void);
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
              /* Window may have been closed. */
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
          conversations.find((c) => c.id === currentConversationId) ||
          conversations.find((c) => c.id === MAIN_CONVERSATION_ID) ||
          conversations[0];
        if (main) {
          currentConversationId = main.id;
          if (main.messages?.length) {
            messages = main.messages;
            if (!compact)
              tick().then(() => scroll?.scrollTo(0, scroll.scrollHeight));
          }
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
          if (!busy && messages === before) {
            const target =
              conversations.find((c) => c.id === currentConversationId) ||
              conversations.find((c) => c.id === MAIN_CONVERSATION_ID);
            messages = target?.messages || [];
          }
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
    const targetId = currentConversationId || MAIN_CONVERSATION_ID;
    messages = [];
    input = '';
    error = '';
    if (desktop) {
      try {
        await call('history_delete', { id: targetId });
        onHistoryUpdated?.();
      } catch (e) {
        error = String(e);
      }
    } else {
      localStorage.removeItem('michiyo_messages');
      onHistoryUpdated?.();
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
        input: {
          id: newId(),
          rev: null,
          title: text.slice(0, 32).trim() || 'Ghi chú từ trò chuyện',
          body: text,
          tags: ['trò-chuyện'],
          pinned: false,
        },
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
      <div class="antigravity-emblem" aria-hidden="true">
        <svg viewBox="0 0 64 64" fill="none">
          <path
            d="M32 8C26 8 20.5 15.5 14 36C9.5 50 7 56 11.5 56C15.5 56 18.5 47 23.5 39.5C26.5 35 29.2 33 32 33C34.8 33 37.5 35 40.5 39.5C45.5 47 48.5 56 52.5 56C57 56 54.5 50 50 36C43.5 15.5 38 8 32 8Z"
            fill="currentColor"
          />
        </svg>
      </div>
      <h2>Hôm nay bạn cần Michiyo hỗ trợ gì không?</h2>
      <p>
        Trợ lý AI cá nhân vận hành trên nền tảng Rust Native với không gian làm
        việc tối giản.
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
            <span>{t(suggestion, $locale)}</span>
            <span class="sug-arrow" aria-hidden="true">↗</span>
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

<!-- Companion Dock: Mascot directly above input (draggable & interactive) -->
{#if !compact}
  <div
    class="companion-dock"
    bind:this={dockEl}
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
      class:dragging={dockDragging}
      style:left={`calc((100% - 88px) * ${dockPosition})`}
      title="Nhấp để tương tác · Giữ trên đầu để xoa đầu · Kéo ngang để di chuyển Reze"
      onclick={handleMascotClick}
      onpointerenter={() => {
        if (dockPressing) return;
        stopDockMotion();
        mascotMachine.hover(true);
        syncMascot();
      }}
      onpointerleave={() => {
        if (dockPressing) return;
        mascotMachine.hover(false);
        syncMascot();
      }}
      onpointerdown={(e) => {
        if (e.button !== 0) return;
        stopDockMotion();
        cancelDockPress();
        const r = e.currentTarget.getBoundingClientRect();
        const relY = (e.clientY - r.top) / Math.max(1, r.height);
        dockHeadPress = relY < 0.56;
        dockPressing = true;
        dockDragging = false;
        dockPetting = false;
        dockStart = { x: e.clientX, y: e.clientY, pos: dockPosition };
        e.currentTarget.setPointerCapture(e.pointerId);
        if (dockHeadPress && config.touchReactions) {
          dockHoldTimer = setTimeout(() => {
            if (dockPressing && !dockDragging) {
              dockPetting = true;
              reactMascot('petting');
            }
          }, 420);
        }
      }}
      onpointermove={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        dockPointer = {
          x: ((e.clientX - r.left) / Math.max(1, r.width)) * 160,
          y: ((e.clientY - r.top) / Math.max(1, r.height)) * 160,
        };
        if (!dockPressing) return;
        const dx = e.clientX - dockStart.x;
        const dy = e.clientY - dockStart.y;
        if (!dockDragging && Math.hypot(dx, dy) > 5) {
          dockDragging = true;
          clearTimeout(dockHoldTimer);
          reactMascot(dockHeadPress ? 'dragging' : 'carried');
        }
        if (dockDragging && dockEl) {
          const track = Math.max(1, dockEl.getBoundingClientRect().width - 88);
          dockPosition = Math.max(0, Math.min(1, dockStart.pos + dx / track));
          if (Math.abs(dx) > 2) atRight = dx > 0;
        }
      }}
      onpointerup={(e) => {
        if (e.button !== 0) return;
        const wasDragging = dockDragging;
        cancelDockPress();
        if (e.currentTarget.hasPointerCapture(e.pointerId))
          e.currentTarget.releasePointerCapture(e.pointerId);
        if (wasDragging) {
          reactMascot('settling');
          setTimeout(() => {
            dockDragging = false;
          }, 60);
        } else if (dockPetting) {
          setTimeout(() => {
            dockPetting = false;
          }, 60);
        }
      }}
      onpointercancel={() => {
        cancelDockPress();
        if (dockDragging) reactMascot('settling');
        dockDragging = false;
        dockPetting = false;
      }}
    >
      <div class="companion-frame" class:entering>
        {#if entering}<span class="entry-burst" aria-hidden="true"></span>{/if}
        {#if active}<Companion
            state={displayedPose}
            facing={atRight ? 1 : -1}
            pointer={dockPointer}
            tracking={config.cursorTracking}
            eyeTracking={true}
            speed={config.animationSpeed}
            reduced={reducedMotion}
            skin={config.skin}
          />{/if}
      </div>
    </button>
  </div>
{/if}

<!-- Antigravity Floating Liquid Glass Composer -->
<div class="composer-shell" class:compact-shell={compact}>
  {#if !compact}
    <div class="composer-context-bar">
      <div class="ctx-chips">
        <span class="ctx-chip">
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <path
              d="M3 7v11a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-7l-2-2H5a2 2 0 0 0-2 2Z"
            />
          </svg>
          michiyo
        </span>
        <span class="ctx-chip">
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <circle cx="6" cy="6" r="2.5" />
            <circle cx="18" cy="18" r="2.5" />
            <path d="M6 8.5v7a2.5 2.5 0 0 0 2.5 2.5h7" />
          </svg>
          {config.mode}
        </span>
      </div>
      {#if messages.length > 0}
        <button
          type="button"
          class="ctx-clear-btn"
          onclick={clearHistory}
          title="Xóa hội thoại"
        >
          Xóa lịch sử
        </button>
      {/if}
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
      placeholder={compact ? 'Hỏi Michiyo…' : t('Nhắn cho Michiyo…', $locale)}
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
      }}
    ></textarea>

    {#if !compact}
      <div class="composer-footer">
        <div class="composer-left-tools">
          <button
            type="button"
            class="composer-plus-btn"
            title="Hội thoại mới"
            onclick={startNewConversation}
          >
            +
          </button>
          <select
            class="composer-model-select"
            aria-label="Chọn nhanh model AI"
            value={activeModelValue}
            onchange={(e) =>
              quickSwitchModel((e.currentTarget as HTMLSelectElement).value)}
          >
            <option value="claude-haiku-5-5">Claude Haiku 5.5</option>
            <option value="claude-sonnet-5-5">Claude Sonnet 5.5</option>
            <option value="local"
              >{config.localModel
                ? `Local · ${config.localModel}`
                : 'Ollama · Gemma Local'}</option
            >
            <option value="openai"
              >{config.openaiModel
                ? `OpenAI · ${config.openaiModel}`
                : 'OpenAI-compatible'}</option
            >
          </select>
        </div>

        <div class="composer-buttons">
          <span class="turbo-indicator" aria-hidden="true">⚡ Rust Native</span>
          {#if busy}
            <button
              type="button"
              class="stop-btn"
              aria-label="Dừng trả lời"
              title="Dừng trả lời"
              onclick={() => call('stop_chat', { id: job })}
            >
              ■
            </button>
          {:else}
            <button
              type="submit"
              class="primary send-btn"
              aria-label="Gửi tin nhắn"
              title="Gửi tin nhắn"
              disabled={!input.trim() || !historyReady}
            >
              <svg
                aria-hidden="true"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M5 12h14M13 5l7 7-7 7" />
              </svg>
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </form>
</div>

<style>
  /* Stream & Messages */
  .chat-stream-container {
    flex: 1;
    min-height: 0;
    width: 100%;
    overflow-y: auto;
    padding: 16px 0 36px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .chat-stream-container::-webkit-scrollbar {
    width: 6px;
  }

  .chat-stream-container::-webkit-scrollbar-button {
    display: none;
    width: 0;
    height: 0;
  }

  .chat-stream-container::-webkit-scrollbar-track {
    background: transparent;
  }

  .chat-stream-container::-webkit-scrollbar-thumb {
    background: var(--scrollbar-thumb);
    border-radius: 999px;
  }

  .chat-stream-container::-webkit-scrollbar-thumb:hover {
    background: var(--scrollbar-thumb-hover);
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

  /* Antigravity Welcome Center Stage */
  .welcome-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    width: min(calc(100% - 48px), 780px);
    margin: auto;
    padding: 36px 16px;
  }

  .antigravity-emblem {
    width: 68px;
    height: 68px;
    margin-bottom: 20px;
    color: var(--text-muted);
    opacity: 0.28;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .antigravity-emblem svg {
    width: 100%;
    height: 100%;
  }

  .welcome-box h2 {
    font-size: 21px;
    font-weight: 500;
    color: var(--text-primary);
    margin: 0 0 8px;
    letter-spacing: -0.02em;
  }

  .welcome-box p {
    font-size: 13px;
    color: var(--text-secondary);
    max-width: 440px;
    margin: 0 0 28px;
    line-height: 1.5;
  }

  .suggestions {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 10px;
    width: min(100%, 640px);
  }

  .suggestions button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 11px 15px;
    text-align: left;
    background: var(--bg-glass);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    border: 1px solid var(--border-glass);
    border-radius: 12px;
    font-size: 12.5px;
    color: var(--text-secondary);
    cursor: pointer;
    box-shadow: var(--glass-shadow-sm);
    transition: all 0.16s ease;
  }

  .suggestions button:hover {
    border-color: var(--border-glass-strong);
    background: var(--bg-glass-hover);
    color: var(--text-primary);
    transform: translateY(-1px);
  }

  .sug-arrow {
    color: var(--text-muted);
    font-size: 12px;
  }

  /* Message Rows */
  .msg-row {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    width: min(calc(100% - 48px), 780px);
    margin-left: auto;
    margin-right: auto;
  }

  .compact-stream .msg-row {
    width: 100%;
    max-width: 100%;
  }

  .msg-row.user-row {
    flex-direction: row;
    justify-content: flex-end;
  }

  .msg-row.bot-row {
    justify-content: flex-start;
  }

  .bubble-wrap {
    max-width: 85%;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .compact-stream .bubble-wrap {
    max-width: 100%;
  }

  .user-row .bubble-wrap {
    align-items: flex-end;
  }

  .bot-row .bubble-wrap {
    align-items: flex-start;
  }

  /* Liquid Glass Chat Bubbles */
  .msg-bubble {
    padding: 11px 16px;
    font-size: 13.8px;
    line-height: 1.58;
    word-break: break-word;
    border-radius: 16px;
  }

  .chat-stream-container:not(.compact-stream) .msg-row article.msg-bubble {
    margin: 0;
    padding: 11px 16px;
  }

  .chat-stream-container:not(.compact-stream) .user-row article.msg-bubble {
    display: flex;
    align-items: center;
    padding: 10px 16px;
    background: var(--bg-glass-hover);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    color: var(--text-primary);
    border: 1px solid var(--border-glass-strong);
    border-radius: 16px 16px 4px 16px;
    box-shadow: var(--glass-shadow-sm);
  }

  .user-row .user-text {
    white-space: pre-wrap;
    min-width: 0;
  }

  .chat-stream-container:not(.compact-stream) .bot-row article.msg-bubble {
    background: var(--bg-glass);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    color: var(--text-primary);
    border: 1px solid var(--border-glass);
    border-radius: 16px 16px 16px 4px;
    box-shadow: var(--glass-shadow-sm);
  }

  .msg-footer {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 5px;
    padding: 0 4px;
  }

  .msg-time {
    font-size: 10.5px;
    color: var(--text-muted);
  }

  .msg-actions {
    display: flex;
    gap: 6px;
    opacity: 0.72;
    transition: opacity 0.15s ease;
  }

  .msg-row:hover .msg-actions {
    opacity: 1;
  }

  .msg-actions button {
    background: transparent;
    border: 0;
    box-shadow: none;
    padding: 2px 6px;
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
    border-radius: 5px;
  }

  .msg-actions button:hover {
    background: var(--bg-input);
    color: var(--text-primary);
  }

  /* Thinking Indicator */
  .thinking-bubble {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 11px 16px;
    background: var(--bg-glass);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--border-glass);
    border-radius: 16px 16px 16px 4px;
  }

  .compact-stream .thinking-bubble {
    padding: 0;
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
  }

  .dot {
    width: 5px;
    height: 5px;
    background: var(--text-primary);
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
      opacity: 0.35;
    }
    40% {
      transform: scale(1);
      opacity: 1;
    }
  }

  .thinking-label {
    font-size: 12.5px;
    color: var(--text-secondary);
    margin-left: 4px;
  }

  /* Companion Dock directly above input */
  .companion-dock {
    position: relative;
    height: 1px;
    margin-top: -1px;
    overflow: visible;
    width: min(calc(100% - 48px), 760px);
    margin-left: auto;
    margin-right: auto;
    flex: none;
    user-select: none;
    z-index: 4;
    pointer-events: none;
  }

  .companion-touch-zone {
    position: absolute;
    bottom: -10px;
    z-index: 4;
    pointer-events: auto;
    background: transparent;
    border: 0;
    box-shadow: none;
    padding: 0;
    cursor: grab;
    touch-action: none;
    outline: none;
    display: flex;
    align-items: flex-end;
    transition: transform 0.2s ease;
  }

  .companion-touch-zone.dragging {
    cursor: grabbing;
    transform: translateY(-6px) scale(1.05);
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
  }

  .companion-frame :global(canvas) {
    width: 88px !important;
    height: 88px !important;
    pointer-events: none;
    filter: drop-shadow(0 6px 14px rgba(0, 0, 0, 0.45));
  }

  .entry-burst {
    position: absolute;
    width: 62px;
    height: 62px;
    border: 3px solid var(--border-focus);
    border-radius: 50%;
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

  .companion-frame :global(.sprite) {
    width: 88px;
    height: 88px;
  }

  /* Antigravity 2-Tier Floating Liquid Glass Composer */
  .composer-shell {
    width: min(calc(100% - 48px), 760px);
    margin: 0 auto 18px;
    flex: none;
    border-radius: 18px;
    background: var(--bg-glass);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    border: 1px solid var(--border-glass);
    box-shadow: var(--glass-shadow);
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    transition: border-color 0.16s ease;
  }

  .composer-shell:focus-within {
    border-color: var(--border-focus);
  }

  .composer-shell.compact-shell {
    width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    box-shadow: none;
  }

  .composer-context-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 10px 0;
    font-size: 11.5px;
    color: var(--text-secondary);
  }

  .ctx-chips {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .ctx-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--text-secondary);
    font-size: 11.5px;
  }

  .ctx-chip svg {
    width: 13px;
    height: 13px;
    opacity: 0.75;
  }

  .ctx-clear-btn {
    border: 0;
    box-shadow: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 11px;
    padding: 2px 6px;
    border-radius: 5px;
  }

  .ctx-clear-btn:hover {
    color: var(--text-primary);
    background: var(--bg-input);
  }

  .chat-composer {
    background: var(--bg-glass-composer);
    border: 1px solid var(--border-subtle);
    border-radius: 13px;
    padding: 10px 12px 8px 14px;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
  }

  .chat-composer textarea {
    width: 100%;
    min-width: 0;
    min-height: 32px;
    max-height: 130px;
    resize: none;
    border: 0;
    outline: none;
    font-family: inherit;
    font-size: 13.8px;
    line-height: 1.48;
    color: var(--text-primary);
    background: transparent;
    padding: 2px 0;
  }

  .chat-composer textarea::placeholder {
    color: var(--text-muted);
  }

  .composer-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding-top: 2px;
  }

  .composer-left-tools {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .composer-plus-btn {
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    box-shadow: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 16px;
    line-height: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .composer-plus-btn:hover {
    background: var(--bg-input);
    color: var(--text-primary);
  }

  .composer-model-select {
    padding: 3px 8px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11.5px;
    font-weight: 500;
    cursor: pointer;
    max-width: 200px;
  }

  .composer-model-select:hover,
  .composer-model-select:focus {
    background: var(--bg-input);
    border-color: var(--border-glass);
    color: var(--text-primary);
  }

  .composer-buttons {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }

  .turbo-indicator {
    font-size: 11px;
    color: var(--text-muted);
    letter-spacing: 0.01em;
    user-select: none;
  }

  .send-btn {
    width: 28px;
    height: 28px;
    padding: 0;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-bg);
    color: var(--accent-fg);
    border: 0;
    cursor: pointer;
    transition:
      opacity 0.15s ease,
      transform 0.15s ease;
  }

  .send-btn svg {
    width: 14px;
    height: 14px;
  }

  .send-btn:hover:not(:disabled) {
    transform: scale(1.05);
  }

  .send-btn:disabled {
    opacity: 0.28;
    cursor: not-allowed;
  }

  .stop-btn {
    width: 28px;
    height: 28px;
    padding: 0;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--danger-fg);
    color: #fff;
    border: 0;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
  }

  /* Compact Bubble Mode Overrides */
  .compact-composer {
    width: 100%;
    margin: 0;
    padding: 0 2px 3px;
    flex-direction: row;
    align-items: center;
    border: 0;
    border-bottom: 1px solid var(--border-glass);
    border-radius: 0;
    background: transparent;
    box-shadow: none;
  }

  .compact-composer textarea {
    min-height: 28px !important;
    height: 28px;
    max-height: 72px;
    font-size: 13.5px;
    line-height: 20px;
    padding: 4px 2px;
    color: var(--text-primary);
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
    background: var(--bg-glass);
    color: var(--text-secondary);
    font-size: 11px;
  }

  .toast-note {
    background: var(--bg-glass-popover);
    border: 1px solid var(--border-glass);
    color: var(--text-primary);
    padding: 8px 14px;
    border-radius: 8px;
    font-size: 12px;
    margin-bottom: 8px;
    align-self: center;
    box-shadow: var(--glass-shadow-sm);
  }

  .chat-notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 9px 14px;
    border-radius: 10px;
    font-size: 12.5px;
    background: var(--bg-glass);
    border: 1px solid var(--border-glass);
    color: var(--text-primary);
    width: min(calc(100% - 48px), 760px);
    margin: 0 auto 8px;
  }

  :global(.bubble) .chat-notice {
    width: 100%;
    margin: 0;
  }

  .chat-notice.notice-error {
    background: var(--danger-bg);
    border-color: var(--danger-border);
    color: var(--danger-fg);
  }

  @media (prefers-reduced-motion: reduce) {
    .entry-burst,
    .entering :global(canvas),
    .companion-touch-zone,
    .msg-bubble.arriving {
      animation: none;
      transition: none;
    }
  }
</style>
