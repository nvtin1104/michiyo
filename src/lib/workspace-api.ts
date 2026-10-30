import type { Note } from '@michiyo/shared';
import { call, desktop } from './api';

export type DueReminder = {
  reminder_id: string;
  title: string;
  kind: string;
  occurrence_start: number;
};

export type TodayAgendaItem = {
  id: string;
  title: string;
  kind: string;
  done: boolean;
  occurrence_start: number;
};

export type TodayHabitItem = {
  id: string;
  title: string;
  streak: number;
  done_today: boolean;
};

export type SyncStatusInfo = {
  enabled: boolean;
  pending: number;
  last_sync: number | null;
  last_error: string | null;
  syncing: boolean;
};

export interface WorkspaceAdapter {
  isAvailable(): boolean;
  notesList(query?: string): Promise<Note[]>;
  eventsRange(
    from: number,
    until: number,
    tzOffset: number,
  ): Promise<TodayAgendaItem[]>;
  habitsList(today: string): Promise<TodayHabitItem[]>;
  habitCheckIn(id: string, date: string, done: boolean): Promise<void>;
  remindersPending(): Promise<DueReminder[]>;
  reminderSnooze(id: string, minutes: number): Promise<void>;
  reminderDismiss(id: string): Promise<void>;
  setTimezone(offset: number): Promise<void>;
  syncStatus(): Promise<SyncStatusInfo>;
}

const tauriAdapter: WorkspaceAdapter = {
  isAvailable: () => desktop,
  notesList: (query = '') => call<Note[]>('notes_list', { query }),
  eventsRange: (from, until, tzOffset) =>
    call<TodayAgendaItem[]>('events_range', { from, until, tzOffset }),
  habitsList: (today) => call<TodayHabitItem[]>('habits_list', { today }),
  habitCheckIn: (id, date, done) => call('habits_check_in', { id, date, done }),
  remindersPending: () => call<DueReminder[]>('reminders_pending'),
  reminderSnooze: (id, minutes) => call('reminders_snooze', { id, minutes }),
  reminderDismiss: (id) => call('reminders_dismiss', { id }),
  setTimezone: (offset) => call('set_timezone', { offset }),
  syncStatus: () => call<SyncStatusInfo>('sync_status'),
};

let activeAdapter: WorkspaceAdapter = tauriAdapter;

export function setWorkspaceAdapter(adapter: WorkspaceAdapter): void {
  activeAdapter = adapter;
}

export function workspace(): WorkspaceAdapter {
  return activeAdapter;
}
