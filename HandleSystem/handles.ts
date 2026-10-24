export type HandleState = "idle" | "active" | "released";
export type HandleOwner = string;
export interface HandleActions {
  onUse(): void;
  onRelease(): void;
}
export class Handle {
  readonly id = crypto.randomUUID();
  private stateValue: HandleState = "idle";
  constructor(
    readonly owner: HandleOwner,
    private actions: HandleActions,
    private unregister: () => void,
  ) {}
  get state() {
    return this.stateValue;
  }
  use() {
    if (this.stateValue !== "idle")
      throw new Error("This handle cannot be activated again");
    try {
      this.actions.onUse();
      this.stateValue = "active";
    } catch (error) {
      try {
        this.actions.onRelease();
      } finally {
        this.stateValue = "released";
        this.unregister();
      }
      throw error;
    }
  }
  release() {
    if (this.stateValue === "released") return;
    const wasActive = this.stateValue === "active";
    this.stateValue = "released";
    try {
      if (wasActive) this.actions.onRelease();
    } finally {
      this.unregister();
    }
  }
}
export class LockHandle extends Handle {
  constructor(
    owner: string,
    readonly resource: string,
    actions: HandleActions,
    unregister: () => void,
  ) {
    super(owner, actions, unregister);
  }
}
export class HandleSystem {
  private handles = new Map<string, Handle>();
  private locks = new Map<string, Set<string>>();
  private listeners = new Map<string, Set<() => void>>();
  getHandle(owner: string, actions: HandleActions): Handle {
    const handle = new Handle(owner, actions, () =>
      this.handles.delete(handle.id),
    );
    this.handles.set(handle.id, handle);
    return handle;
  }
  getLockHandle(owner: string, resource: string): LockHandle {
    if (!owner || !resource)
      throw new Error("A handle needs an owner and resource");
    const handle = new LockHandle(
      owner,
      resource,
      {
        onUse: () => {
          const ids = this.locks.get(resource) ?? new Set<string>();
          ids.add(handle.id);
          this.locks.set(resource, ids);
          this.notify(resource);
        },
        onRelease: () => {
          const ids = this.locks.get(resource);
          ids?.delete(handle.id);
          if (!ids?.size) this.locks.delete(resource);
          this.notify(resource);
        },
      },
      () => this.handles.delete(handle.id),
    );
    this.handles.set(handle.id, handle);
    return handle;
  }
  isLocked(resource: string) {
    return (this.locks.get(resource)?.size ?? 0) > 0;
  }
  subscribe(resource: string, listener: () => void) {
    const listeners = this.listeners.get(resource) ?? new Set();
    listeners.add(listener);
    this.listeners.set(resource, listeners);
    return () => {
      listeners.delete(listener);
      if (!listeners.size) this.listeners.delete(resource);
    };
  }
  private notify(resource: string) {
    for (const listener of this.listeners.get(resource) ?? []) listener();
  }
  private releaseHandles(handles: Handle[]) {
    const failures: unknown[] = [];
    for (const handle of handles) {
      try {
        handle.release();
      } catch (error) {
        failures.push(error);
      }
    }
    if (failures.length)
      throw new AggregateError(failures, "Handle cleanup failed");
  }
  releaseOwner(owner: string) {
    this.releaseHandles(
      [...this.handles.values()].filter((handle) => handle.owner === owner),
    );
  }
  releaseAll() {
    this.releaseHandles([...this.handles.values()]);
  }
  async run<T>(
    owner: string,
    resource: string,
    operation: () => Promise<T>,
  ): Promise<T> {
    if (this.isLocked(resource)) throw new Error("این عملیات در حال انجام است");
    const lock = this.getLockHandle(owner, resource);
    lock.use();
    try {
      return await operation();
    } finally {
      lock.release();
    }
  }
}
