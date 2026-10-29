export type State =
  | 'idle'
  | 'hover'
  | 'listening'
  | 'thinking'
  | 'talking'
  | 'happy'
  | 'error'
  | 'sleep';
export type Reaction =
  | 'greeting'
  | 'touched'
  | 'petting'
  | 'dragging'
  | 'carried'
  | 'settling'
  | 'waking';
export type Pose = State | Reaction | 'walk' | 'sit' | 'wave';
const states: State[] = [
  'idle',
  'hover',
  'listening',
  'thinking',
  'talking',
  'happy',
  'error',
  'sleep',
];
export const isState = (value: unknown): value is State =>
  states.includes(value as State);
export class MascotMachine {
  state: State = 'idle';
  reaction: Reaction | null = null;
  private reactionUntil = 0;
  private lastTouch = -Infinity;
  lastActivity = Date.now();
  get pose(): Pose {
    return this.reaction || this.state;
  }
  react(next: Reaction, now = Date.now()) {
    if (next === 'touched' && now - this.lastTouch < 500) return;
    if (next === 'touched') this.lastTouch = now;
    if (this.state === 'sleep') {
      this.state = 'idle';
      next = 'waking';
    }
    this.lastActivity = now;
    this.reaction = next;
    this.reactionUntil =
      now +
      (next === 'dragging' || next === 'carried'
        ? 15000
        : next === 'petting'
          ? 1800
          : 1100);
  }
  hover(inside: boolean) {
    if (['idle', 'hover'].includes(this.state))
      this.state = inside ? 'hover' : 'idle';
    return this.state;
  }
  transition(next: State) {
    this.state = next;
    if (next !== 'sleep') this.lastActivity = Date.now();
    return this.state;
  }
  tick(now: number, minutes: number) {
    if (this.state === 'listening' && now - this.lastActivity > 5000)
      this.state = 'idle';
    if (this.reaction && now >= this.reactionUntil) this.reaction = null;
    if (
      ['happy', 'error'].includes(this.state) &&
      now - this.lastActivity > 2200
    )
      this.state = 'idle';
    if (
      !this.reaction &&
      !['listening', 'thinking', 'talking'].includes(this.state) &&
      now - this.lastActivity >= minutes * 60000
    )
      this.state = 'sleep';
    return this.state;
  }
}
