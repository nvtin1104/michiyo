<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { call, desktop } from '../lib/api';
  type Habit = {
    id: string;
    rev: number;
    title: string;
    frequency: string;
    streak: number;
    done_today: boolean;
    dates: string[];
  };
  const dayNames = ['CN', 'T2', 'T3', 'T4', 'T5', 'T6', 'T7'];
  let habits: Habit[] = [];
  let error = '';
  let editing: Habit | null = null;
  let creating = false;
  let title = '',
    kind: 'daily' | 'weekdays' | 'weekly' = 'daily',
    days: number[] = [1, 2, 3, 4, 5],
    times = 3,
    saving = false;

  function localDate(date = new Date()) {
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
  }
  // The last 12 weeks, oldest first, for the heatmap.
  function heat(done: string[]) {
    const set = new Set(done);
    const cells: { date: string; on: boolean }[] = [];
    const now = new Date();
    for (let i = 83; i >= 0; i--) {
      const day = localDate(
        new Date(now.getFullYear(), now.getMonth(), now.getDate() - i),
      );
      cells.push({ date: day, on: set.has(day) });
    }
    return cells;
  }
  function describe(frequency: string) {
    if (frequency === 'daily') return 'Mỗi ngày';
    if (frequency.startsWith('weekdays:'))
      return frequency
        .slice(9)
        .split(',')
        .map((d) => dayNames[Number(d)])
        .join(' · ');
    return `${frequency.slice(7)} lần mỗi tuần`;
  }
  function frequencyValue() {
    if (kind === 'daily') return 'daily';
    if (kind === 'weekly') return `weekly:${times}`;
    return `weekdays:${[...days].sort().join(',')}`;
  }
  function edit(habit: Habit | null) {
    editing = habit;
    creating = true;
    error = '';
    title = habit?.title || '';
    kind = 'daily';
    days = [1, 2, 3, 4, 5];
    times = 3;
    if (habit?.frequency.startsWith('weekdays:')) {
      kind = 'weekdays';
      days = habit.frequency.slice(9).split(',').map(Number);
    } else if (habit?.frequency.startsWith('weekly:')) {
      kind = 'weekly';
      times = Number(habit.frequency.slice(7));
    }
  }
  async function refresh() {
    if (!desktop) return;
    try {
      habits = await call<Habit[]>('habits_list', { today: localDate() });
    } catch (e) {
      error = String(e);
    }
  }
  async function run(action: () => Promise<unknown>) {
    if (saving) return;
    saving = true;
    error = '';
    try {
      await action();
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
  const saveHabit = () =>
    run(async () => {
      await call('habits_save', {
        input: {
          id: editing?.id ?? null,
          rev: editing?.rev ?? null,
          title,
          frequency: frequencyValue(),
        },
      });
      creating = false;
    });
  const removeHabit = (habit: Habit) =>
    run(() => call('habits_delete', { id: habit.id, rev: habit.rev }));
  const toggle = (habit: Habit) =>
    run(() =>
      call('habits_check_in', {
        id: habit.id,
        date: localDate(),
        done: !habit.done_today,
      }),
    );
  onMount(() => {
    let off: (() => void) | undefined;
    let disposed = false;
    refresh();
    if (desktop)
      listen('habits-changed', refresh).then((f) =>
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
    <small>BUILD YOUR ROUTINE</small>
    <h1>Thói quen <span>✦</span></h1>
  </div>
  <button onclick={() => edit(null)}>+ Thói quen mới</button>
</header>
{#if !desktop}<p class="muted">Mở ứng dụng desktop để sử dụng thói quen.</p>{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if creating}
  <section class="habit-form" aria-label="Thói quen">
    <label
      >Tên thói quen<input
        aria-label="Tên thói quen"
        bind:value={title}
        maxlength="100"
      /></label
    >
    <label
      >Tần suất<select aria-label="Tần suất" bind:value={kind}
        ><option value="daily">Mỗi ngày</option><option value="weekdays"
          >Các ngày trong tuần</option
        ><option value="weekly">Số lần mỗi tuần</option></select
      ></label
    >
    {#if kind === 'weekdays'}
      <div class="days" role="group" aria-label="Ngày trong tuần">
        {#each [1, 2, 3, 4, 5, 6, 0] as day}<label
            ><input type="checkbox" bind:group={days} value={day} />
            {dayNames[day]}</label
          >{/each}
      </div>
    {:else if kind === 'weekly'}
      <label
        >Số lần mỗi tuần<input
          aria-label="Số lần mỗi tuần"
          type="number"
          min="1"
          max="7"
          bind:value={times}
        /></label
      >
    {/if}
    <div class="actions">
      <button
        class="primary"
        disabled={saving ||
          !title.trim() ||
          (kind === 'weekdays' && !days.length)}
        onclick={saveHabit}>Lưu</button
      ><button disabled={saving} onclick={() => (creating = false)}>Hủy</button>
    </div>
  </section>
{/if}
<ul class="habit-list">
  {#each habits as habit (habit.id)}
    <li>
      <div class="top">
        <button
          class="check"
          class:on={habit.done_today}
          aria-pressed={habit.done_today}
          aria-label={`Check-in ${habit.title}`}
          disabled={saving}
          onclick={() => toggle(habit)}>{habit.done_today ? '✓' : ''}</button
        >
        <div class="meta">
          <strong>{habit.title}</strong>
          <span class="muted">{describe(habit.frequency)}</span>
        </div>
        <span class="streak" title="Chuỗi liên tiếp">🔥 {habit.streak}</span>
        <button onclick={() => edit(habit)}>Sửa</button><button
          disabled={saving}
          onclick={() => removeHabit(habit)}>Xóa</button
        >
      </div>
      <div class="heat" aria-hidden="true">
        {#each heat(habit.dates) as cell}<i
            class:on={cell.on}
            title={cell.date}
          ></i>{/each}
      </div>
    </li>
  {/each}
</ul>
{#if desktop && !habits.length}<p class="muted">
    Chưa có thói quen nào. Hãy tạo thói quen đầu tiên.
  </p>{/if}

<style>
  .habit-form,
  .habit-list li {
    background: #e9eee3;
    border: 1px solid #d4dfd0;
    border-radius: 12px;
    padding: 14px;
    margin-bottom: 12px;
  }
  .habit-form label {
    display: block;
    margin-bottom: 10px;
  }
  .habit-form input:not([type='checkbox']),
  .habit-form select {
    display: block;
    width: 100%;
  }
  .days {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-bottom: 10px;
  }
  .days label {
    display: inline-flex;
    gap: 4px;
    margin: 0;
  }
  .habit-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .meta {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .check {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: 2px solid #8aa58c;
    background: transparent;
    cursor: pointer;
  }
  .check.on {
    background: #3b5642;
    border-color: #3b5642;
    color: #fff;
  }
  .streak {
    font-variant-numeric: tabular-nums;
  }
  .heat {
    display: grid;
    grid-template-rows: repeat(7, 9px);
    grid-auto-flow: column;
    gap: 3px;
    margin-top: 12px;
    overflow: hidden;
  }
  .heat i {
    width: 9px;
    background: #d4dfd0;
    border-radius: 2px;
  }
  .heat i.on {
    background: #3b5642;
  }
</style>
