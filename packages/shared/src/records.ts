import { z } from 'zod';

// Times are Unix milliseconds; deletions remain as tombstones for synchronization.
export const RecordMetadata = z.object({
  id: z.string().min(1),
  updated_at: z.number().int().nonnegative(),
  deleted_at: z.number().int().nonnegative().nullable(),
  device_id: z.string().min(1),
  rev: z.number().int().nonnegative(),
});
export const Note = RecordMetadata.extend({ title: z.string(), body: z.string(), tags: z.array(z.string()), pinned: z.boolean() });
export const Habit = RecordMetadata.extend({ title: z.string(), frequency: z.string() });
export const HabitLog = RecordMetadata.extend({ habit_id: z.string(), date: z.string() });
export const Event = RecordMetadata.extend({ title: z.string(), kind: z.enum(['event', 'todo']).default('event'), starts_at: z.number().int(), ends_at: z.number().int().nullable(), rrule: z.string().nullable(), done: z.boolean().default(false) });
export const Reminder = RecordMetadata.extend({
  event_id: z.string(),
  minutes_before: z.number().int().nonnegative(),
  fired_for: z.number().int().nullable(),
  pending_start: z.number().int().nullable(),
  snooze_until: z.number().int().nullable(),
  snooze_start: z.number().int().nullable(),
});
export const Activity = RecordMetadata.extend({ kind: z.string(), occurred_at: z.number().int(), data: z.unknown() });
export const Conversation = RecordMetadata.extend({ title: z.string() });
export const Message = RecordMetadata.extend({ conversation_id: z.string(), role: z.enum(['user', 'assistant']), content: z.string() });
export const TelegramBotConfig = z.object({
  enabled: z.boolean().default(true),
  allowed_chat_ids: z.array(z.string().regex(/^-?\d+$/)).max(100).default([]),
  system_prompt: z.string().trim().min(1).max(2000).default('Bạn là Michiyo, trợ lý thân thiện và ngắn gọn.'),
  model: z.string().trim().max(120).default(''),
  hourly_limit: z.number().int().min(1).max(500).default(20),
  allow_workspace_queries: z.boolean().default(true),
});
export const TelegramInboxItem = z.object({
  update_id: z.number().int().nonnegative(),
  status: z.enum(['received', 'processing', 'sending', 'sent', 'ignored']),
  received_at: z.number().int().nonnegative(),
  updated_at: z.number().int().nonnegative(),
  chat_id: z.string().nullable().optional(),
  text_preview: z.string().nullable().optional(),
  reply_preview: z.string().nullable().optional(),
  error_reason: z.string().nullable().optional(),
});
export type Note = z.infer<typeof Note>;
export type Habit = z.infer<typeof Habit>;
export type HabitLog = z.infer<typeof HabitLog>;
export type Event = z.infer<typeof Event>;
export type Reminder = z.infer<typeof Reminder>;
export type Activity = z.infer<typeof Activity>;
export type Conversation = z.infer<typeof Conversation>;
export type Message = z.infer<typeof Message>;
export type TelegramBotConfig = z.infer<typeof TelegramBotConfig>;
export type TelegramInboxItem = z.infer<typeof TelegramInboxItem>;
