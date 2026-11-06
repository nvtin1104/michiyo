const CROCKFORD = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';

/** Generates a 26-char Crockford Base32 ULID string natively without external packages. */
export function newId(now = Date.now()): string {
  let time = Math.floor(now);
  let timeChars = '';
  for (let i = 0; i < 10; i++) {
    timeChars = CROCKFORD[time % 32] + timeChars;
    time = Math.floor(time / 32);
  }
  const randBytes = new Uint8Array(16);
  if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
    crypto.getRandomValues(randBytes);
  } else {
    for (let i = 0; i < 16; i++) randBytes[i] = Math.floor(Math.random() * 256);
  }
  let randChars = '';
  for (let i = 0; i < 16; i++) {
    randChars += CROCKFORD[randBytes[i] & 31];
  }
  return timeChars + randChars;
}

export type RecordMetadata = {
  id: string;
  updated_at: number;
  deleted_at: number | null;
  device_id: string;
  rev: number;
};

export type Note = RecordMetadata & {
  title: string;
  body: string;
  tags: string[];
  pinned: boolean;
};

export type Habit = RecordMetadata & {
  title: string;
  frequency: string;
};

export type Event = RecordMetadata & {
  title: string;
  kind: 'event' | 'todo';
  starts_at: number;
  ends_at: number | null;
  rrule: string | null;
  done: boolean;
};

export type Reminder = RecordMetadata & {
  event_id: string;
  minutes_before: number;
  fired_for: number | null;
  pending_start: number | null;
  snooze_until: number | null;
  snooze_start: number | null;
};

export type Activity = RecordMetadata & {
  kind: string;
  occurred_at: number;
  data: unknown;
};

export type ChatMessage = {
  role: 'user' | 'assistant';
  content: string;
  time?: string;
  animate?: boolean;
};

export type ConversationRecord = {
  id: string;
  title: string;
  messages: ChatMessage[];
};
