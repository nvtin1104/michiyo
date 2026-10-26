export type State =
  | 'idle'
  | 'hover'
  | 'listening'
  | 'thinking'
  | 'talking'
  | 'happy'
  | 'error'
  | 'sleep';
export class MascotMachine {
  state: State = 'idle';
  lastActivity = Date.now();
  transition(next: State) {
    this.state = next;
    if (next !== 'sleep') this.lastActivity = Date.now();
    return this.state;
  }
  tick(now: number, minutes: number) {
    if (this.state === 'happy' && now - this.lastActivity > 2200)
      this.state = 'idle';
    if (
      !['thinking', 'talking'].includes(this.state) &&
      now - this.lastActivity >= minutes * 60000
    )
      this.state = 'sleep';
    return this.state;
  }
}
