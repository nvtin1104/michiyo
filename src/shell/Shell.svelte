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
  let route = 'home';
  let chatVisited = false;
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
    route = routes.some(([id]) => id === next) ? next : 'home';
    if (route === 'chat') chatVisited = true;
    if (route === 'translate') translateVisited = true;
    if (route === 'settings') settingsVisited = true;
    if (route === 'notes') notesVisited = true;
    if (route === 'habits') habitsVisited = true;
    if (route === 'calendar') calendarVisited = true;
    location.hash = `#/${route}`;
  }
  onMount(() => {
    const fromHash = () => navigate(location.hash.slice(2) || 'home');
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

<div class="shell">
  <aside>
    <div class="brand">✦ <span>Michiyo</span></div>
    <nav aria-label="Điều hướng chính">
      {#each routes as [id, label]}
        <button
          class:active={route === id}
          aria-current={route === id ? 'page' : undefined}
          onclick={() => navigate(id)}>{label}</button
        >
      {/each}
    </nav>
    <div class="shell-status">
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
      ><span>Đồng bộ: {syncText}</span>
    </div>
  </aside>
  <main class="shell-main">
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
    {#if chatVisited}<div class="shell-page" hidden={route !== 'chat'}>
        <Chat {config} />
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
    min-height: 100vh;
    display: grid;
    grid-template-columns: 210px minmax(0, 1fr);
    background: #faf9f2;
    color: #293c31;
  }
  aside {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    padding: 22px 14px;
    background: #e9eee3;
    border-right: 1px solid #d6dfd2;
  }
  .brand {
    font-family: Georgia, serif;
    font-size: 24px;
    padding: 0 10px 20px;
    color: #9b7745;
  }
  .brand span {
    color: #3b5642;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  nav button {
    text-align: left;
    border: 0;
    border-radius: 8px;
    padding: 10px 12px;
    background: transparent;
    color: #506456;
    cursor: pointer;
  }
  nav button.active {
    background: #cdddc9;
    color: #233d2a;
    font-weight: 600;
  }
  .shell-status {
    display: flex;
    flex-direction: column;
    gap: 5px;
    margin-top: auto;
    padding: 16px 10px 0;
    color: #667a68;
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  [hidden] {
    display: none !important;
  }
  .shell-main {
    min-width: 0;
    min-height: 100vh;
  }
  .shell-page {
    display: flex;
    flex-direction: column;
    height: 100vh;
    max-width: 1000px;
    padding: 32px;
    margin: auto;
    overflow: auto;
  }
  .home-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
  }
  .home-grid section {
    padding: 14px;
    background: #e9eee3;
    border: 1px solid #d4dfd0;
    border-radius: 12px;
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
    height: 100vh;
  }
  .cards {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin: 16px 0;
  }
  .cards button {
    padding: 20px;
    background: #e9eee3;
    border: 1px solid #d4dfd0;
    border-radius: 12px;
    cursor: pointer;
  }
  @media (max-width: 640px) {
    .shell {
      grid-template-columns: 145px minmax(0, 1fr);
    }
    .shell-page {
      padding: 18px;
    }
    aside {
      padding: 15px 8px;
    }
  }
</style>
