import { z } from 'zod';
import { Activity, Conversation, Event, Habit, HabitLog, Message, Note, Reminder } from './records';

export const SyncRecord = z.discriminatedUnion('type', [
  z.object({ type: z.literal('note'), record: Note }),
  z.object({ type: z.literal('habit'), record: Habit }),
  z.object({ type: z.literal('habit_log'), record: HabitLog }),
  z.object({ type: z.literal('event'), record: Event }),
  z.object({ type: z.literal('reminder'), record: Reminder }),
  z.object({ type: z.literal('activity'), record: Activity }),
  z.object({ type: z.literal('conversation'), record: Conversation }),
  z.object({ type: z.literal('message'), record: Message }),
]);
export const PushRequest = z.object({ records: z.array(SyncRecord).max(500) });
export const PullResponse = z.object({ records: z.array(SyncRecord), cursor: z.string() });
export type SyncRecord = z.infer<typeof SyncRecord>;
export type PushRequest = z.infer<typeof PushRequest>;
export type PullResponse = z.infer<typeof PullResponse>;
export const SyncRef = z.object({ type: z.string(), id: z.string(), rev: z.number().int().nonnegative() });
// `accepted` records won or were already stored; `rejected` lost to a newer write and will arrive on the next pull.
export const PushResponse = z.object({ accepted: z.array(SyncRef), rejected: z.array(SyncRef) });
export type SyncRef = z.infer<typeof SyncRef>;
export type PushResponse = z.infer<typeof PushResponse>;
