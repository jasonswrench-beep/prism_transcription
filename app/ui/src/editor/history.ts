// Unlimited undo (T4). Each committed change pushes the transcript's turn array; unchanged turns are shared between
// entries, so a long history of a long transcript costs little memory.

export class History<T> {
  private past: T[] = [];
  private future: T[] = [];

  constructor(private present: T) {}

  get current(): T {
    return this.present;
  }

  get canUndo(): boolean {
    return this.past.length > 0;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  push(next: T): T {
    if (next === this.present) return this.present;
    this.past.push(this.present);
    this.present = next;
    this.future = [];
    return next;
  }

  undo(): T {
    const p = this.past.pop();
    if (p === undefined) return this.present;
    this.future.push(this.present);
    this.present = p;
    return p;
  }

  redo(): T {
    const f = this.future.pop();
    if (f === undefined) return this.present;
    this.past.push(this.present);
    this.present = f;
    return f;
  }

  /** Start again from a value, forgetting the history (a different transcript was opened). */
  reset(value: T): void {
    this.past = [];
    this.future = [];
    this.present = value;
  }
}
