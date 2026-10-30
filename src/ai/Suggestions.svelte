<script lang="ts">
  import { call, desktop } from '../lib/api';
  export let text: string;
  type Suggestion = {
    kind: 'note' | 'event' | 'todo' | 'habit';
    title: string;
    body: string | null;
    when: string | null;
    frequency: string | null;
  };
  const labels = {
    note: 'Ghi chú',
    event: 'Sự kiện',
    todo: 'Việc cần làm',
    habit: 'Thói quen',
  };
  let items: Suggestion[] = [];
  let loaded = false;
  let busy = false;
  let error = '';
  let added = '';
  const pad = (n: number) => String(n).padStart(2, '0');
  function nowLocal() {
    const d = new Date();
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }
  async function suggest() {
    if (busy || !desktop) return;
    busy = true;
    error = added = '';
    try {
      items = await call<Suggestion[]>('ai_suggest_actions', {
        text,
        nowLocal: nowLocal(),
      });
      loaded = true;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  // Only a user click reaches these; the model never writes anything itself.
  async function accept(item: Suggestion) {
    if (busy) return;
    busy = true;
    error = added = '';
    try {
      if (item.kind === 'note')
        await call('notes_save', {
          input: {
            id: null,
            rev: null,
            title: item.title,
            body: item.body ?? '',
            tags: [],
            pinned: false,
          },
        });
      else if (item.kind === 'habit')
        await call('habits_save', {
          input: {
            id: null,
            rev: null,
            title: item.title,
            frequency: item.frequency ?? 'daily',
          },
        });
      else
        await call('events_save', {
          input: {
            id: null,
            rev: null,
            title: item.title,
            kind: item.kind,
            starts_at: new Date(item.when ?? '').getTime(),
            ends_at: null,
            rrule: null,
            done: false,
            remind_before: null,
          },
        });
      added = `Đã thêm: ${item.title}`;
      items = items.filter((i) => i !== item);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<button disabled={busy || !text.trim()} onclick={suggest}
  >{busy ? 'Đang nghĩ…' : 'Gợi ý hành động'}</button
>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if added}<p class="muted" role="status">{added}</p>{/if}
{#if loaded && !items.length && !added}<p class="muted">
    Không có gợi ý nào.
  </p>{/if}
<ul class="suggestions" aria-label="Gợi ý hành động">
  {#each items as item}
    <li>
      <span
        ><b>{labels[item.kind]}</b>
        {item.title}{#if item.when}
          <small>{new Date(item.when).toLocaleString()}</small
          >{/if}{#if item.frequency}
          <small>{item.frequency}</small>{/if}</span
      >
      <button
        aria-label={`Thêm ${item.title}`}
        disabled={busy}
        onclick={() => accept(item)}>Thêm</button
      ><button
        aria-label={`Bỏ qua ${item.title}`}
        onclick={() => (items = items.filter((i) => i !== item))}>Bỏ qua</button
      >
    </li>
  {/each}
</ul>

<style>
  .suggestions {
    list-style: none;
    padding: 0;
    margin: 8px 0;
  }
  .suggestions li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px;
    margin-bottom: 6px;
    background: #f3e9cf;
    border: 1px solid #e0d1a6;
    border-radius: 8px;
  }
  .suggestions span {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
