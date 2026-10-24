import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Fuel, ArrowLeft, ShieldCheck, Activity, Layers3 } from "lucide-react";
import {
  api,
  events,
  refreshSession,
  setSession,
  type Actor,
  type Session,
} from "../web/src/api";
import { Button, ErrorNotice, Field, Input } from "../web/src/ui";
const Context = createContext<{ actor: Actor | null; ready: boolean }>({
  actor: null,
  ready: false,
});
export function AuthProvider({ children }: { children: ReactNode }) {
  const client = useQueryClient();
  const identity = useRef("");
  const [actor, setActor] = useState<Actor | null>(null);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let active = true;
    const listener = (event: Event) => {
      if (active) {
        const next =
          (event as CustomEvent<Session | null>).detail?.actor ?? null;
        const nextIdentity = next ? `${next.org_id}:${next.user_id}` : "";
        if (identity.current !== nextIdentity) {
          client.clear();
          identity.current = nextIdentity;
        }
        setActor(next);
        setReady(true);
      }
    };
    events.addEventListener("session-changed", listener);
    void refreshSession()
      .catch(() => undefined)
      .finally(() => {
        if (active) setReady(true);
      });
    return () => {
      active = false;
      events.removeEventListener("session-changed", listener);
    };
  }, [client]);
  return (
    <Context.Provider value={{ actor, ready }}>{children}</Context.Provider>
  );
}
export function useSession() {
  return useContext(Context);
}
export function usePermission(permission: string) {
  return useSession().actor?.permissions.includes(permission) ?? false;
}
export function Can({
  permission,
  children,
}: {
  permission: string;
  children: ReactNode;
}) {
  return usePermission(permission) ? <>{children}</> : null;
}
export default function Login() {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  return (
    <div className="login-page">
      <section className="login-story">
        <div className="brand brand-light">
          <span className="brand-icon">
            <Fuel size={27} />
          </span>
          <div>
            <b>Fuel Ops</b>
            <small>هوشمند، یکپارچه، قابل اعتماد</small>
          </div>
        </div>
        <div className="login-art">
          <div className="orbit orbit-one" />
          <div className="orbit orbit-two" />
          <div className="art-center">
            <Fuel size={64} strokeWidth={1.2} />
          </div>
          <div className="floating-card float-one">
            <Activity size={20} />
            <span>دید کامل بر عملیات</span>
            <i className="live-dot" />
          </div>
          <div className="floating-card float-two">
            <ShieldCheck size={20} />
            <span>هر تغییر، قابل پیگیری</span>
          </div>
        </div>
        <div className="login-copy">
          <span className="eyebrow">فراتر از مدیریت روزمره</span>
          <h1>
            همه‌چیز در جریان.
            <br />
            همه‌چیز تحت کنترل.
          </h1>
          <p>
            از اولین قرائت کنتور تا آخرین تأیید شیفت؛
            <br />
            تصویری روشن از تمام جایگاه‌های شما.
          </p>
          <div className="login-features">
            <span>
              <Layers3 size={16} />
              چند جایگاه، یک دیدگاه
            </span>
            <span>
              <ShieldCheck size={16} />
              دسترسی امن و دقیق
            </span>
          </div>
        </div>
        <small className="login-foot">FUEL OPS · STATION OPERATIONS</small>
      </section>
      <section className="login-form-wrap">
        <div className="login-form">
          <span className="mini-brand">
            <Fuel size={23} /> Fuel Ops
          </span>
          <span className="eyebrow">خوش آمدید</span>
          <h2>وارد فضای کاری خود شوید</h2>
          <p>برای ادامه، اطلاعات حساب خود را وارد کنید.</p>
          <form
            onSubmit={async (event) => {
              event.preventDefault();
              setPending(true);
              setError("");
              const data = new FormData(event.currentTarget);
              try {
                const session = await api<Session>(
                  "/auth/login",
                  {
                    method: "POST",
                    body: JSON.stringify(Object.fromEntries(data)),
                  },
                  false,
                );
                setSession(session);
              } catch (error) {
                setError(
                  error instanceof Error ? error.message : "ورود انجام نشد",
                );
              } finally {
                setPending(false);
              }
            }}
          >
            <Field label="شناسه سازمان">
              <Input
                name="organization"
                autoComplete="organization"
                defaultValue="fuelops-demo"
                required
                dir="ltr"
              />
            </Field>
            <Field label="ایمیل">
              <Input
                name="email"
                type="email"
                autoComplete="username"
                placeholder="you@example.com"
                required
                dir="ltr"
              />
            </Field>
            <Field label="رمز عبور">
              <Input
                name="password"
                type="password"
                autoComplete="current-password"
                minLength={12}
                required
                dir="ltr"
              />
            </Field>
            <ErrorNotice message={error} />
            <Button type="submit" disabled={pending} className="login-submit">
              {pending ? "در حال ورود…" : "ورود به Fuel Ops"}
              <ArrowLeft size={18} />
            </Button>
          </form>
          <div className="demo-note">
            <span>محیط نمایشی</span>
            <p>
              حساب مالک: <b dir="ltr">owner@fuelops.demo</b>
              <br />
              رمز عبور همان مقداری است که هنگام آماده‌سازی داده‌ها تعیین
              کرده‌اید. تمام اطلاعات نمونه، فرضی است.
            </p>
          </div>
        </div>
        <small>مدیریت مطمئن، تصمیم‌های روشن‌تر.</small>
      </section>
    </div>
  );
}
