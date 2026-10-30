<script lang="ts">
  import type { Note } from '@michiyo/shared';
  import type { TodayAgendaItem, TodayHabitItem } from '../lib/workspace-api';

  export let agenda: TodayAgendaItem[] = [];
  export let habitsToDo: TodayHabitItem[] = [];
  export let pinnedNotes: Note[] = [];
  export let homeError = '';
  export let onNavigate: (route: string) => void;
  export let onCheckIn: (habit: TodayHabitItem) => void;
</script>

<section class="shell-page">
  <header>
    <div>
      <small>CHÀO MỪNG TRỞ LẠI</small>
      <h1>Một ngày cùng Michiyo</h1>
    </div>
  </header>
  <p>Quản lý ghi chú, lịch trình, thói quen và hoạt động cùng trợ lý của bạn.</p>
  <div class="cards">
    <button onclick={() => onNavigate('chat')}>✦ Bắt đầu trò chuyện</button>
    <button onclick={() => onNavigate('notes')}>📝 Mở ghi chú</button>
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
      <button onclick={() => onNavigate('calendar')}>Mở lịch</button>
    </section>
    <section aria-label="Thói quen chưa check-in">
      <h2>Thói quen hôm nay</h2>
      {#each habitsToDo as habit}
        <p class="habit-row">
          <span>{habit.title} <small>🔥 {habit.streak}</small></span>
          <button
            aria-label={`Check-in ${habit.title}`}
            onclick={() => onCheckIn(habit)}>✓</button>
        </p>
      {:else}
        <p class="muted">Đã hoàn thành hết hoặc chưa có thói quen.</p>
      {/each}
      <button onclick={() => onNavigate('habits')}>Mở thói quen</button>
    </section>
    <section aria-label="Ghi chú ghim">
      <h2>Ghi chú ghim</h2>
      {#each pinnedNotes as note}
        <p>📌 {note.title}</p>
      {:else}
        <p class="muted">Chưa có ghi chú ghim.</p>
      {/each}
      <button onclick={() => onNavigate('notes')}>Mở ghi chú</button>
    </section>
  </div>
</section>

<style>
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
    .shell-page {
      padding: 20px 18px;
    }
  }
  @media (max-width: 520px) {
    .cards {
      flex-wrap: wrap;
    }
  }
</style>
