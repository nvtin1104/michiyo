<script lang="ts">
  import type { DueReminder } from '../lib/workspace-api';

  export let due: DueReminder[] = [];
  export let error = '';
  export let onAnswer: (item: DueReminder, minutes?: number) => void;
</script>

{#if due.length}
  <div class="reminders" role="region" aria-label="Nhắc nhở">
    {#each due as item (item.reminder_id)}
      <div class="reminder">
        <span>
          ⏰ <strong>{item.title}</strong>
          <small>{new Date(item.occurrence_start).toLocaleTimeString()}</small>
        </span>
        <button onclick={() => onAnswer(item, 10)}>Hoãn 10 phút</button>
        <button onclick={() => onAnswer(item)}>Đã rõ</button>
      </div>
    {/each}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
{/if}

<style>
  .reminders {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 24px;
    background: var(--bg-glass);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    border-bottom: 1px solid var(--border-glass);
  }
  .reminder {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--text-primary);
  }
  .reminder button {
    padding: 3px 8px;
    font-size: 11px;
    border-radius: 6px;
    border: 1px solid var(--border-glass);
    background: var(--bg-glass-hover);
    color: var(--text-primary);
    cursor: pointer;
  }
</style>
