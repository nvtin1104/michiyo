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
    background: #fdf6e7;
    border-bottom: 1px solid #f3e3be;
  }
  .reminder {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
  }
  .reminder button {
    padding: 3px 8px;
    font-size: 11px;
    border-radius: 5px;
    border: 1px solid #d7c598;
    background: #fff;
    cursor: pointer;
  }
</style>
