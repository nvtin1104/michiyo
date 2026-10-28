<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { call, desktop } from '../lib/api';
  type Item = {
    id: string;
    rev: number;
    title: string;
    kind: 'event' | 'todo';
    starts_at: number;
    ends_at: number | null;
    rrule: string | null;
    done: boolean;
    occurrence_start: number;
    occurrence_end: number | null;
    remind_before: number | null;
  };
  type View = 'month' | 'week' | 'day';
  const weekdayNames = ['T2', 'T3', 'T4', 'T5', 'T6', 'T7', 'CN'];
  const repeats: [string, string][] = [
    ['', 'Không lặp'],
    ['FREQ=DAILY', 'Mỗi ngày'],
    ['FREQ=WEEKLY', 'Mỗi tuần'],
    ['FREQ=MONTHLY', 'Mỗi tháng'],
    ['FREQ=YEARLY', 'Mỗi năm'],
  ];
  const reminders: [string, string][] = [
    ['', 'Không nhắc'],
    ['0', 'Đúng giờ'],
    ['5', 'Trước 5 phút'],
    ['15', 'Trước 15 phút'],
    ['60', 'Trước 1 giờ'],
    ['1440', 'Trước 1 ngày'],
  ];
  const pad = (n: number) => String(n).padStart(2, '0');
  const dateKey = (d: Date) =>
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const timeKey = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const startOfDay = (d: Date) =>
    new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const addDays = (d: Date, n: number) =>
    new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);

  let view: View = 'month';
  let focus = startOfDay(new Date());
  let items: Item[] = [];
  let error = '';
  let editing: Item | null = null;
  let open = false;
  let saving = false;
  let title = '',
    kind: 'event' | 'todo' = 'event',
    date = dateKey(focus),
    start = '09:00',
    end = '',
    repeat = '',
    customRule = '',
    remind = '';

  function range(v: View, f: Date): [Date, Date] {
    if (v === 'day') return [f, addDays(f, 1)];
    if (v === 'week') {
      const monday = addDays(f, -((f.getDay() + 6) % 7));
      return [monday, addDays(monday, 7)];
    }
    const first = new Date(f.getFullYear(), f.getMonth(), 1);
    const gridStart = addDays(first, -((first.getDay() + 6) % 7));
    return [gridStart, addDays(gridStart, 42)];
  }
  $: [from, until] = range(view, focus);
  $: days = Array.from(
    { length: Math.round((until.getTime() - from.getTime()) / 864e5) },
    (_, i) => addDays(from, i),
  );
  $: byDay = groupByDay(items);
  $: heading =
    view === 'day'
      ? focus.toLocaleDateString()
      : view === 'week'
        ? `${from.toLocaleDateString()} – ${addDays(until, -1).toLocaleDateString()}`
        : `Tháng ${focus.getMonth() + 1}/${focus.getFullYear()}`;

  function groupByDay(list: Item[]) {
    const map = new Map<string, Item[]>();
    for (const item of list) {
      const key = dateKey(new Date(item.occurrence_start));
      map.set(key, [...(map.get(key) ?? []), item]);
    }
    return map;
  }
  async function refresh() {
    if (!desktop) return;
    try {
      call('set_timezone', { offset: -new Date().getTimezoneOffset() }).catch(() => {});
      items = await call<Item[]>('events_range', {
        from: from.getTime(),
        until: until.getTime(),
        tzOffset: -new Date().getTimezoneOffset(),
      });
    } catch (e) {
      error = String(e);
    }
  }
  function move(step: number) {
    if (view === 'month')
      focus = new Date(focus.getFullYear(), focus.getMonth() + step, 1);
    else focus = addDays(focus, step * (view === 'week' ? 7 : 1));
  }
  function edit(item: Item | null, on?: Date) {
    editing = item;
    open = true;
    error = '';
    title = item?.title ?? '';
    kind = item?.kind ?? 'event';
    const s = item ? new Date(item.starts_at) : (on ?? focus);
    date = dateKey(s);
    start = item ? timeKey(s) : '09:00';
    end = item?.ends_at ? timeKey(new Date(item.ends_at)) : '';
    const rule = item?.rrule ?? '';
    repeat = repeats.some(([v]) => v === rule) ? rule : 'custom';
    customRule = repeat === 'custom' ? rule : '';
    remind = item?.remind_before == null ? '' : String(item.remind_before);
  }
  async function run(action: () => Promise<unknown>) {
    if (saving) return;
    saving = true;
    error = '';
    try {
      await action();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
  const stamp = (t: string) => new Date(`${date}T${t}`).getTime();
  const saveItem = () =>
    run(async () => {
      const startsAt = stamp(start || '00:00');
      let endsAt: number | null = null;
      if (kind === 'event' && end) {
        endsAt = stamp(end);
        if (endsAt < startsAt) endsAt += 864e5; // ends after midnight
      }
      await call('events_save', {
        input: {
          id: editing?.id ?? null,
          rev: editing?.rev ?? null,
          title,
          kind,
          starts_at: startsAt,
          ends_at: endsAt,
          rrule: (repeat === 'custom' ? customRule.trim() : repeat) || null,
          done: editing?.done ?? false,
          remind_before: remind === '' ? null : Number(remind),
        },
      });
      open = false;
      await refresh();
    });
  const remove = () =>
    run(async () => {
      if (!editing) return;
      await call('events_delete', { id: editing.id, rev: editing.rev });
      open = false;
      await refresh();
    });
  const toggleDone = (item: Item) =>
    run(async () => {
      await call('events_set_done', {
        id: item.id,
        rev: item.rev,
        done: !item.done,
      });
      await refresh();
    });
  $: from, until, refresh();
  onMount(() => {
    let off: (() => void) | undefined;
    let disposed = false;
    if (desktop)
      listen('events-changed', refresh).then((f) =>
        disposed ? f() : (off = f),
      );
    return () => {
      disposed = true;
      off?.();
    };
  });
</script>

<header>
  <div>
    <small>PLAN YOUR DAYS</small>
    <h1>Lịch <span>✦</span></h1>
  </div>
  <button onclick={() => edit(null)}>+ Mới</button>
</header>
{#if !desktop}<p class="muted">Mở ứng dụng desktop để sử dụng lịch.</p>{/if}
<div class="bar">
  <button aria-label="Trước" onclick={() => move(-1)}>‹</button>
  <button onclick={() => (focus = startOfDay(new Date()))}>Hôm nay</button>
  <button aria-label="Sau" onclick={() => move(1)}>›</button>
  <strong>{heading}</strong>
  <span class="views" role="group" aria-label="Kiểu xem">
    {#each [['month', 'Tháng'], ['week', 'Tuần'], ['day', 'Ngày']] as [id, label]}
      <button
        class:active={view === id}
        aria-pressed={view === id}
        onclick={() => (view = id as View)}>{label}</button
      >{/each}
  </span>
</div>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if open}
  <section class="form" aria-label="Sự kiện">
    <label
      >Tiêu đề<input
        aria-label="Tiêu đề"
        bind:value={title}
        maxlength="200"
      /></label
    >
    <label
      >Loại<select aria-label="Loại" bind:value={kind}
        ><option value="event">Sự kiện</option><option value="todo"
          >Việc cần làm</option
        ></select
      ></label
    >
    <label>{kind === 'todo' ? 'Hạn' : 'Ngày'}<input type="date" aria-label="Ngày" bind:value={date} /></label>
    <label
      >{kind === 'todo' ? 'Giờ' : 'Bắt đầu'}<input
        type="time"
        aria-label="Giờ bắt đầu"
        bind:value={start}
      /></label
    >
    {#if kind === 'event'}<label
        >Kết thúc<input
          type="time"
          aria-label="Giờ kết thúc"
          bind:value={end}
        /></label
      >{/if}
    <label
      >Lặp lại<select aria-label="Lặp lại" bind:value={repeat}
        >{#each repeats as [value, label]}<option {value}>{label}</option
          >{/each}<option value="custom">Tùy chỉnh (RRULE)</option></select
      ></label
    >
    <label
      >Nhắc nhở<select aria-label="Nhắc nhở" bind:value={remind}
        >{#each reminders as [value, label]}<option {value}>{label}</option
          >{/each}{#if remind !== '' && !reminders.some(([v]) => v === remind)}<option
            value={remind}>Trước {remind} phút</option
          >{/if}</select
      ></label
    >
    {#if repeat === 'custom'}<label
        >RRULE<input
          aria-label="RRULE"
          bind:value={customRule}
          placeholder="FREQ=WEEKLY;BYDAY=MO,WE"
        /></label
      >{/if}
    <div class="actions">
      <button
        class="primary"
        disabled={saving || !title.trim() || !date}
        onclick={saveItem}>Lưu</button
      ><button disabled={saving} onclick={() => (open = false)}>Hủy</button
      >{#if editing}<button disabled={saving} onclick={remove}>Xóa</button
        >{/if}
    </div>
  </section>
{/if}

{#if view === 'day'}
  <ul class="agenda">
    {#each byDay.get(dateKey(focus)) ?? [] as item}
      <li>
        {@render row(item)}
      </li>
    {:else}<li class="muted">Không có gì trong ngày này.</li>{/each}
  </ul>
{:else}
  <div class="grid" class:week={view === 'week'}>
    {#each weekdayNames as name}<b>{name}</b>{/each}
    {#each days as day}
      <div
        class="cell"
        class:other={view === 'month' && day.getMonth() !== focus.getMonth()}
        class:today={dateKey(day) === dateKey(new Date())}
      >
        <button
          class="num"
          aria-label={`Thêm vào ${day.toLocaleDateString()}`}
          onclick={() => edit(null, day)}>{day.getDate()}</button
        >
        {#each byDay.get(dateKey(day)) ?? [] as item}
          {@render row(item)}
        {/each}
      </div>
    {/each}
  </div>
{/if}

{#snippet row(item: Item)}
  <span class="item" class:done={item.done} class:todo={item.kind === 'todo'}>
    {#if item.kind === 'todo'}<input
        type="checkbox"
        aria-label={`Hoàn thành ${item.title}`}
        checked={item.done}
        disabled={saving}
        onchange={() => toggleDone(item)}
      />{/if}
    <button class="link" onclick={() => edit(item)}
      >{timeKey(new Date(item.occurrence_start))} {item.title}{item.rrule
        ? ' ↻'
        : ''}</button
    >
  </span>
{/snippet}

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 12px 0;
  }
  .views {
    margin-left: auto;
    display: inline-flex;
    gap: 4px;
  }
  .views .active {
    background: #cdddc9;
    font-weight: 600;
  }
  .form {
    background: #e9eee3;
    border: 1px solid #d4dfd0;
    border-radius: 12px;
    padding: 14px;
    margin-bottom: 12px;
  }
  .form label {
    display: block;
    margin-bottom: 10px;
  }
  .form input:not([type='checkbox']),
  .form select {
    display: block;
    width: 100%;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 4px;
  }
  .grid b {
    text-align: center;
    color: #667a68;
    font-size: 12px;
  }
  .cell {
    min-height: 84px;
    padding: 4px;
    background: #f1f4ec;
    border: 1px solid #dde6d9;
    border-radius: 8px;
    overflow: hidden;
  }
  .grid.week .cell {
    min-height: 240px;
  }
  .cell.other {
    opacity: 0.5;
  }
  .cell.today {
    border-color: #3b5642;
    background: #e3eddc;
  }
  .num {
    border: 0;
    background: transparent;
    padding: 0 2px;
    font-size: 12px;
    cursor: pointer;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 12px;
  }
  .item.done .link {
    text-decoration: line-through;
    opacity: 0.6;
  }
  .link {
    border: 0;
    background: transparent;
    padding: 1px 2px;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .agenda {
    list-style: none;
    padding: 0;
  }
  .agenda li {
    padding: 8px 0;
    font-size: 15px;
  }
  @media (max-width: 700px) {
    .cell {
      min-height: 56px;
    }
  }
</style>
