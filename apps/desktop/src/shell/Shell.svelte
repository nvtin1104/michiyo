<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { locale, t } from '../lib/i18n';
  import { call, desktop, applyTheme, type Config } from '../lib/api';
  import {
    workspace,
    type DueReminder,
    type SyncStatusInfo,
    type TodayAgendaItem,
    type TodayHabitItem,
  } from '../lib/workspace-api';
  import Chat from '../chat/Chat.svelte';
  import Settings from '../settings/Settings.svelte';
  import Notes from '../notes/Notes.svelte';
  import Activity from '../activity/Activity.svelte';
  import Habits from '../habits/Habits.svelte';
  import Calendar from '../calendar/Calendar.svelte';
  import Home from './Home.svelte';
  import ReminderBar from './ReminderBar.svelte';
  import RezeIntro from './RezeIntro.svelte';
  import type { Note, ConversationRecord } from '../lib/types';

  export let config: Config;
  export let save: () => Promise<void>;

  let intro: RezeIntro | undefined;
  let introStaging = false;
  let chatRef: Chat | undefined;

  function startWindowDrag(e: PointerEvent) {
    if (!desktop || e.button !== 0) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest('button, a, input, select, textarea')) return;
    getCurrentWindow()
      .startDragging()
      .catch(() => {});
  }

  function minimizeWindow() {
    if (!desktop) return;
    call('window_minimize').catch(() => {
      getCurrentWindow()
        .minimize()
        .catch(() => {});
    });
  }

  function maximizeWindow() {
    if (!desktop) return;
    call('window_maximize').catch(() => {});
  }

  function closeToMascot() {
    if (!desktop) return;
    call('window_close_to_mascot').catch(() => {
      getCurrentWindow()
        .hide()
        .catch(() => {});
    });
  }

  async function toggleTheme() {
    config.theme = config.theme === 'light' ? 'dark' : 'light';
    applyTheme(config.theme);
    try {
      await save();
    } catch {}
  }

  const workspaceRoutes = [
    ['home', 'Tổng quan'],
    ['notes', 'Ghi chú'],
    ['habits', 'Thói quen'],
    ['calendar', 'Lịch'],
    ['history', 'Lịch sử'],
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

  let conversations: ConversationRecord[] = [];
  let activeConversationId = 'main_continuous_chat';

  async function loadConversations() {
    if (!desktop) {
      try {
        const saved = localStorage.getItem('michiyo_conversations');
        if (saved) {
          conversations = JSON.parse(saved);
          return;
        }
        const single = localStorage.getItem('michiyo_messages');
        if (single) {
          const msgs = JSON.parse(single);
          if (Array.isArray(msgs) && msgs.length) {
            conversations = [
              {
                id: 'main_continuous_chat',
                title: msgs[0]?.content?.slice(0, 36) || 'Hội thoại cùng Michiyo',
                messages: msgs,
              },
            ];
          }
        }
      } catch {}
      return;
    }
    try {
      const list = await call<ConversationRecord[]>('history_list');
      conversations = Array.isArray(list) ? list : [];
    } catch {}
  }

  function handleNewConversation() {
    navigate('chat');
    chatRef?.startNewConversation();
  }

  function handleSelectConversation(id: string) {
    activeConversationId = id;
    navigate('chat');
    chatRef?.selectConversation(id);
  }

  let agenda: TodayAgendaItem[] = [];
  let habitsToDo: TodayHabitItem[] = [];
  let homeError = '';
  const pad = (n: number) => String(n).padStart(2, '0');
  const dayKey = (d: Date) =>
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

  async function loadToday() {
    if (!workspace().isAvailable()) return;
    const now = new Date();
    const start = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    try {
      const [events, habits] = await Promise.all([
        workspace().eventsRange(
          start.getTime(),
          start.getTime() + 864e5,
          -now.getTimezoneOffset(),
        ),
        workspace().habitsList(dayKey(now)),
      ]);
      agenda = events ?? [];
      habitsToDo = (habits ?? []).filter((h) => !h.done_today);
      homeError = '';
    } catch (e) {
      homeError = String(e);
    }
  }

  async function checkIn(habit: TodayHabitItem) {
    try {
      await workspace().habitCheckIn(habit.id, dayKey(new Date()), true);
    } catch (e) {
      homeError = String(e);
    }
    await loadToday();
  }

  let syncInfo: SyncStatusInfo | null = null;
  async function loadSync() {
    if (!workspace().isAvailable()) return;
    try {
      syncInfo = await workspace().syncStatus();
    } catch {
      syncInfo = null;
    }
  }
  $: syncText = !syncInfo?.enabled
    ? 'cục bộ'
    : syncInfo.syncing
      ? 'đang đồng bộ…'
      : syncInfo.last_error
        ? 'lỗi đồng bộ'
        : syncInfo.pending
          ? `${syncInfo.pending} chờ gửi`
          : 'đã đồng bộ';

  let due: DueReminder[] = [];
  let reminderError = '';
  async function loadDue() {
    if (!workspace().isAvailable()) return;
    try {
      due = (await workspace().remindersPending()) ?? [];
    } catch {
      due = [];
    }
  }

  async function answer(item: DueReminder, minutes?: number) {
    reminderError = '';
    try {
      if (minutes) await workspace().reminderSnooze(item.reminder_id, minutes);
      else await workspace().reminderDismiss(item.reminder_id);
    } catch (e) {
      reminderError = String(e);
    }
    await loadDue();
  }

  async function loadPinned() {
    if (!workspace().isAvailable()) return;
    try {
      const notes = await workspace().notesList('');
      pinnedNotes = (Array.isArray(notes) ? notes : [])
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
    let offOpened: (() => void) | undefined;
    let offNotes: (() => void) | undefined;
    let offReminders: (() => void) | undefined;
    let offHistory: (() => void) | undefined;
    const offToday: (() => void)[] = [];
    let disposed = false;
    loadPinned();
    loadToday();
    loadSync();
    loadConversations();
    const syncTimer = setInterval(loadSync, 15000);
    if (desktop)
      listen('sync-changed', loadSync).then((f) =>
        disposed ? f() : offToday.push(f),
      );
    if (desktop)
      listen('history-changed', loadConversations).then((f) =>
        disposed ? f() : (offHistory = f),
      );
    if (desktop)
      for (const name of ['events-changed', 'habits-changed'])
        listen(name, loadToday).then((f) =>
          disposed ? f() : offToday.push(f),
        );
    loadDue();
    if (desktop) {
      workspace()
        .setTimezone(-new Date().getTimezoneOffset())
        .catch(() => {});
      listen('reminders-changed', loadDue).then((f) =>
        disposed ? f() : (offReminders = f),
      );
    }
    if (desktop)
      listen('notes-changed', loadPinned).then((f) =>
        disposed ? f() : (offNotes = f),
      );
    if (desktop) {
      listen<string>('shell-navigate', (e) => navigate(e.payload)).then((f) =>
        disposed ? f() : (off = f),
      );
      listen<string>('shell-opened', (e) => {
        navigate(e.payload || 'chat');
        intro?.play();
      }).then((f) => (disposed ? f() : (offOpened = f)));
    }
    return () => {
      disposed = true;
      off?.();
      offOpened?.();
      offNotes?.();
      offReminders?.();
      offHistory?.();
      offToday.forEach((f) => f());
      clearInterval(syncTimer);
      window.removeEventListener('hashchange', fromHash);
    };
  });
</script>

<div
  class="shell"
  class:intro-staging={introStaging}
  class:chat-layout={isChat}
  class:workspace-layout={isWorkspace}
  class:settings-layout={isSettings}
  class:sidebar-closed={!sidebarOpen}
>
  {#if !isSettings}
    <aside class="shell-sidebar">
      <div class="brand">
        <span class="brand-mark" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 3 4 20h3.5l4.5-10 4.5 10H20L12 3Z" />
          </svg>
        </span>
        <span class="brand-copy">
          <strong>Michiyo</strong>
          <small>AI Workspace</small>
        </span>
      </div>

      <!-- Antigravity-style + New Conversation Glass Button -->
      <button
        type="button"
        class="new-conv-btn"
        onclick={handleNewConversation}
        title={t('Hội thoại mới', $locale)}
      >
        <span class="plus-icon" aria-hidden="true">+</span>
        <span>{t('Hội thoại mới', $locale)}</span>
      </button>

      {#if isWorkspace}
        <button
          type="button"
          class="aside-chat-action"
          onclick={() => navigate('chat')}
          title={t('Trò chuyện', $locale)}
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <path d="M4 5h16v11H9l-5 4V5Z" />
          </svg>
          <span>← {t('Trò chuyện', $locale)}</span>
        </button>
      {/if}

      <nav aria-label="Điều hướng chính">
        <div class="nav-heading">KHÔNG GIAN</div>
        {#each workspaceRoutes as [id, label]}
          <button
            type="button"
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

        {#if conversations.length > 0}
          <div class="nav-heading conv-heading">HỘI THOẠI</div>
          <div class="conv-list">
            {#each conversations.slice(0, 12) as conv (conv.id)}
              <button
                type="button"
                class="conv-item"
                class:active={isChat && activeConversationId === conv.id}
                onclick={() => handleSelectConversation(conv.id)}
                title={conv.title}
              >
                <svg
                  aria-hidden="true"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.6"
                >
                  <path d="M4 5h16v11H9l-5 4V5Z" />
                </svg>
                <span class="conv-title">{conv.title || 'Hội thoại'}</span>
              </button>
            {/each}
          </div>
        {/if}
      </nav>

      <div class="shell-status">
        <div class="status-pill">
          <span class="status-dot"></span>
          <span class="status-label">
            {config.mode === 'offline'
              ? 'Offline'
              : config.mode === 'hybrid'
                ? 'Hybrid'
                : 'Online'} · {config.mode === 'offline' ||
            config.chatProvider === 'local'
              ? config.localModel || 'Ollama Local'
              : config.modelSource === 'workersai'
                ? 'Workers AI'
                : config.chatProvider === 'openai_compat'
                  ? config.openaiModel || 'OpenAI'
                  : config.model}
          </span>
        </div>
        <span class="sync-caption">Lưu trữ: {syncText}</span>
      </div>
    </aside>
  {/if}

  <main class="shell-main" class:intro-staging={introStaging}>
    <RezeIntro
      bind:this={intro}
      bind:staging={introStaging}
      active={true}
      reduced={config.reducedMotion}
    />

    <!-- Frameless Topbar (Antigravity Window Header) -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <header
      class="shell-topbar"
      data-tauri-drag-region
      onpointerdown={startWindowDrag}
    >
      {#if !isSettings}
        <button
          type="button"
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
            <rect x="3" y="4" width="18" height="16" rx="2.5" />
            <path d="M9 4v16" />
          </svg>
        </button>
      {/if}

      {#if isWorkspace}
        <span class="topbar-crumb" data-tauri-drag-region>
          <strong>Không gian làm việc</strong>
          <span class="crumb-sep">/</span>
          {workspaceRoutes.find(([id]) => id === route)?.[1] || ''}
        </span>
      {:else}
        <div class="topbar-brand" data-tauri-drag-region>
          <span class="topbar-logo-mark" aria-hidden="true">M</span>
          <span class="topbar-logo-text">Michiyo</span>
          <span class="topbar-status-dot" title="Sẵn sàng"></span>
        </div>
      {/if}

      <div class="topbar-spacer" data-tauri-drag-region></div>

      <nav class="topbar-nav" aria-label="Điều hướng thanh trên">
        <button
          type="button"
          class="floating-icon-btn theme-toggle-btn"
          onclick={toggleTheme}
          title={config.theme === 'light'
            ? 'Chuyển sang giao diện Đen (Antigravity Dark)'
            : 'Chuyển sang giao diện Trắng (Light)'}
          aria-label="Đổi giao diện Sáng / Tối"
        >
          {#if config.theme === 'light'}
            <svg
              aria-hidden="true"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
            >
              <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
            </svg>
          {:else}
            <svg
              aria-hidden="true"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
            >
              <circle cx="12" cy="12" r="4" />
              <path
                d="M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32 1.41 1.41M2 12h2m16 0h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"
              />
            </svg>
          {/if}
        </button>

        {#if !isChat}
          <button
            type="button"
            class="floating-icon-btn topbar-chat-btn"
            onclick={() => navigate('chat')}
            title={t('Trò chuyện', $locale)}
            aria-label={t('Trò chuyện', $locale)}
          >
            <svg
              aria-hidden="true"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
            >
              <path d="M4 5h16v11H9l-5 4V5Z" />
            </svg>
            <span class="sr-only">{t('Trò chuyện', $locale)}</span>
          </button>
        {/if}

        <button
          type="button"
          class="floating-icon-btn"
          class:active={isWorkspace}
          onclick={() => navigate('home')}
          title={t('Không gian làm việc', $locale)}
          aria-label={t('Không gian làm việc', $locale)}
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <path d="M3 10.5 12 3l9 7.5M5 9v11h14V9M9 20v-6h6v6" />
          </svg>
          <span class="sr-only">{t('Không gian làm việc', $locale)}</span>
        </button>

        <button
          type="button"
          class="floating-icon-btn"
          class:active={isSettings}
          onclick={() => navigate('settings')}
          title={t('Cài đặt', $locale)}
          aria-label={t('Cài đặt', $locale)}
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <circle cx="12" cy="12" r="3" />
            <path
              d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
            />
          </svg>
          <span class="sr-only">{t('Cài đặt', $locale)}</span>
        </button>
      </nav>

      <div class="window-controls" aria-label="Điều khiển cửa sổ">
        <button
          type="button"
          class="win-ctrl-btn win-minimize"
          onclick={minimizeWindow}
          title="Thu gọn"
          aria-label="Thu gọn cửa sổ"
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <path d="M5 12h14" />
          </svg>
        </button>
        <button
          type="button"
          class="win-ctrl-btn win-maximize"
          onclick={maximizeWindow}
          title="Phóng to / Khôi phục"
          aria-label="Phóng to cửa sổ"
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <rect x="5" y="5" width="14" height="14" rx="1.5" />
          </svg>
        </button>
        <button
          type="button"
          class="win-ctrl-btn win-close"
          onclick={closeToMascot}
          title="Đóng cửa sổ"
          aria-label="Đóng cửa sổ"
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
          >
            <path d="M6 6l12 12M18 6 6 18" />
          </svg>
        </button>
      </div>
    </header>

    <ReminderBar {due} error={reminderError} onAnswer={answer} />

    <!-- Primary Chat Route -->
    {#if chatVisited}
      <div class="shell-chat-container" hidden={route !== 'chat'}>
        <Chat
          bind:this={chatRef}
          bind:config
          {save}
          active={isChat}
          onNavigate={navigate}
          onHistoryUpdated={loadConversations}
          bind:currentConversationId={activeConversationId}
        />
      </div>
    {/if}

    <!-- Workspace Routes -->
    {#if route === 'home'}
      <Home
        {agenda}
        {habitsToDo}
        {pinnedNotes}
        {homeError}
        onNavigate={navigate}
        onCheckIn={checkIn}
      />
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
    grid-template-columns: 236px minmax(0, 1fr);
    overflow: hidden;
    background-color: var(--bg-canvas);
    background-image: var(--bg-canvas-glow);
    color: var(--text-primary);
    transition:
      background-color 240ms ease,
      color 240ms ease;
  }

  .shell.intro-staging {
    background: transparent;
  }

  .shell.intro-staging > aside {
    opacity: 0;
  }

  .shell.settings-layout {
    grid-template-columns: minmax(0, 1fr);
  }

  .shell.sidebar-closed:not(.settings-layout) {
    grid-template-columns: 64px minmax(0, 1fr);
  }

  .sidebar-closed > aside {
    padding: 16px 8px 12px;
  }

  .sidebar-closed .brand {
    justify-content: center;
    padding: 0 0 14px;
  }

  .sidebar-closed .brand-copy,
  .sidebar-closed .new-conv-btn span:not(.plus-icon),
  .sidebar-closed .aside-chat-action span,
  .sidebar-closed .nav-heading,
  .sidebar-closed aside nav button span,
  .sidebar-closed .conv-list,
  .sidebar-closed .shell-status {
    display: none;
  }

  .sidebar-closed .new-conv-btn,
  .sidebar-closed .aside-chat-action,
  .sidebar-closed aside nav button {
    justify-content: center;
    padding: 9px;
  }

  /* Antigravity Left Sidebar */
  aside.shell-sidebar {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 16px 12px 14px;
    background: var(--bg-sidebar);
    border-right: 1px solid var(--border-subtle);
    user-select: none;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 4px 14px;
    margin-bottom: 10px;
  }

  .brand-mark {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: var(--bg-glass);
    border: 1px solid var(--border-glass);
    color: var(--text-primary);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--glass-specular);
    flex-shrink: 0;
  }

  .brand-mark svg {
    width: 15px;
    height: 15px;
  }

  .brand-copy {
    display: flex;
    flex-direction: column;
    line-height: 1.22;
    min-width: 0;
  }

  .brand-copy strong {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: -0.01em;
  }

  .brand-copy small {
    font-size: 10.5px;
    letter-spacing: 0.02em;
    color: var(--text-muted);
  }

  .new-conv-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    margin-bottom: 8px;
    border-radius: 9px;
    border: 1px solid var(--border-glass);
    background: var(--bg-glass);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    color: var(--text-primary);
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;
    box-shadow: var(--glass-shadow-sm);
    transition: all 0.15s ease;
  }

  .new-conv-btn:hover {
    background: var(--bg-glass-hover);
    border-color: var(--border-glass-strong);
  }

  .plus-icon {
    font-size: 15px;
    font-weight: 400;
    line-height: 1;
    color: var(--text-secondary);
  }

  .aside-chat-action {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--accent-bg);
    color: var(--accent-fg);
    font-size: 12.5px;
    font-weight: 600;
    border: 1px solid var(--accent-bg);
    cursor: pointer;
    margin-bottom: 10px;
    box-shadow: none;
    transition: opacity 0.15s ease;
  }

  .aside-chat-action svg {
    width: 14px;
    height: 14px;
  }

  .aside-chat-action:hover {
    opacity: 0.9;
  }

  aside nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    border: 0;
    padding: 0;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .nav-heading {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    color: var(--text-muted);
    padding: 10px 8px 4px;
  }

  .conv-heading {
    margin-top: 8px;
    border-top: 1px solid var(--border-subtle);
    padding-top: 12px;
  }

  aside nav button {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 10px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    box-shadow: none;
    color: var(--text-secondary);
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
    transition: all 0.12s ease;
    margin: 0;
  }

  aside nav button svg {
    width: 15px;
    height: 15px;
    flex-shrink: 0;
    opacity: 0.85;
  }

  aside nav button span {
    color: inherit;
    font-size: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  aside nav button:hover {
    background: var(--bg-input);
    color: var(--text-primary);
  }

  aside nav button.active {
    background: var(--bg-glass);
    border-color: var(--border-glass);
    color: var(--text-primary);
    font-weight: 500;
    box-shadow: var(--glass-specular);
  }

  .conv-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .conv-title {
    flex: 1;
    min-width: 0;
  }

  .shell-status {
    margin-top: auto;
    padding-top: 12px;
    border-top: 1px solid var(--border-subtle);
    font-size: 11px;
    color: var(--text-muted);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .status-pill {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-secondary);
    font-size: 11.5px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-primary);
    flex-shrink: 0;
  }

  .sync-caption {
    font-size: 10.5px;
    color: var(--text-muted);
  }

  /* Main Stage */
  .shell-main {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    position: relative;
    background: transparent;
  }

  .shell-topbar,
  .shell-chat-container,
  .shell-page,
  .shell-settings {
    transition: opacity 280ms ease;
  }

  .shell-main.intro-staging .shell-topbar,
  .shell-main.intro-staging .shell-chat-container,
  .shell-main.intro-staging .shell-page,
  .shell-main.intro-staging .shell-settings {
    opacity: 0;
  }

  /* Frameless Topbar (Liquid Glass) */
  .shell-topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px 8px 16px;
    background: var(--bg-topbar);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    border-bottom: 1px solid var(--border-subtle);
    min-height: 46px;
    flex-shrink: 0;
    margin: 0;
    user-select: none;
    z-index: 20;
  }

  .topbar-brand {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .topbar-logo-mark {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: var(--bg-glass);
    border: 1px solid var(--border-glass);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .topbar-logo-text {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: -0.01em;
  }

  .topbar-status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-secondary);
    opacity: 0.7;
  }

  .sidebar-toggle {
    background: transparent;
    border: 1px solid transparent;
    box-shadow: none;
    border-radius: 7px;
    padding: 5px;
    color: var(--text-secondary);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .sidebar-toggle:hover {
    background: var(--bg-glass);
    border-color: var(--border-glass);
    color: var(--text-primary);
  }

  .sidebar-toggle svg {
    width: 16px;
    height: 16px;
  }

  .topbar-crumb {
    font-size: 12.5px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .topbar-crumb strong {
    color: var(--text-primary);
    font-weight: 600;
  }

  .crumb-sep {
    margin: 0 5px;
    color: var(--text-muted);
  }

  .topbar-spacer {
    flex: 1;
    min-width: 12px;
    align-self: stretch;
  }

  .topbar-nav {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 6px;
    border: 0;
    padding: 0;
  }

  .floating-icon-btn {
    width: 32px;
    height: 32px;
    padding: 0;
    border-radius: 8px;
    border: 1px solid var(--border-glass);
    background: var(--bg-glass);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    color: var(--text-secondary);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    box-shadow: var(--glass-specular);
    transition: all 0.14s ease;
    margin: 0;
  }

  .floating-icon-btn svg {
    width: 15px;
    height: 15px;
  }

  .floating-icon-btn:hover {
    background: var(--bg-glass-hover);
    border-color: var(--border-glass-strong);
    color: var(--text-primary);
  }

  .floating-icon-btn.active {
    background: var(--accent-bg);
    color: var(--accent-fg);
    border-color: var(--accent-bg);
  }

  .window-controls {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-left: 6px;
    padding-left: 8px;
    border-left: 1px solid var(--border-subtle);
  }

  .win-ctrl-btn {
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    box-shadow: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all 0.12s ease;
  }

  .win-ctrl-btn svg {
    width: 14px;
    height: 14px;
  }

  .win-ctrl-btn:hover {
    background: var(--bg-input);
    color: var(--text-primary);
  }

  .win-ctrl-btn.win-close:hover {
    background: var(--danger-bg);
    color: var(--danger-fg);
  }

  .shell-chat-container {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 0;
    width: 100%;
    position: relative;
  }

  .shell-chat-container[hidden],
  .shell-page[hidden],
  .shell-settings[hidden] {
    display: none !important;
  }

  .shell-page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 28px 36px;
    max-width: 1060px;
    width: 100%;
    margin: 0 auto;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .shell-settings {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  @media (max-width: 720px) {
    .shell:not(.sidebar-closed) {
      grid-template-columns: 190px minmax(0, 1fr);
    }
    .shell-page {
      padding: 18px 16px;
    }
  }
</style>
