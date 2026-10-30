<script lang="ts">
  const habitLabels: Record<string, string> = {
    habit_checkin: 'Đã check-in thói quen',
    habit_uncheck: 'Đã bỏ check-in',
    habit_saved: 'Đã lưu thói quen',
    habit_deleted: 'Đã xóa thói quen',
    event_saved: 'Đã lưu sự kiện',
    event_deleted: 'Đã xóa sự kiện',
    todo_done: 'Đã hoàn thành việc',
    todo_reopened: 'Đã mở lại việc',
  };
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { call, desktop } from '../lib/api';
  type Activity = {
    id: string;
    kind: string;
    occurred_at: number;
    data: { title?: string };
  };
  let items: Activity[] = [],
    kind = '',
    from = '',
    through = '',
    error = '',
    working = false;
  const start = (date: string) =>
    date ? new Date(`${date}T00:00:00`).getTime() : null;
  function end(date: string) {
    if (!date) return null;
    const d = new Date(`${date}T00:00:00`);
    d.setDate(d.getDate() + 1);
    return d.getTime();
  }
  async function refresh() {
    if (!desktop) return;
    error = '';
    const begin = start(from),
      finish = end(through);
    if (begin !== null && finish !== null && begin >= finish) {
      error = 'Khoảng ngày không hợp lệ';
      return;
    }
    try {
      items = await call<Activity[]>('activity_list', {
        kind: kind || null,
        from: begin,
        until: finish,
      });
    } catch (e) {
      error = String(e);
    }
  }
  async function remove(id: string) {
    working = true;
    try {
      await call('activity_delete', { ids: [id] });
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      working = false;
    }
  }
  async function removeRange() {
    const begin = start(from),
      finish = end(through);
    if (begin === null || finish === null || begin >= finish) {
      error = 'Chọn khoảng ngày hợp lệ';
      return;
    }
    if (!confirm('Xóa toàn bộ hoạt động trong khoảng ngày đã chọn?')) return;
    working = true;
    try {
      await call('activity_delete_range', { from: begin, until: finish });
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      working = false;
    }
  }
  onMount(() => {
    let off: (() => void) | undefined,
      offNotes: (() => void) | undefined,
      disposed = false;
    refresh();
    if (desktop) {
      listen('activity-changed', refresh).then((f) =>
        disposed ? f() : (off = f),
      );
      listen('notes-changed', refresh).then((f) =>
        disposed ? f() : (offNotes = f),
      );
    }
    return () => {
      disposed = true;
      off?.();
      offNotes?.();
    };
  });
</script>

<header>
  <div>
    <small>YOUR TIMELINE</small>
    <h1>Lịch sử hoạt động <span>✦</span></h1>
  </div>
</header>
{#if !desktop}<p class="muted">
    Mở ứng dụng desktop để xem lịch sử hoạt động.
  </p>{/if}
<form
  class="filters"
  onsubmit={(event) => {
    event.preventDefault();
    refresh();
  }}
>
  <label
    >Loại<select bind:value={kind}
      ><option value="">Tất cả</option><option value="note_saved"
        >Lưu ghi chú</option
      ><option value="note_deleted">Xóa ghi chú</option><option value="chat"
        >Trò chuyện</option
      ><option value="translation">Dịch văn bản</option><option
        value="habit_checkin">Check-in thói quen</option
      ><option value="habit_uncheck">Bỏ check-in</option><option
        value="habit_saved">Lưu thói quen</option
      ><option value="habit_deleted">Xóa thói quen</option><option
        value="event_saved">Lưu sự kiện</option
      ><option value="event_deleted">Xóa sự kiện</option><option
        value="todo_done">Hoàn thành việc</option
      ></select
    ></label
  >
  <label>Từ ngày<input type="date" bind:value={from} /></label><label
    >Đến ngày<input type="date" bind:value={through} /></label
  >
  <button>Lọc</button><button
    type="button"
    disabled={working || !from || !through}
    onclick={removeRange}>Xóa khoảng ngày</button
  >
</form>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if !items.length}<p class="muted">Chưa có hoạt động phù hợp.</p>{/if}
{#if items.length === 1000}<p class="muted">
    Đang hiển thị 1.000 mục gần nhất; hãy thu hẹp khoảng ngày để xem các mục
    khác.
  </p>{/if}
<div class="timeline">
  {#each items as item}<article>
      <div>
        <small>{new Date(item.occurred_at).toLocaleString()}</small>
        <p>
          {item.kind === 'note_saved'
            ? 'Đã lưu ghi chú'
            : item.kind === 'note_deleted'
              ? 'Đã xóa ghi chú'
              : item.kind === 'chat'
                ? 'Đã trò chuyện'
                : item.kind === 'translation'
                  ? 'Đã dịch văn bản'
                  : (habitLabels[item.kind] ?? item.kind)}{item.data?.title
            ? ` · ${item.data.title}`
            : ''}
        </p>
      </div>
      <button
        disabled={working}
        aria-label="Xóa hoạt động"
        onclick={() => remove(item.id)}>×</button
      >
    </article>{/each}
</div>

<style>
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: end;
    margin-bottom: 18px;
  }
  .filters label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .timeline {
    overflow: auto;
  }
  .timeline article {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px;
    border-bottom: 1px solid #e2e8dd;
  }
  .timeline p {
    margin: 4px 0;
  }
</style>
