<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { call, desktop, type Config } from '../lib/api';
  import Chat from '../chat/Chat.svelte';
  import Translate from '../translate/Translate.svelte';
  import Settings from '../settings/Settings.svelte';
  import Notes from '../notes/Notes.svelte';
  import Activity from '../activity/Activity.svelte';
  import Habits from '../habits/Habits.svelte';
  import Calendar from '../calendar/Calendar.svelte';
  import type { Note } from '@michiyo/shared';
  export let config: Config;
  export let save: () => Promise<void>;
  const routes = [
    ['home', 'Trang chủ'],
    ['chat', 'Trò chuyện'],
    ['translate', 'Dịch'],
    ['notes', 'Ghi chú'],
    ['habits', 'Thói quen'],
    ['calendar', 'Lịch'],
    ['history', 'Lịch sử'],
    ['bot', 'Bot'],
    ['settings', 'Cài đặt'],
  ];
  const icons: Record<string, string> = {
    home: 'M3 10.5 12 3l9 7.5M5 9v11h14V9M9 20v-6h6v6',
    chat: 'M4 5h16v11H9l-5 4V5Z',
    translate: 'M4 6h10M9 3v3c0 6-2 9-5 11M6 10c1 3 3 5 6 7M14 20l3-9 3 9M15 17h4',
    notes: 'M5 3h11l3 3v15H5V3Zm3 7h8M8 14h8M8 18h5',
    habits: 'M12 3a9 9 0 1 0 9 9M8 12l3 3 9-10',
    calendar: 'M4 6h16v14H4V6Zm4-3v6M16 3v6M4 10h16',
    history: 'M4 5v5h5M4.5 10A8 8 0 1 1 4 14M12 7v5l3 2',
    bot: 'M5 8h14v11H5V8Zm7-5v5M8 12h.01M16 12h.01M9 16h6',
    settings: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 2v3m0 14v3M2 12h3m14 0h3M5 5l2 2m10 10 2 2M19 5l-2 2M7 17l-2 2',
  };
  let route = 'chat';
  let sidebarOpen = true;
  let chatVisited = true;
  let translateVisited = false;
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
    if (route === 'translate') translateVisited = true;
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
      call('set_timezone', { offset: -new Date().getTimezoneOffset() }).catch(() => {});
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

<div class="shell" class:sidebar-closed={!sidebarOpen}>
  <aside>
    <div class="brand"><span class="brand-mark" aria-hidden="true">M</span><span class="brand-copy"><strong>Michiyo</strong><small>Không gian của bạn</small></span></div>
    <nav aria-label="Điều hướng chính">
      <div class="nav-heading">KHÔNG GIAN LÀM VIỆC</div>
      {#each routes as [id, label]}
        <button
          class:active={route === id}
          aria-label={label}
          title={label}
          aria-current={route === id ? 'page' : undefined}
          onclick={() => navigate(id)}><svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d={icons[id]} /></svg><span>{label}</span></button
        >
      {/each}
    </nav>
    <div class="shell-status">
      <span class="status-title">TRẠNG THÁI</span>
      <span
        >{config.mode === 'offline'
          ? 'Offline'
          : config.mode === 'hybrid'
            ? 'Hybrid'
            : 'Online'} · {config.modelSource === 'cloudflare'
          ? 'Cloudflare'
          : config.chatProvider === 'openai_compat'
            ? config.openaiModel || 'OpenAI-compatible'
            : config.model}</span
      ><span><i aria-hidden="true"></i> Đồng bộ: {syncText}</span>
    </div>
  </aside>
  <main class="shell-main">
    <div class="shell-topbar">
      <button class="sidebar-toggle" aria-label={sidebarOpen ? 'Đóng thanh bên' : 'Mở thanh bên'} aria-expanded={sidebarOpen} onclick={() => (sidebarOpen = !sidebarOpen)} title={sidebarOpen ? 'Đóng thanh bên' : 'Mở thanh bên'}>
        <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/></svg>
      </button>
      <strong>Michiyo</strong>
      <span class="topbar-spacer"></span>
      <button class="topbar-link" onclick={() => navigate('home')}>Không gian làm việc</button>
      <button class="topbar-link" onclick={() => navigate('settings')}>Cài đặt</button>
    </div>
    {#if due.length}<div class="reminders" role="region" aria-label="Nhắc nhở">
        {#each due as item (item.reminder_id)}
          <div class="reminder">
            <span
              >⏰ <strong>{item.title}</strong>
              <small>{new Date(item.occurrence_start).toLocaleTimeString()}</small></span
            >
            <button onclick={() => answer(item, 10)}>Hoãn 10 phút</button>
            <button onclick={() => answer(item)}>Đã rõ</button>
          </div>
        {/each}
        {#if reminderError}<p class="error" role="alert">{reminderError}</p>{/if}
      </div>{/if}
    {#if route === 'home'}
      <section class="shell-page">
        <header>
          <div>
            <small>CHÀO MỪNG TRỞ LẠI</small>
            <h1>Một ngày cùng Michiyo <span>✦</span></h1>
          </div>
        </header>
        <p>Trò chuyện, dịch và quản lý cuộc sống ở một nơi.</p>
        <div class="cards">
          <button onclick={() => navigate('chat')}>✦ Bắt đầu trò chuyện</button
          ><button onclick={() => navigate('translate')}>↔ Dịch văn bản</button>
        </div>
        {#if homeError}<p class="error" role="alert">{homeError}</p>{/if}
        <div class="home-grid">
          <section aria-label="Hôm nay">
            <h2>Hôm nay</h2>
            {#each agenda as item}<p class:done={item.done}>
                {new Date(item.occurrence_start).toLocaleTimeString([], {
                  hour: '2-digit',
                  minute: '2-digit',
                })}
                {item.kind === 'todo' ? '☐' : '•'}
                {item.title}
              </p>{:else}<p class="muted">Không có lịch hôm nay.</p>{/each}
            <button onclick={() => navigate('calendar')}>Mở lịch</button>
          </section>
          <section aria-label="Thói quen chưa check-in">
            <h2>Thói quen hôm nay</h2>
            {#each habitsToDo as habit}<p class="habit-row">
                <span>{habit.title} <small>🔥 {habit.streak}</small></span
                ><button
                  aria-label={`Check-in ${habit.title}`}
                  onclick={() => checkIn(habit)}>✓</button
                >
              </p>{:else}<p class="muted">
                Đã hoàn thành hết hoặc chưa có thói quen.
              </p>{/each}
            <button onclick={() => navigate('habits')}>Mở thói quen</button>
          </section>
          <section aria-label="Ghi chú ghim">
            <h2>Ghi chú ghim</h2>
            {#each pinnedNotes as note}<p>📌 {note.title}</p>{:else}<p
                class="muted"
              >
                Chưa có ghi chú ghim.
              </p>{/each}
            <button onclick={() => navigate('notes')}>Mở ghi chú</button>
          </section>
        </div>
      </section>
    {:else if !['chat', 'translate', 'settings', 'notes', 'habits', 'calendar', 'history'].includes(route)}<section
        class="shell-page"
      >
        <header><h1>{routes.find(([id]) => id === route)?.[1]}</h1></header>
        <p>Tính năng này đang được phát triển.</p>
      </section>{/if}
    {#if route === 'history'}<div class="shell-page"><Activity /></div>{/if}
    {#if notesVisited}<div class="shell-page" hidden={route !== 'notes'}>
        <Notes />
      </div>{/if}
    {#if habitsVisited}<div class="shell-page" hidden={route !== 'habits'}>
        <Habits />
      </div>{/if}
    {#if calendarVisited}<div class="shell-page" hidden={route !== 'calendar'}>
        <Calendar />
      </div>{/if}
    {#if chatVisited}<div class="shell-page shell-chat" hidden={route !== 'chat'}>
        <Chat {config} onNavigate={navigate} />
      </div>{/if}
    {#if translateVisited}<div
        class="shell-page"
        hidden={route !== 'translate'}
      >
        <Translate />
      </div>{/if}
    {#if settingsVisited}<div
        class="shell-settings"
        hidden={route !== 'settings'}
      >
        <Settings bind:config {save} />
      </div>{/if}
  </main>
</div>

<style>
  .shell {
    height: 100dvh;
    display: grid;
    grid-template-columns: 224px minmax(0, 1fr);
    overflow: hidden;
    background: #f7f8f6;
    color: #26352f;
  }
  .shell.sidebar-closed { grid-template-columns: 0 minmax(0, 1fr); }
  .sidebar-closed > aside { visibility: hidden; overflow: hidden; padding: 0; border: 0; }
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 22px 12px 14px;
    background: #f1f4f0;
    border-right: 1px solid #dfe6df;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 2px 8px 24px;
  }
  .brand-mark {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: #385e4b;
    color: white;
    font-size: 18px;
    font-weight: 700;
  }
  .brand-copy { display: grid; gap: 2px; min-width: 0; }
  .brand-copy strong { font-size: 16px; letter-spacing: -.3px; }
  .brand-copy small { color: #78877d; font-size: 10px; letter-spacing: 0; white-space: nowrap; }
  nav {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
    gap: 2px;
    scrollbar-width: thin;
    border: 0;
    padding: 0;
  }
  .nav-heading { padding: 0 10px 9px; color: #8a988e; font-size: 9px; font-weight: 700; letter-spacing: .08em; }
  nav button {
    display: flex;
    align-items: center;
    gap: 11px;
    min-height: 39px;
    width: 100%;
    text-align: left;
    border: 0;
    border-radius: 7px;
    padding: 9px 10px;
    margin: 0;
    background: transparent;
    color: #596c61;
    font-size: 13px;
    cursor: pointer;
  }
  nav button svg { width: 18px; height: 18px; flex: none; }
  nav button span { color: inherit; font-size: inherit; }
  nav button:hover { background: #e3ebe3; color: #284b37; }
  nav button:focus-visible { outline: 2px solid #638974; outline-offset: -2px; }
  nav button.active {
    background: #dbe8dc;
    color: #244a34;
    font-weight: 600;
  }
  .shell-status {
    display: flex;
    flex-direction: column;
    gap: 7px;
    margin-top: auto;
    padding: 15px 10px 0;
    border-top: 1px solid #dce5dd;
    color: #738278;
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .status-title { font-size: 9px; font-weight: 700; letter-spacing: .08em; color: #91a096; }
  .shell-status i { display: inline-block; width: 6px; height: 6px; border-radius: 50%; background: #86a990; margin-right: 3px; }
  [hidden] {
    display: none !important;
  }
  .shell-main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    height: 100%;
    overflow: hidden;
    background: #fafbf9;
  }
  .shell-topbar { flex: none; display: flex; align-items: center; gap: 10px; min-height: 56px; padding: 8px 22px; border-bottom: 1px solid #e6ebe6; background: #fafbf9; }
  .shell-topbar strong { font-size: 14px; color: #344b3d; }
  .topbar-spacer { flex: 1; }
  .shell-topbar button { border: 0; background: transparent; color: #617168; font-size: 12px; padding: 8px 10px; }
  .shell-topbar button:hover { background: #edf2ed; }
  .sidebar-toggle { display: grid; place-items: center; }
  .sidebar-toggle svg { width: 19px; height: 19px; }
  .shell-page {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    height: auto;
    width: 100%;
    max-width: 1120px;
    padding: 36px 40px;
    margin: auto;
    overflow: auto;
    scrollbar-width: thin;
    scrollbar-color: #c9d2ca transparent;
  }
  .shell-page :global(header h1 span) { display: none; }
  .home-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
  }
  .home-grid section {
    padding: 14px;
    background: #fff;
    border: 1px solid #e0e7e0;
    border-radius: 10px;
  }
  .home-grid h2 {
    margin: 0 0 8px;
    font-size: 16px;
  }
  .home-grid p {
    margin: 4px 0;
  }
  .home-grid p.done {
    text-decoration: line-through;
    opacity: 0.6;
  }
  .habit-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .reminders {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 32px;
    background: #f3e9cf;
    border-bottom: 1px solid #e0d1a6;
  }
  .reminder {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .reminder span {
    flex: 1;
    min-width: 0;
  }
  .shell-settings {
    flex: 1;
    min-height: 0;
  }
  .shell-chat { max-width: 900px; padding: 20px 32px 26px; }
  .shell-chat :global(.chat-header) { margin-bottom: 14px; padding-bottom: 12px; }
  .shell-chat :global(.chat-header h1) { font-size: 16px; letter-spacing: 0; margin: 0; }
  .shell-chat :global(.toolbar) { gap: 8px; }
  .shell-chat :global(.toolbar select) { max-width: 300px; }
  .shell-chat :global(.messages) { padding: 0 12px; scrollbar-width: thin; }
  .shell-chat :global(.welcome) { display: flex; min-height: 100%; flex-direction: column; justify-content: center; align-items: center; padding: 20px 0; }
  .shell-chat :global(.welcome h2) { font-size: 25px; font-weight: 600; letter-spacing: -.03em; }
  .shell-chat :global(.welcome p) { margin: 8px 0; }
  .shell-chat :global(.suggestions) { grid-template-columns: repeat(auto-fit, minmax(165px, 1fr)); width: min(100%, 620px); margin-top: 24px; }
  .shell-chat :global(.suggestions button) { background: #fff; border-color: #e3e9e3; font-size: 12px; }
  .shell-chat :global(.messages article) { max-width: 86%; margin: 18px 0; border: 0; padding: 12px 16px; background: transparent; }
  .shell-chat :global(.messages article.user) { margin-left: auto; background: #eaf0e9; border-radius: 16px 16px 4px 16px; }
  .shell-chat :global(form) { flex: none; margin-top: 14px; border-radius: 16px; border-color: #dbe4db; box-shadow: 0 4px 18px #2c443b0b; }
  .shell-chat :global(form textarea) { min-height: 52px; }
  .shell-chat :global(.composer-footer) { padding: 0 4px 2px; }
  .cards {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin: 16px 0;
  }
  .cards button {
    padding: 20px;
    background: #fff;
    border: 1px solid #dce5dc;
    border-radius: 10px;
    cursor: pointer;
  }
  @media (max-width: 760px) {
    .shell {
      grid-template-columns: 184px minmax(0, 1fr);
    }
    .shell-page {
      padding: 26px 24px;
    }
    aside {
      padding: 15px 8px;
    }
  }
  @media (max-width: 560px) {
    .shell { grid-template-columns: 60px minmax(0, 1fr); }
    aside { padding: 16px 7px; }
    .brand { justify-content: center; padding: 0 0 22px; }
    .brand-copy, .nav-heading, nav button span, .shell-status { display: none; }
    nav button { justify-content: center; padding: 10px; }
    .shell-page { padding: 22px 16px; }
    .shell-chat { padding: 12px 14px 16px; }
    .shell-topbar { padding: 7px 10px; gap: 3px; }
    .shell-topbar button { padding: 8px 6px; }
  }
  @media (max-height: 560px) {
    aside { padding-top: 14px; }
    .brand { padding-bottom: 14px; }
    .nav-heading { padding-bottom: 5px; }
    nav button { min-height: 34px; padding-top: 7px; padding-bottom: 7px; }
    .shell-status { display: none; }
  }
</style>
