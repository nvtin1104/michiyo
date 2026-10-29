<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { locale, t } from '../lib/i18n';
  import { call, desktop, type Config } from '../lib/api';
  import Chat from '../chat/Chat.svelte';
  import Settings from '../settings/Settings.svelte';
  import Notes from '../notes/Notes.svelte';
  import Activity from '../activity/Activity.svelte';
  import Habits from '../habits/Habits.svelte';
  import Calendar from '../calendar/Calendar.svelte';
  import type { Note } from '@michiyo/shared';

  export let config: Config;
  export let save: () => Promise<void>;

  const workspaceRoutes = [
    ['home', 'Tổng quan'],
    ['notes', 'Ghi chú'],
    ['habits', 'Thói quen'],
    ['calendar', 'Lịch'],
    ['history', 'Lịch sử'],
    ['bot', 'Bot'],
  ];

  const routes = [
    ['chat', 'Trò chuyện'],
    ...workspaceRoutes,
    ['settings', 'Cài đặt'],
  ];

  const icons: Record<string, string> = {
    chat: 'M4 5h16v11H9l-5 4V5Z',
    home: 'M3 10.5 12 3l9 7.5M5 9v11h14V9M9 20v-6h6v6',
    notes: 'M5 3h11l3 3v15H5V3Zm3 7h8M8 14h8M8 18h5',
    habits: 'M12 3a9 9 0 1 0 9 9M8 12l3 3 9-10',
    calendar: 'M4 6h16v14H4V6Zm4-3v6M16 3v6M4 10h16',
    history: 'M4 5v5h5M4.5 10A8 8 0 1 1 4 14M12 7v5l3 2',
    bot: 'M5 8h14v11H5V8Zm7-5v5M8 12h.01M16 12h.01M9 16h6',
    settings:
      'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 2v3m0 14v3M2 12h3m14 0h3M5 5l2 2m10 10 2 2M19 5l-2 2M7 17l-2 2',
  };

  let route = 'chat';
  $: isWorkspace = workspaceRoutes.some(([id]) => id === route);
  $: isChat = route === 'chat';
  $: isSettings = route === 'settings';

  let sidebarOpen = true;
  let chatVisited = true;
  let settingsVisited = false;
  let notesVisited = false;
  let habitsVisited = false;
  let calendarVisited = false;
  let pinnedNotes: Note[] = [];

  type Due = {
    reminder_id: string;
    title: string;
    kind: string;
    occurrence_start: number;
  };
  type Today = {
    id: string;
    title: string;
    kind: string;
    done: boolean;
    occurrence_start: number;
  };
  type TodayHabit = {
    id: string;
    title: string;
    streak: number;
    done_today: boolean;
  };

  let agenda: Today[] = [];
  let habitsToDo: TodayHabit[] = [];
  let homeError = '';
  const pad = (n: number) => String(n).padStart(2, '0');
  const dayKey = (d: Date) =>
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

  async function loadToday() {
    if (!desktop) return;
    const now = new Date();
    const start = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    try {
      const [events, habits] = await Promise.all([
        call<Today[]>('events_range', {
          from: start.getTime(),
          until: start.getTime() + 864e5,
          tzOffset: -now.getTimezoneOffset(),
        }),
        call<TodayHabit[]>('habits_list', { today: dayKey(now) }),
      ]);
      agenda = events ?? [];
      habitsToDo = (habits ?? []).filter((h) => !h.done_today);
      homeError = '';
    } catch (e) {
      homeError = String(e);
    }
  }

  async function checkIn(habit: TodayHabit) {
    try {
      await call('habits_check_in', {
        id: habit.id,
        date: dayKey(new Date()),
        done: true,
      });
    } catch (e) {
      homeError = String(e);
    }
    await loadToday();
  }

  type SyncInfo = {
    enabled: boolean;
    pending: number;
    last_error: string | null;
    syncing: boolean;
  };
  let syncInfo: SyncInfo | null = null;
  async function loadSync() {
    if (!desktop) return;
    try {
      syncInfo = await call<SyncInfo>('sync_status');
    } catch {
      syncInfo = null;
    }
  }
  $: syncText = !syncInfo?.enabled
    ? 'chưa kết nối'
    : syncInfo.syncing
      ? 'đang đồng bộ…'
      : syncInfo.last_error
        ? 'lỗi, sẽ thử lại'
        : syncInfo.pending
          ? `${syncInfo.pending} thay đổi chờ gửi`
          : 'đã đồng bộ';

  let due: Due[] = [];
  let reminderError = '';
  async function loadDue() {
    if (!desktop) return;
    try {
      due = (await call<Due[]>('reminders_pending')) ?? [];
    } catch {
      due = [];
    }
  }

  async function answer(item: Due, minutes?: number) {
    reminderError = '';
    try {
      if (minutes)
        await call('reminders_snooze', { id: item.reminder_id, minutes });
      else await call('reminders_dismiss', { id: item.reminder_id });
    } catch (e) {
      reminderError = String(e);
    }
    await loadDue();
  }

  async function loadPinned() {
    if (!desktop) return;
    try {
      pinnedNotes = (await call<Note[]>('notes_list', { query: '' }))
        .filter((note) => note.pinned)
        .slice(0, 5);
    } catch {
      pinnedNotes = [];
    }
  }

  function navigate(next: string) {
    route = routes.some(([id]) => id === next) ? next : 'chat';
    if (route === 'chat') chatVisited = true;
    if (route === 'settings') settingsVisited = true;
    if (route === 'notes') notesVisited = true;
    if (route === 'habits') habitsVisited = true;
    if (route === 'calendar') calendarVisited = true;
    if (location.hash !== `#/${route}`) location.hash = `#/${route}`;
  }

  onMount(() => {
    const fromHash = () => navigate(location.hash.slice(2) || 'chat');
    fromHash();
    window.addEventListener('hashchange', fromHash);
    let off: (() => void) | undefined;
    let offNotes: (() => void) | undefined;
    let offReminders: (() => void) | undefined;
    const offToday: (() => void)[] = [];
    loadPinned();
    loadToday();
    loadSync();
    const syncTimer = setInterval(loadSync, 15000);
    if (desktop)
      listen('sync-changed', loadSync).then((f) =>
        disposed ? f() : offToday.push(f),
      );
    if (desktop)
      for (const name of ['events-changed', 'habits-changed'])
        listen(name, loadToday).then((f) =>
          disposed ? f() : offToday.push(f),
        );
    loadDue();
    if (desktop) {
      call('set_timezone', {
        offset: -new Date().getTimezoneOffset(),
      }).catch(() => {});
      listen('reminders-changed', loadDue).then((f) =>
        disposed ? f() : (offReminders = f),
      );
    }
    if (desktop)
      listen('notes-changed', loadPinned).then((f) =>
        disposed ? f() : (offNotes = f),
      );
    let disposed = false;
    if (desktop)
      listen<string>('shell-navigate', (e) => navigate(e.payload)).then((f) =>
        disposed ? f() : (off = f),
      );
    return () => {
      disposed = true;
      off?.();
      offNotes?.();
      offReminders?.();
      offToday.forEach((f) => f());
      clearInterval(syncTimer);
      window.removeEventListener('hashchange', fromHash);
    };
  });
</script>

<div
  class="shell"
  class:chat-layout={isChat}
  class:workspace-layout={isWorkspace}
  class:settings-layout={isSettings}
  class:sidebar-closed={!sidebarOpen && isWorkspace}
>
  {#if isWorkspace}
    <aside>
      <div class="brand">
        <span class="brand-mark" aria-hidden="true">M</span>
        <span class="brand-copy">
          <strong>Michiyo</strong>
          <small>Không gian làm việc</small>
        </span>
      </div>

      <!-- Quick back button to Chat -->
      <button
        class="aside-chat-action"
        onclick={() => navigate('chat')}
        title={t('Trò chuyện', $locale)}
      >
        <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9">
          <path d="M4 5h16v11H9l-5 4V5Z" />
        </svg>
        <span>← {t('Trò chuyện', $locale)}</span>
      </button>

      <nav aria-label="Điều hướng chính">
        <div class="nav-heading">CÔNG CỤ QUẢN LÝ</div>
        {#each workspaceRoutes as [id, label]}
          <button
            class:active={route === id}
            aria-label={label}
            title={label}
            aria-current={route === id ? 'page' : undefined}
            onclick={() => navigate(id)}
          >
            <svg
              aria-hidden="true"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.7"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d={icons[id]} />
            </svg>
            <span>{label}</span>
          </button>
        {/each}
      </nav>

      <div class="shell-status">
        <span class="status-title">TRẠNG THÁI</span>
        <span>
          {config.mode === 'offline'
            ? 'Offline'
            : config.mode === 'hybrid'
              ? 'Hybrid'
              : 'Online'} · {config.modelSource === 'cloudflare'
            ? 'Cloudflare'
            : config.chatProvider === 'openai_compat'
              ? config.openaiModel || 'OpenAI-compatible'
              : config.model}
        </span>
        <span><i aria-hidden="true"></i> Đồng bộ: {syncText}</span>
      </div>
    </aside>
  {/if}

  <main class="shell-main">
    <!-- Top Navigation Bar -->
    <header class="shell-topbar">
      {#if isWorkspace}
        <button
          class="sidebar-toggle"
          aria-label={sidebarOpen ? 'Đóng thanh bên' : 'Mở thanh bên'}
          aria-expanded={sidebarOpen}
          onclick={() => (sidebarOpen = !sidebarOpen)}
          title={sidebarOpen ? 'Đóng thanh bên' : 'Mở thanh bên'}
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.7"
          >
            <rect x="3" y="4" width="18" height="16" rx="2" />
            <path d="M9 4v16" />
          </svg>
        </button>
        <span class="topbar-crumb">
          <strong>Không gian làm việc</strong> / {workspaceRoutes.find(([id]) => id === route)?.[1] || ''}
        </span>
      {:else}
        <div class="topbar-brand">
          <span class="topbar-logo-icon">🌸</span>
          <span class="topbar-logo-text">Michiyo</span>
          <span class="topbar-status-dot" title="Sẵn sàng"></span>
        </div>
      {/if}

      <div class="topbar-spacer"></div>

      <!-- 2 primary navigation buttons as requested: floating icon buttons -->
      <nav class="topbar-nav" aria-label="Điều hướng thanh trên">
        {#if !isChat}
          <button
            class="floating-icon-btn topbar-chat-btn"
            onclick={() => navigate('chat')}
            title={t('Trò chuyện', $locale)}
            aria-label={t('Trò chuyện', $locale)}
          >
            <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
              <path d="M4 5h16v11H9l-5 4V5Z" />
            </svg>
            <span class="sr-only">{t('Trò chuyện', $locale)}</span>
          </button>
        {/if}

        <button
          class="floating-icon-btn"
          class:active={isWorkspace}
          onclick={() => navigate('home')}
          title={t('Không gian làm việc', $locale)}
          aria-label={t('Không gian làm việc', $locale)}
        >
          <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
            <path d="M3 10.5 12 3l9 7.5M5 9v11h14V9M9 20v-6h6v6" />
          </svg>
          <span class="sr-only">{t('Không gian làm việc', $locale)}</span>
        </button>

        <button
          class="floating-icon-btn"
          class:active={isSettings}
          onclick={() => navigate('settings')}
          title={t('Cài đặt', $locale)}
          aria-label={t('Cài đặt', $locale)}
        >
          <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
          <span class="sr-only">{t('Cài đặt', $locale)}</span>
        </button>
      </nav>
    </header>

    {#if due.length}
      <div class="reminders" role="region" aria-label="Nhắc nhở">
        {#each due as item (item.reminder_id)}
          <div class="reminder">
            <span>
              ⏰ <strong>{item.title}</strong>
              <small>{new Date(item.occurrence_start).toLocaleTimeString()}</small>
            </span>
            <button onclick={() => answer(item, 10)}>Hoãn 10 phút</button>
            <button onclick={() => answer(item)}>Đã rõ</button>
          </div>
        {/each}
        {#if reminderError}<p class="error" role="alert">{reminderError}</p>{/if}
      </div>
    {/if}

    <!-- Primary Chat Route -->
    {#if chatVisited}
      <div class="shell-chat-container" hidden={route !== 'chat'}>
        <Chat {config} onNavigate={navigate} />
      </div>
    {/if}

    <!-- Workspace Routes -->
    {#if route === 'home'}
      <section class="shell-page">
        <header>
          <div>
            <small>CHÀO MỪNG TRỞ LẠI</small>
            <h1>Một ngày cùng Michiyo</h1>
          </div>
        </header>
        <p>Quản lý ghi chú, lịch trình, thói quen và hoạt động cùng trợ lý của bạn.</p>
        <div class="cards">
          <button onclick={() => navigate('chat')}>✦ Bắt đầu trò chuyện</button>
          <button onclick={() => navigate('notes')}>📝 Mở ghi chú</button>
        </div>
        {#if homeError}<p class="error" role="alert">{homeError}</p>{/if}
        <div class="home-grid">
          <section aria-label="Hôm nay">
            <h2>Hôm nay</h2>
            {#each agenda as item}
              <p class:done={item.done}>
                {new Date(item.occurrence_start).toLocaleTimeString([], {
                  hour: '2-digit',
                  minute: '2-digit',
                })}
                {item.kind === 'todo' ? '☐' : '•'}
                {item.title}
              </p>
            {:else}
              <p class="muted">Không có lịch hôm nay.</p>
            {/each}
            <button onclick={() => navigate('calendar')}>Mở lịch</button>
          </section>
          <section aria-label="Thói quen chưa check-in">
            <h2>Thói quen hôm nay</h2>
            {#each habitsToDo as habit}
              <p class="habit-row">
                <span>{habit.title} <small>🔥 {habit.streak}</small></span>
                <button
                  aria-label={`Check-in ${habit.title}`}
                  onclick={() => checkIn(habit)}>✓</button>
              </p>
            {:else}
              <p class="muted">Đã hoàn thành hết hoặc chưa có thói quen.</p>
            {/each}
            <button onclick={() => navigate('habits')}>Mở thói quen</button>
          </section>
          <section aria-label="Ghi chú ghim">
            <h2>Ghi chú ghim</h2>
            {#each pinnedNotes as note}
              <p>📌 {note.title}</p>
            {:else}
              <p class="muted">Chưa có ghi chú ghim.</p>
            {/each}
            <button onclick={() => navigate('notes')}>Mở ghi chú</button>
          </section>
        </div>
      </section>
    {:else if isWorkspace && !['home', 'notes', 'habits', 'calendar', 'history'].includes(route)}
      <section class="shell-page">
        <header><h1>{routes.find(([id]) => id === route)?.[1]}</h1></header>
        <p>Tính năng này đang được phát triển.</p>
      </section>
    {/if}

    {#if route === 'history'}
      <div class="shell-page"><Activity /></div>
    {/if}
    {#if notesVisited}
      <div class="shell-page" hidden={route !== 'notes'}>
        <Notes />
      </div>
    {/if}
    {#if habitsVisited}
      <div class="shell-page" hidden={route !== 'habits'}>
        <Habits />
      </div>
    {/if}
    {#if calendarVisited}
      <div class="shell-page" hidden={route !== 'calendar'}>
        <Calendar />
      </div>
    {/if}

    <!-- Settings Route -->
    {#if settingsVisited}
      <div class="shell-settings" hidden={route !== 'settings'}>
        <Settings bind:config {save} />
      </div>
    {/if}
  </main>
</div>

<style>
  .shell {
    height: 100dvh;
    display: grid;
    overflow: hidden;
    background: #f6f8f5;
    color: #24352b;
  }
  .shell.chat-layout,
  .shell.settings-layout {
    grid-template-columns: minmax(0, 1fr);
  }
  .shell.workspace-layout {
    grid-template-columns: 224px minmax(0, 1fr);
  }
  .shell.workspace-layout.sidebar-closed {
    grid-template-columns: minmax(0, 1fr);
  }
  .sidebar-closed > aside {
    display: none;
    visibility: hidden;
    overflow: hidden;
    padding: 0;
    border: 0;
  }

  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 20px 12px 14px;
    background: #eef2ed;
    border-right: 1px solid #dce4dc;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 4px 16px;
    border-bottom: 1px solid #dbe4db;
    margin-bottom: 12px;
  }
  .brand-mark {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: #365b43;
    color: #fff;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 14px;
  }
  .brand-copy {
    display: flex;
    flex-direction: column;
    line-height: 1.25;
  }
  .brand-copy strong {
    font-size: 14.5px;
  }
  .brand-copy small {
    font-size: 11px;
    color: #6d8072;
  }

  .aside-chat-action {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 12px;
    border-radius: 9px;
    background: #345c43;
    color: #ffffff;
    font-size: 13px;
    font-weight: 600;
    border: 0;
    cursor: pointer;
    margin-bottom: 14px;
    transition: background 0.15s ease;
  }
  .aside-chat-action svg {
    width: 15px;
    height: 15px;
  }
  .aside-chat-action:hover {
    background: #284734;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-height: 0;
    overflow-y: auto;
  }
  .nav-heading {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: #7b8d7e;
    padding: 6px 8px 4px;
  }
  nav button {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 8px 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: #405547;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    transition: all 0.12s ease;
  }
  nav button svg {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }
  nav button:hover {
    background: #e2ebe1;
    color: #1f3024;
  }
  nav button.active {
    background: #ffffff;
    color: #244b33;
    font-weight: 600;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
  }

  .shell-status {
    margin-top: auto;
    padding-top: 14px;
    border-top: 1px solid #dbe4db;
    font-size: 11px;
    color: #6a7c6e;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .status-title {
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: #849688;
  }

  .shell-main {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    position: relative;
  }

  /* Shell Topbar */
  .shell-topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 24px;
    background: #ffffff;
    border-bottom: 1px solid #e2e8e1;
    min-height: 48px;
    flex-shrink: 0;
  }
  .chat-layout .shell-topbar {
    background: transparent;
    border-bottom: 0;
    padding: 16px 32px 0;
    min-height: unset;
  }
  .topbar-brand {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .topbar-logo-icon {
    font-size: 18px;
  }
  .topbar-logo-text {
    font-size: 16px;
    font-weight: 700;
    color: #213527;
    letter-spacing: -0.01em;
  }
  .topbar-status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #3e9458;
    margin-left: 2px;
  }
  .sidebar-toggle {
    background: transparent;
    border: 1px solid #dbe3da;
    border-radius: 6px;
    padding: 5px;
    color: #55675a;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .sidebar-toggle svg {
    width: 16px;
    height: 16px;
  }
  .topbar-crumb {
    font-size: 13px;
    color: #627566;
  }
  .topbar-crumb strong {
    color: #25392d;
  }
  .topbar-spacer {
    flex: 1;
  }

  .topbar-nav {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .floating-icon-btn {
    width: 38px;
    height: 38px;
    border-radius: 12px;
    background: #ffffff;
    border: 1px solid #dce4dc;
    color: #44594a;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    box-shadow: 0 2px 8px rgba(40, 70, 50, 0.05);
    transition: all 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .floating-icon-btn svg {
    width: 18px;
    height: 18px;
  }
  .floating-icon-btn:hover {
    background: #eef4ee;
    color: #224730;
    border-color: #cad8cc;
    transform: translateY(-2px);
    box-shadow: 0 4px 14px rgba(40, 70, 50, 0.12);
  }
  .floating-icon-btn.active,
  .floating-icon-btn.topbar-chat-btn {
    background: #345c43;
    border-color: #345c43;
    color: #ffffff;
  }
  .floating-icon-btn.topbar-chat-btn:hover {
    background: #274733;
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

  /* Chat Container */
  .shell-chat-container {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    max-width: 900px;
    width: 100%;
    margin: 0 auto;
    padding: 16px 24px 20px;
    overflow: hidden;
  }

  /* Reminders */
  .reminders {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 24px;
    background: #fdf6e7;
    border-bottom: 1px solid #f3e3be;
  }
  .reminder {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
  }
  .reminder button {
    padding: 3px 8px;
    font-size: 11px;
    border-radius: 5px;
    border: 1px solid #d7c598;
    background: #fff;
    cursor: pointer;
  }

  /* Workspace Pages */
  .shell-page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 32px 40px;
    max-width: 1100px;
    width: 100%;
    margin: 0 auto;
    scrollbar-width: thin;
    scrollbar-color: #c9d2ca transparent;
  }
  .shell-settings {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .home-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
    margin-top: 24px;
  }
  .home-grid section {
    padding: 16px;
    background: #fff;
    border: 1px solid #e1e7e0;
    border-radius: 12px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.02);
  }
  .home-grid h2 {
    margin: 0 0 10px;
    font-size: 15px;
    color: #24382c;
  }
  .home-grid p {
    font-size: 13.5px;
    margin: 6px 0;
    color: #3f5446;
  }
  .home-grid p.done {
    text-decoration: line-through;
    opacity: 0.55;
  }
  .home-grid button {
    margin-top: 10px;
    padding: 5px 12px;
    border-radius: 6px;
    border: 1px solid #d4dfd4;
    background: #f5f8f5;
    font-size: 12px;
    cursor: pointer;
  }
  .habit-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .cards {
    display: flex;
    gap: 12px;
    margin: 16px 0;
  }
  .cards button {
    padding: 14px 20px;
    background: #fff;
    border: 1px solid #d7e0d7;
    border-radius: 10px;
    font-size: 13.5px;
    font-weight: 600;
    color: #284b35;
    cursor: pointer;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.03);
    transition: all 0.15s ease;
  }
  .cards button:hover {
    background: #f2f7f2;
    transform: translateY(-1px);
  }

  @media (max-width: 760px) {
    .shell.workspace-layout {
      grid-template-columns: 180px minmax(0, 1fr);
    }
    .shell-chat-container {
      padding: 10px 14px 14px;
    }
    .shell-page {
      padding: 20px 18px;
    }
  }
</style>
