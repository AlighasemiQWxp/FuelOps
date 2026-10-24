import { useQuery } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { useLockedAction } from "../../HandleSystem/react";
export interface Entity<T> {
  id: string;
  org_id: string;
  station_id: string;
  version: number;
  created_at: number;
  updated_at: number;
  data: T;
}
export interface Page<T> {
  items: T[];
  total: number;
}
export interface Actor {
  user_id: string;
  name: string;
  org_id: string;
  station_id: string;
  station_ids: string[];
  permissions: string[];
}
export interface Session {
  access_token: string;
  actor: Actor;
}
export interface DataChanged {
  resource: string;
  station: string;
}
export const events = new EventTarget();
let accessToken: string | null = null;
let station = localStorage.getItem("fuel-ops-station") ?? "";
let refreshPromise: Promise<Session> | null = null;
export const scope = () => station;
export function setStation(value: string) {
  station = value;
  localStorage.setItem("fuel-ops-station", value);
  events.dispatchEvent(new Event("station-changed"));
}
export function setSession(session: Session | null) {
  accessToken = session?.access_token ?? null;
  if (session && !session.actor.station_ids.includes(station))
    setStation(session.actor.station_id);
  events.dispatchEvent(new CustomEvent("session-changed", { detail: session }));
}
export class ApiError extends Error {
  constructor(
    message: string,
    public status: number,
  ) {
    super(message);
  }
}
export async function refreshSession(): Promise<Session> {
  if (!refreshPromise) {
    const refresh = () =>
      fetch("/api/v1/auth/refresh", {
        method: "POST",
        credentials: "include",
        headers: station ? { "X-Station-Id": station } : {},
      }).then(async (response) => {
        if (!response.ok) {
          setSession(null);
          throw new ApiError("لطفاً دوباره وارد شوید", response.status);
        }
        const session = (await response.json()) as Session;
        setSession(session);
        return session;
      });
    const coordinated = navigator.locks
      ? navigator.locks.request("fuel-ops-refresh", refresh)
      : refresh();
    refreshPromise = coordinated.finally(() => {
      refreshPromise = null;
    });
  }
  return refreshPromise;
}
export async function api<T>(
  path: string,
  options: RequestInit = {},
  retry = true,
): Promise<T> {
  const response = await fetch(`/api/v1${path}`, {
    ...options,
    credentials: "include",
    headers: {
      ...(options.body ? { "Content-Type": "application/json" } : {}),
      ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
      ...(station ? { "X-Station-Id": station } : {}),
      ...options.headers,
    },
  });
  if (response.status === 401 && retry && !path.startsWith("/auth/")) {
    await refreshSession();
    return api(path, options, false);
  }
  if (!response.ok) {
    const value = (await response.json().catch(() => ({}))) as {
      error?: string;
    };
    throw new ApiError(
      value.error ?? "ارتباط با سرور برقرار نشد",
      response.status,
    );
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}
export async function exportReport(query: string) {
  await refreshSession();
  const response = await fetch(`/api/v1/dashboard/export?${query}`, {
    headers: {
      Authorization: `Bearer ${accessToken}`,
      "X-Station-Id": station,
    },
    credentials: "include",
  });
  if (!response.ok) throw new Error("دریافت گزارش انجام نشد");
  const url = URL.createObjectURL(await response.blob());
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = "fuel-ops-report.csv";
  anchor.click();
  URL.revokeObjectURL(url);
}
export function useResource<T>(resource: string, page = 1, enabled = true) {
  return useQuery({
    queryKey: [station, resource, page],
    queryFn: ({ signal }) =>
      api<Page<Entity<T>>>(`/${resource}?page=${page}&limit=30`, { signal }),
    enabled,
    staleTime: 15_000,
    refetchInterval: 30_000,
  });
}
export function useCommand(resource: string) {
  const run = useLockedAction(`${station}:${resource}`);
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const execute = useCallback(
    async <T>(
      path: string,
      body: unknown,
      method = "POST",
    ): Promise<T | undefined> => {
      setError("");
      setPending(true);
      try {
        return await run(async (signal) => {
          const result = await api<T>(path, {
            method,
            body: JSON.stringify(body),
            signal,
          });
          events.dispatchEvent(
            new CustomEvent<DataChanged>("data-changed", {
              detail: { resource, station },
            }),
          );
          return result;
        });
      } catch (error) {
        if (!(error instanceof DOMException && error.name === "AbortError"))
          setError(error instanceof Error ? error.message : "عملیات انجام نشد");
        return undefined;
      } finally {
        setPending(false);
      }
    },
    [resource, run],
  );
  return { execute, error, pending, clearError: () => setError("") };
}
