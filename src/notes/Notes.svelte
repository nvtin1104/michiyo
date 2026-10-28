<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { call, desktop } from '../lib/api';
  import Suggestions from '../ai/Suggestions.svelte';
  type Note = {
    id: string;
    rev: number;
    title: string;
    body: string;
    tags: string[];
    pinned: boolean;
    updated_at: number;
    deleted_at: number | null;
    device_id: string;
  };
  let notes: Note[] = [];
  let selected: Note | null = null;
  let title = '',
    body = '',
    tags = '',
    query = '',
    error = '';
  let summary = '',
    summarizing = false;
  async function summarize() {
    if (!selected || summarizing) return;
    summarizing = true;
    error = summary = '';
    try {
      summary = await call<string>('ai_summarize', {
        text: `${selected.title}\n\n${selected.body}`,
      });
    } catch (e) {
      error = String(e);
    } finally {
      summarizing = false;
    }
  }
  function insertSummary() {
    if (!selected) return;
    const quoted = summary
      .trim()
      .split('\n')
      .map((line) => `> ${line}`)
      .join('\n');
    edit(selected);
    body = `> **Tóm tắt**\n${quoted}\n\n${body}`;
    summary = '';
  }
  let pinned = false,
    saving = false,
    editing = false;
  async function refresh() {
    if (!desktop) return;
    try {
      notes = await call<Note[]>('notes_list', { query });
    } catch (e) {
      error = String(e);
    }
  }
  function edit(note: Note | null) {
    selected = note;
    title = note?.title || '';
    body = note?.body || '';
    tags = note?.tags.join(', ') || '';
    pinned = note?.pinned || false;
    editing = true;
    error = '';
  }
  async function saveNote() {
    if (!title.trim() || saving) return;
    saving = true;
    error = '';
    try {
      selected = await call<Note>('notes_save', {
        input: {
          id: selected?.id ?? null,
          rev: selected?.rev ?? null,
          title,
          body,
          tags: tags
            .split(',')
            .map((tag) => tag.trim())
            .filter(Boolean),
          pinned,
        },
      });
      editing = false;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
  async function deleteNote() {
    if (!selected || saving) return;
    saving = true;
    error = '';
    try {
      await call('notes_delete', { id: selected.id, rev: selected.rev });
      selected = null;
      editing = false;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
  onMount(() => {
    let off: (() => void) | undefined;
    let disposed = false;
    refresh();
    if (desktop)
      listen('notes-changed', refresh).then((f) =>
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
    <small>YOUR PERSONAL SPACE</small>
    <h1>Ghi chú <span>✦</span></h1>
  </div>
  <button onclick={() => edit(null)}>+ Ghi chú mới</button>
</header>
{#if !desktop}<p class="muted">Mở ứng dụng desktop để sử dụng ghi chú.</p>{/if}
<div class="notes-layout">
  <section class="notes-list" aria-label="Danh sách ghi chú">
    <form
      onsubmit={(e) => {
        e.preventDefault();
        refresh();
      }}
    >
      <input
        aria-label="Tìm ghi chú"
        bind:value={query}
        placeholder="Tìm ghi chú…"
      /><button>Tìm</button>
    </form>
    {#each notes as note}<button
        class:chosen={selected?.id === note.id}
        onclick={() => {
          selected = note;
          editing = false;
          error = '';
        }}>{note.pinned ? '📌 ' : ''}{note.title}</button
      >{/each}
    {#if !notes.length}<p class="muted">Chưa có ghi chú phù hợp.</p>{/if}
  </section>
  <section class="note-detail">
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if editing}
      <label
        >Tiêu đề<input
          aria-label="Tiêu đề"
          bind:value={title}
          maxlength="200"
        /></label
      >
      <label
        >Nội dung Markdown<textarea
          aria-label="Nội dung Markdown"
          bind:value={body}
          rows="12"></textarea></label
      >
      <label
        >Thẻ (cách nhau bằng dấu phẩy)<input
          aria-label="Thẻ"
          bind:value={tags}
        /></label
      >
      <label><input type="checkbox" bind:checked={pinned} /> Ghim ghi chú</label
      >
      <div class="actions">
        <button
          class="primary"
          disabled={saving || !title.trim()}
          onclick={saveNote}>Lưu</button
        ><button disabled={saving} onclick={() => (editing = false)}>Hủy</button
        >
      </div>
    {:else if selected}
      <h2>{selected.title}</h2>
      <p class="muted">{selected.tags.join(' · ')}</p>
      <div class="markdown">
        {@html DOMPurify.sanitize(
          marked.parse(selected.body, { async: false }) as string,
          { FORBID_TAGS: ['a', 'img', 'video', 'audio', 'iframe'] },
        )}
      </div>
      <div class="actions">
        <button onclick={() => edit(selected)}>Sửa</button><button
          disabled={saving}
          onclick={deleteNote}>Xóa</button
        ><button disabled={summarizing} onclick={summarize}
          >{summarizing ? 'Đang tóm tắt…' : 'Tóm tắt bằng AI'}</button
        >
        {#key selected.id}<Suggestions
            text={`${selected.title}\n\n${selected.body}`}
          />{/key}
      </div>
      {#if summary}<aside class="summary" aria-label="Bản tóm tắt">
          <pre>{summary}</pre>
          <button onclick={insertSummary}>Chèn vào đầu ghi chú</button><button
            onclick={() => (summary = '')}>Bỏ</button
          >
        </aside>{/if}
    {:else}<p>Chọn một ghi chú hoặc tạo ghi chú mới.</p>{/if}
  </section>
</div>

<style>
  .notes-layout {
    display: grid;
    grid-template-columns: minmax(160px, 240px) minmax(0, 1fr);
    gap: 20px;
    min-height: 0;
  }
  .notes-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow: auto;
  }
  .notes-list form {
    display: flex;
    gap: 4px;
  }
  .notes-list input {
    min-width: 0;
    width: 100%;
  }
  .notes-list > button {
    text-align: left;
  }
  .notes-list > button.chosen {
    background: #dbe8d5;
  }
  .note-detail {
    min-width: 0;
    overflow: auto;
  }
  .note-detail label {
    display: block;
    margin-bottom: 12px;
  }
  .note-detail input:not([type='checkbox']) {
    display: block;
    width: 100%;
  }
  .summary {
    margin-top: 12px;
    padding: 10px;
    background: #f3e9cf;
    border: 1px solid #e0d1a6;
    border-radius: 8px;
  }
  .summary pre {
    white-space: pre-wrap;
    margin: 0 0 8px;
    font: inherit;
  }
  .note-detail textarea {
    min-height: 160px;
  }
  @media (max-width: 700px) {
    .notes-layout {
      grid-template-columns: 1fr;
    }
    .notes-list {
      max-height: 180px;
    }
  }
</style>
