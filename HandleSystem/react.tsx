import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { HandleSystem } from "./handles";
const Context = createContext<HandleSystem | null>(null);
export function HandleProvider({ children }: { children: ReactNode }) {
  const [system] = useState(() => new HandleSystem());
  useEffect(() => () => system.releaseAll(), [system]);
  return <Context.Provider value={system}>{children}</Context.Provider>;
}
export function useHandles() {
  const system = useContext(Context);
  if (!system) throw new Error("HandleProvider is missing");
  return system;
}
export function useLockState(resource: string) {
  const system = useHandles();
  return useSyncExternalStore(
    useCallback(
      (listener) => system.subscribe(resource, listener),
      [system, resource],
    ),
    () => system.isLocked(resource),
    () => false,
  );
}
export function useHandleOwner() {
  const owner = useId();
  const system = useHandles();
  useEffect(() => () => system.releaseOwner(owner), [owner, system]);
  return { owner, system };
}
export function useLockedAction(resource: string) {
  const { owner, system } = useHandleOwner();
  const requests = useRef(new Set<AbortController>());
  useEffect(() => {
    const controllers = requests.current;
    return () => {
      for (const request of controllers) request.abort();
    };
  }, []);
  return useCallback(
    async <T,>(operation: (signal: AbortSignal) => Promise<T>) => {
      return system.run(owner, resource, async () => {
        const controller = new AbortController();
        requests.current.add(controller);
        const navigation = system.getLockHandle(owner, "navigation");
        navigation.use();
        try {
          return await operation(controller.signal);
        } finally {
          requests.current.delete(controller);
          navigation.release();
        }
      });
    },
    [owner, resource, system],
  );
}
