export interface PhaseActions<P extends string> {
  guard?(from: P, to: P): Promise<boolean> | boolean;
  enter?(phase: P): Promise<void> | void;
  exit?(phase: P): Promise<void> | void;
  cleanup?(): Promise<void> | void;
}
export type PhaseEvent<P> = {
  type:
    | "initialized"
    | "transitioned"
    | "completed"
    | "cancelled"
    | "reset"
    | "faulted";
  current: P | null;
};
export class BasePhasedController<P extends string> {
  private current: P | null = null;
  private history: P[] = [];
  private busy = false;
  private inactive = false;
  private listeners = new Set<(event: PhaseEvent<P>) => void>();
  constructor(
    private initial: P,
    private phases: Record<P, readonly P[]>,
    private actions: PhaseActions<P> = {},
  ) {
    if (
      !(initial in phases) ||
      Object.values<readonly P[]>(phases).some((targets) =>
        targets.some((target) => !(target in phases)),
      )
    )
      throw new Error("Invalid phase configuration");
  }
  getCurrentPhase() {
    return this.current;
  }
  subscribe(listener: (event: PhaseEvent<P>) => void) {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }
  private emit(type: PhaseEvent<P>["type"]) {
    for (const listener of this.listeners)
      listener({ type, current: this.current });
  }
  private async exclusive(operation: () => Promise<void>) {
    if (this.busy) throw new Error("Transition already running");
    this.busy = true;
    try {
      await operation();
    } finally {
      this.busy = false;
    }
  }
  async initialize() {
    await this.exclusive(async () => {
      if (this.current || this.inactive) throw new Error("Already initialized");
      try {
        await this.actions.enter?.(this.initial);
        this.current = this.initial;
        this.emit("initialized");
      } catch (error) {
        await this.actions.cleanup?.();
        throw error;
      }
    });
  }
  canTransition(to: P) {
    return (
      !this.busy &&
      !this.inactive &&
      this.current !== null &&
      this.phases[this.current].includes(to)
    );
  }
  async transitionTo(to: P) {
    await this.move(to, false);
  }
  async nextPhase() {
    const next = this.current && this.phases[this.current][0];
    if (!next) throw new Error("No next phase");
    await this.move(next, false);
  }
  async previousPhase() {
    const previous = this.history.at(-1);
    if (!previous) throw new Error("No previous phase");
    await this.move(previous, true);
  }
  private async move(to: P, previous: boolean) {
    if (!this.canTransition(to)) throw new Error("Invalid phase transition");
    await this.exclusive(async () => {
      const from = this.current as P;
      if (this.actions.guard && !(await this.actions.guard(from, to)))
        throw new Error("Transition rejected");
      try {
        await this.actions.exit?.(from);
      } catch (error) {
        this.inactive = true;
        await this.actions.cleanup?.();
        this.emit("faulted");
        throw error;
      }
      try {
        await this.actions.enter?.(to);
      } catch (error) {
        try {
          await this.actions.cleanup?.();
          await this.actions.enter?.(from);
        } catch {
          this.inactive = true;
          this.emit("faulted");
        }
        throw error;
      }
      this.current = to;
      if (previous) this.history.pop();
      else this.history.push(from);
      this.emit("transitioned");
      if (this.phases[to].length === 0) {
        this.inactive = true;
        await this.actions.cleanup?.();
        this.emit("completed");
      }
    });
  }
  async cancel() {
    await this.exclusive(async () => {
      if (this.inactive) return;
      this.inactive = true;
      await this.actions.cleanup?.();
      this.emit("cancelled");
    });
  }
  async reset() {
    await this.exclusive(async () => {
      this.inactive = true;
      await this.actions.cleanup?.();
      await this.actions.enter?.(this.initial);
      this.current = this.initial;
      this.history = [];
      this.inactive = false;
      this.emit("reset");
    });
  }
}
