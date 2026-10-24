import {
  lazy,
  Suspense,
  useEffect,
  useState,
  Component,
  type ErrorInfo,
  type ReactNode,
} from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  NavLink,
  Navigate,
  Route,
  Routes,
  useBlocker,
  useLocation,
} from "react-router";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import {
  Activity,
  Bell,
  ChevronDown,
  ClipboardCheck,
  Clock3,
  Droplets,
  Fuel,
  LayoutDashboard,
  LogOut,
  Menu,
  Moon,
  Settings2,
  ShieldCheck,
  Sun,
  Truck,
  Users,
  X,
} from "lucide-react";
import Login, { useSession } from "../../AuthModule/frontend";
import { useLockState } from "../../HandleSystem/react";
import {
  api,
  events,
  scope,
  setSession,
  setStation,
  useResource,
  type Actor,
  type DataChanged,
} from "./api";
import { Button, ErrorNotice, Loading, Modal, Panel } from "./ui";
import { date } from "./localization";
const Dashboard = lazy(() => import("../../DashboardModule/frontend"));
const Shifts = lazy(() => import("../../ShiftModule/frontend"));
const Reconciliation = lazy(
  () => import("../../ReconciliationModule/frontend"),
);
const Inventory = lazy(() => import("../../InventoryModule/frontend"));
const Deliveries = lazy(() => import("../../DeliveryModule/frontend"));
const Employees = lazy(() => import("../../EmployeeModule/frontend"));
const Permissions = lazy(() => import("../../PermissionModule/frontend"));
const Stations = lazy(() => import("../../StationModule/frontend"));
const Alerts = lazy(() => import("../../AlertModule/frontend"));
const Audit = lazy(() => import("../../AuditModule/frontend"));
const modules = [
  {
    path: "/",
    title: "نمای کلی",
    icon: LayoutDashboard,
    permission: "dashboard.view",
    component: Dashboard,
  },
  {
    path: "/shifts",
    title: "مدیریت شیفت‌ها",
    icon: Clock3,
    permission: "shift.view",
    component: Shifts,
  },
  {
    path: "/reconciliations",
    title: "تطبیق و تسویه",
    icon: ClipboardCheck,
    permission: "reconciliation.view",
    component: Reconciliation,
  },
  {
    path: "/inventory",
    title: "موجودی مخازن",
    icon: Droplets,
    permission: "inventory.view",
    component: Inventory,
  },
  {
    path: "/deliveries",
    title: "تحویل سوخت",
    icon: Truck,
    permission: "delivery.view",
    component: Deliveries,
  },
  {
    path: "/alerts",
    title: "رویدادها و هشدارها",
    icon: Bell,
    permission: "alert.view",
    component: Alerts,
  },
  {
    path: "/employees",
    title: "کارکنان",
    icon: Users,
    permission: "employee.view",
    component: Employees,
  },
  {
    path: "/roles",
    title: "نقش‌ها و دسترسی",
    icon: ShieldCheck,
    permission: "permission.manage",
    component: Permissions,
  },
  {
    path: "/stations",
    title: "تنظیمات جایگاه",
    icon: Settings2,
    permission: "station.view",
    component: Stations,
  },
  {
    path: "/audit",
    title: "تاریخچه فعالیت",
    icon: Activity,
    permission: "audit.view",
    component: Audit,
  },
];
class ErrorBoundary extends Component<
  { children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(_error: Error, _info: ErrorInfo) {}
  render() {
    if (this.state.failed)
      return (
        <Panel title="نمایش این صفحه با مشکل مواجه شد">
          <p>صفحه را دوباره بارگذاری کنید. اطلاعات ثبت‌شده محفوظ است.</p>
          <Button onClick={() => window.location.reload()}>
            بارگذاری مجدد
          </Button>
        </Panel>
      );
    return this.props.children;
  }
}
export default function App() {
  const { actor, ready } = useSession();
  const client = useQueryClient();
  useEffect(() => {
    const listener = (event: Event) => {
      const detail = (event as CustomEvent<DataChanged>).detail;
      void client.invalidateQueries({
        predicate: (query) => query.queryKey[0] === detail.station,
      });
    };
    events.addEventListener("data-changed", listener);
    return () => events.removeEventListener("data-changed", listener);
  }, [client]);
  if (!ready)
    return (
      <div className="boot">
        <Fuel size={40} />
        <span>Fuel Ops</span>
        <Loading />
      </div>
    );
  if (!actor) return <Login />;
  return <Workspace actor={actor} />;
}
function Workspace({ actor }: { actor: Actor }) {
  const client = useQueryClient();
  const location = useLocation();
  const reduceMotion = useReducedMotion();
  const [selected, selectStation] = useState(scope());
  const [mobile, setMobile] = useState(false);
  const [error, setError] = useState("");
  const [theme, setTheme] = useState(
    () => localStorage.getItem("fuel-ops-theme") ?? "light",
  );
  const stations = useResource<{ name: string; demo: boolean }>(
    "stations",
    1,
    actor.permissions.includes("station.view"),
  );
  const locked = useLockState("navigation");
  const blocker = useBlocker(locked);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("fuel-ops-theme", theme);
  }, [theme]);
  useEffect(() => {
    const listener = (event: BeforeUnloadEvent) => {
      if (locked) event.preventDefault();
    };
    window.addEventListener("beforeunload", listener);
    return () => window.removeEventListener("beforeunload", listener);
  }, [locked]);
  const available = modules.filter((module) =>
    actor.permissions.includes(module.permission),
  );
  const current =
    modules.find((module) => module.path === location.pathname)?.title ??
    "Fuel Ops";
  const station = stations.data?.items.find((item) => item.id === selected);
  return (
    <div className="workspace">
      <aside className={`sidebar ${mobile ? "sidebar-open" : ""}`}>
        <div className="brand">
          <span className="brand-icon">
            <Fuel size={26} />
          </span>
          <div>
            <b>Fuel Ops</b>
            <small>مدیریت هوشمند جایگاه</small>
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="mobile-only"
            onClick={() => setMobile(false)}
            aria-label="بستن فهرست"
          >
            <X size={20} />
          </Button>
        </div>
        <div className="workspace-label">
          <span>فضای کاری</span>
          <i>عملیات</i>
        </div>
        <nav>
          {available.map((module, index) => (
            <NavLink
              end={module.path === "/"}
              key={module.path}
              to={module.path}
              onClick={() => {
                if (!locked) setMobile(false);
              }}
              className={({ isActive }) =>
                `nav-item ${isActive ? "nav-active" : ""} ${index === 6 ? "nav-divider" : ""}`
              }
            >
              <module.icon size={19} strokeWidth={1.7} />
              <span>{module.title}</span>
              {module.path === "/" && <i className="nav-dot" />}
            </NavLink>
          ))}
        </nav>
        <div className="sidebar-footer">
          <div className="system-health">
            <span className="live-dot" />
            <span>اتصال امن به فضای کاری</span>
          </div>
          <div className="profile">
            <span className="avatar">{actor.name.slice(0, 1)}</span>
            <div>
              <b>{actor.name}</b>
              <small>حساب سازمانی</small>
            </div>
            <Button
              aria-label="خروج از حساب"
              variant="ghost"
              size="icon"
              disabled={locked}
              onClick={async () => {
                try {
                  await api("/auth/logout", { method: "POST" });
                  client.clear();
                  setSession(null);
                } catch (error) {
                  setError(
                    error instanceof Error ? error.message : "خروج انجام نشد",
                  );
                }
              }}
            >
              <LogOut size={17} />
            </Button>
          </div>
        </div>
      </aside>
      {mobile && (
        <button
          className="sidebar-scrim"
          aria-label="بستن فهرست"
          onClick={() => setMobile(false)}
        />
      )}
      <div className="main-column">
        <header className="topbar">
          <div className="breadcrumb">
            <Button
              variant="ghost"
              size="icon"
              className="mobile-only"
              onClick={() => setMobile(true)}
              aria-label="باز کردن فهرست"
            >
              <Menu size={21} />
            </Button>
            <span>فضای مدیریت</span>
            <span className="breadcrumb-slash">/</span>
            <b>{current}</b>
          </div>
          <div className="topbar-tools">
            <div className="station-select">
              <span className="station-pin">
                <Fuel size={15} />
              </span>
              <select
                aria-label="انتخاب جایگاه"
                disabled={locked}
                value={selected}
                onChange={async (event) => {
                  const next = event.target.value;
                  setStation(next);
                  selectStation(next);
                  try {
                    const nextActor = await api<Actor>("/auth/me");
                    events.dispatchEvent(
                      new CustomEvent("session-changed", {
                        detail: { actor: nextActor },
                      }),
                    );
                    await client.invalidateQueries();
                  } catch (error) {
                    setError(
                      error instanceof Error
                        ? error.message
                        : "انتخاب جایگاه انجام نشد",
                    );
                  }
                }}
              >
                {stations.data?.items.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.data.name}
                  </option>
                )) ?? <option value={selected}>{selected}</option>}
              </select>
              <ChevronDown size={14} />
            </div>
            <span className="topbar-separator" />
            <Button
              variant="ghost"
              size="icon"
              onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
              aria-label="تغییر پوسته"
            >
              {theme === "dark" ? <Sun size={19} /> : <Moon size={19} />}
            </Button>
            <NavLink
              to="/alerts"
              className="notification-button"
              aria-label="رویدادها"
            >
              <Bell size={19} />
              <i />
            </NavLink>
            <span className="avatar small">{actor.name.slice(0, 1)}</span>
          </div>
        </header>
        <main>
          <div className="context-strip">
            <span>
              {date(Date.now())}
              <i />
              به وقت تهران
            </span>
            {station?.data.demo && (
              <span className="demo-pill">محیط نمایشی · اطلاعات فرضی</span>
            )}
          </div>
          <ErrorNotice message={error} />
          <AnimatePresence mode="wait">
            <motion.div
              key={`${location.pathname}:${selected}`}
              initial={{ opacity: 0, y: reduceMotion ? 0 : 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              transition={{ duration: reduceMotion ? 0 : 0.18 }}
            >
              <ErrorBoundary>
                <Suspense fallback={<Loading />}>
                  <Routes>
                    {available.map((module) => (
                      <Route
                        key={module.path}
                        path={module.path}
                        element={<module.component />}
                      />
                    ))}
                    <Route
                      path="*"
                      element={
                        available[0] ? (
                          <Navigate to={available[0].path} replace />
                        ) : (
                          <Panel title="دسترسی تعریف نشده">
                            <p>
                              برای دریافت دسترسی با مدیر سازمان تماس بگیرید.
                            </p>
                          </Panel>
                        )
                      }
                    />
                  </Routes>
                </Suspense>
              </ErrorBoundary>
            </motion.div>
          </AnimatePresence>
          <footer className="page-footer">
            <span>
              Fuel Ops <i /> مدیریت یکپارچه عملیات جایگاه
            </span>
            <span>مقادیر مالی به ریال · حجم به لیتر</span>
          </footer>
        </main>
      </div>
      <Modal
        open={blocker.state === "blocked"}
        onClose={() => blocker.reset?.()}
        title="عملیات در حال ثبت است"
        description="برای جلوگیری از ارسال تکراری، تا پایان ثبت در این صفحه بمانید."
      >
        <p>پس از پایان عملیات می‌توانید صفحه را تغییر دهید.</p>
      </Modal>
    </div>
  );
}
