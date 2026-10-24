import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Legend,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  ArrowDownToLine,
  ArrowLeft,
  CalendarDays,
  ClipboardCheck,
  Clock3,
  Droplets,
  Filter,
  Fuel,
  RefreshCw,
  ShieldAlert,
  Truck,
  TrendingUp,
  Users,
} from "lucide-react";
import { api, exportReport, scope, useResource } from "../web/src/api";
import {
  Button,
  DateField,
  Empty,
  ErrorNotice,
  Field,
  Form,
  Loading,
  Modal,
  PageHeader,
  Panel,
  Select,
} from "../web/src/ui";
import {
  date,
  label,
  liters,
  money,
  number,
  time,
} from "../web/src/localization";
import { Can } from "../AuthModule/frontend";
import type { Station } from "../StationModule/frontend";
interface Point {
  at: number;
  dispensed_ml: number;
  delivery_ml: number;
  net_ml: number;
}
interface Category {
  name: string;
  value: number;
}
interface Report {
  generated_at: number;
  from: number;
  to: number;
  inventory_ml: number;
  dispensed_ml: number;
  active_shifts: number;
  completed_shifts: number;
  pending_approvals: number;
  open_discrepancies: number;
  financial_difference_irr: number;
  correction_irr: number;
  incoming_deliveries: number;
  open_alerts: number;
  employee_count: number;
  series: Point[];
  inventory_history: { at: number; volume_ml: number }[];
  fuel_distribution: Category[];
  shift_status: Category[];
  issue_categories: Category[];
  discrepancy_status: Category[];
  stations: { name: string; dispensed_ml: number; delivery_ml: number }[];
  recent: {
    id: string;
    collection: string;
    action: string;
    actor_name: string;
    created_at: number;
  }[];
  comparison: { dispensed_ml: number } | null;
}
const colors = ["#0d9488", "#f2b84b", "#7a8ce0", "#e4867d", "#94a3b8"];
export default function Dashboard() {
  const [period, setPeriod] = useState("7d");
  const [interval, setInterval] = useState("day");
  const [compare, setCompare] = useState(false);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [filters, setFilters] = useState<Record<string, string>>({});
  const [hidden, setHidden] = useState<Record<string, boolean>>({});
  const [exportError, setExportError] = useState("");
  const [exporting, setExporting] = useState(false);
  const stations = useResource<Station>("stations");
  const station = stations.data?.items.find((item) => item.id === scope());
  const params = new URLSearchParams({
    period,
    interval,
    compare: String(compare),
    ...filters,
  });
  const query = useQuery({
    queryKey: [scope(), "dashboard", params.toString()],
    queryFn: ({ signal }) => api<Report>(`/dashboard?${params}`, { signal }),
    refetchInterval: 30_000,
  });
  const data = query.data;
  const chartData =
    data?.series.map((point) => ({
      ...point,
      name: date(point.at),
      dispensed: point.dispensed_ml / 1000,
      delivery: point.delivery_ml / 1000,
      net: point.net_ml / 1000,
    })) ?? [];
  const fuel =
    data?.fuel_distribution.map((item) => ({
      ...item,
      name:
        station?.data.tanks.find((tank) => tank.id === item.name)?.name ??
        item.name,
      value: item.value / 1000,
    })) ?? [];
  const delta =
    data?.comparison && data.comparison.dispensed_ml
      ? ((data.dispensed_ml - data.comparison.dispensed_ml) /
          data.comparison.dispensed_ml) *
        100
      : null;
  return (
    <>
      <PageHeader
        eyebrow="نبض جایگاه شما"
        title="یک نگاه، تصویر کامل"
        description="عملیات امروز را ببینید، جزئیات را بررسی کنید و با اطمینان تصمیم بگیرید."
        actions={
          <>
            <Button
              variant="secondary"
              onClick={() => void query.refetch()}
              aria-label="تازه‌سازی"
              disabled={query.isFetching}
            >
              <RefreshCw size={16} className={query.isFetching ? "spin" : ""} />
              تازه‌سازی
            </Button>
            <Can permission="report.export">
              <Button
                disabled={exporting || !data}
                onClick={async () => {
                  setExporting(true);
                  setExportError("");
                  try {
                    await exportReport(params.toString());
                  } catch (error) {
                    setExportError(
                      error instanceof Error
                        ? error.message
                        : "دریافت گزارش انجام نشد",
                    );
                  } finally {
                    setExporting(false);
                  }
                }}
              >
                <ArrowDownToLine size={17} />
                دریافت گزارش
              </Button>
            </Can>
          </>
        }
      />
      <div className="dashboard-toolbar">
        <div className="period-tabs">
          {[
            ["today", "امروز"],
            ["7d", "۷ روز اخیر"],
            ["30d", "۳۰ روز اخیر"],
          ].map(([value, title]) => (
            <button
              key={value}
              className={period === value && !filters.from ? "selected" : ""}
              onClick={() => {
                setPeriod(value);
                setFilters(({ from: _from, to: _to, ...rest }) => rest);
              }}
            >
              {title}
            </button>
          ))}
        </div>
        <div className="toolbar-options">
          <label className="compare-toggle">
            <input
              type="checkbox"
              checked={compare}
              onChange={(event) => setCompare(event.target.checked)}
            />
            <span>مقایسه با دوره قبل</span>
          </label>
          <div className="compact-select">
            <CalendarDays size={16} />
            <Select
              aria-label="بازه زمانی"
              value={period}
              onChange={(event) => {
                setPeriod(event.target.value);
                setFilters(({ from: _from, to: _to, ...rest }) => rest);
              }}
            >
              {[
                ["today", "امروز"],
                ["yesterday", "دیروز"],
                ["7d", "۷ روز اخیر"],
                ["week", "هفته جاری"],
                ["30d", "۳۰ روز اخیر"],
                ["month", "ماه جاری"],
                ["previous_month", "ماه قبل"],
                ["year", "سال جاری"],
              ].map(([value, title]) => (
                <option key={value} value={value}>
                  {title}
                </option>
              ))}
            </Select>
          </div>
          <Button
            variant="secondary"
            size="small"
            onClick={() => setFiltersOpen(true)}
          >
            <Filter size={15} />
            فیلترها
            {Object.keys(filters).length > 0 && (
              <span className="filter-count">
                {number(Object.keys(filters).length)}
              </span>
            )}
          </Button>
        </div>
      </div>
      <ErrorNotice message={query.error?.message || exportError} />
      {query.isPending ? (
        <Loading />
      ) : data ? (
        <>
          <div className="kpi-grid">
            <Kpi
              title="موجودی ثبت‌شده سوخت"
              value={liters(data.inventory_ml)}
              unit="لیتر"
              icon={Droplets}
              tone="teal"
              foot="آخرین موجودی ثبت‌شده مخازن"
            />
            <Kpi
              title="حجم سوخت تحویلی به مشتری"
              value={liters(data.dispensed_ml)}
              unit="لیتر"
              icon={Fuel}
              tone="blue"
              foot={
                delta === null
                  ? "بر پایه شیفت‌های تأییدشده"
                  : `${number(delta, 1)}٪ تغییر نسبت به دوره قبل`
              }
              trend={delta !== null}
            />
            <Kpi
              title="شیفت‌های فعال"
              value={number(data.active_shifts)}
              unit="شیفت"
              icon={Clock3}
              tone="amber"
              foot={`${number(data.completed_shifts)} شیفت تکمیل‌شده در بازه`}
            />
            <Kpi
              title="در انتظار تأیید"
              value={number(data.pending_approvals)}
              unit="مورد"
              icon={ClipboardCheck}
              tone="violet"
              foot={`${number(data.open_discrepancies)} اختلاف در حال بررسی`}
            />
          </div>
          <div className="dashboard-primary">
            <Panel
              title="روند مصرف و تأمین سوخت"
              subtitle="مقایسه حجم خروجی تأییدشده و تحویل‌های ورودی · لیتر"
              actions={
                <Select
                  aria-label="تفکیک نمودار"
                  className="chart-select"
                  value={interval}
                  onChange={(event) => setInterval(event.target.value)}
                >
                  {[
                    ["day", "روزانه"],
                    ["week", "هفتگی"],
                    ["month", "ماهانه"],
                    ["year", "سالانه"],
                  ].map(([value, title]) => (
                    <option key={value} value={value}>
                      {title}
                    </option>
                  ))}
                </Select>
              }
            >
              <div className="chart-summary">
                <div>
                  <small>حجم خروجی در بازه</small>
                  <strong>
                    {liters(data.dispensed_ml)} <span>لیتر</span>
                  </strong>
                </div>
                <div className="chart-legend">
                  <button
                    onClick={() =>
                      setHidden((value) => ({
                        ...value,
                        dispensed: !value.dispensed,
                      }))
                    }
                    className={hidden.dispensed ? "muted" : ""}
                  >
                    <i style={{ background: colors[0] }} />
                    سوخت خروجی
                  </button>
                  <button
                    onClick={() =>
                      setHidden((value) => ({
                        ...value,
                        delivery: !value.delivery,
                      }))
                    }
                    className={hidden.delivery ? "muted" : ""}
                  >
                    <i style={{ background: colors[1] }} />
                    تحویل ورودی
                  </button>
                </div>
              </div>
              <div className="chart chart-main" dir="ltr">
                {chartData.length ? (
                  <ResponsiveContainer width="100%" height="100%">
                    <AreaChart
                      data={chartData}
                      margin={{ top: 12, right: 20, left: 5, bottom: 0 }}
                    >
                      <defs>
                        <linearGradient
                          id="fuelGradient"
                          x1="0"
                          y1="0"
                          x2="0"
                          y2="1"
                        >
                          <stop
                            offset="0%"
                            stopColor="#0d9488"
                            stopOpacity={0.19}
                          />
                          <stop
                            offset="95%"
                            stopColor="#0d9488"
                            stopOpacity={0}
                          />
                        </linearGradient>
                      </defs>
                      <CartesianGrid
                        vertical={false}
                        strokeDasharray="3 5"
                        stroke="var(--chart-grid)"
                      />
                      <XAxis
                        dataKey="name"
                        axisLine={false}
                        tickLine={false}
                        tickMargin={15}
                        minTickGap={38}
                      />
                      <YAxis
                        axisLine={false}
                        tickLine={false}
                        tickFormatter={(value) => number(Number(value))}
                        width={55}
                      />
                      <Tooltip
                        contentStyle={tooltipStyle}
                        formatter={(value, name) => [
                          `${number(Number(value), 2)} لیتر`,
                          name === "dispensed" ? "خروجی" : "ورودی",
                        ]}
                      />
                      <Area
                        hide={hidden.dispensed}
                        type="monotone"
                        dataKey="dispensed"
                        stroke="#0d9488"
                        strokeWidth={2.8}
                        fill="url(#fuelGradient)"
                        activeDot={{ r: 5, strokeWidth: 4, stroke: "#ccfbf1" }}
                      />
                      <Area
                        hide={hidden.delivery}
                        type="monotone"
                        dataKey="delivery"
                        stroke="#e9ae42"
                        strokeWidth={2}
                        fill="transparent"
                        strokeDasharray="5 5"
                      />
                    </AreaChart>
                  </ResponsiveContainer>
                ) : (
                  <Empty title="در این بازه گردش سوختی ثبت نشده" />
                )}
              </div>
            </Panel>
            <Panel
              title="ترکیب انواع سوخت"
              subtitle="آخرین حجم ثبت‌شده · لیتر"
              className="inventory-donut"
            >
              <div className="donut-wrap" dir="ltr">
                {fuel.length ? (
                  <>
                    <ResponsiveContainer width="100%" height="100%">
                      <PieChart>
                        <Pie
                          data={fuel}
                          dataKey="value"
                          innerRadius="70%"
                          outerRadius="91%"
                          paddingAngle={4}
                          cornerRadius={5}
                          stroke="none"
                        >
                          {fuel.map((_, index) => (
                            <Cell
                              key={index}
                              fill={colors[index % colors.length]}
                            />
                          ))}
                        </Pie>
                        <Tooltip
                          contentStyle={tooltipStyle}
                          formatter={(value) => `${number(Number(value))} لیتر`}
                        />
                      </PieChart>
                    </ResponsiveContainer>
                    <div className="donut-center">
                      <Droplets size={20} />
                      <strong>{liters(data.inventory_ml)}</strong>
                      <small>لیتر موجودی</small>
                    </div>
                  </>
                ) : (
                  <Empty />
                )}
              </div>
              <div className="donut-legend">
                {fuel.map((item, index) => (
                  <div key={item.name}>
                    <span>
                      <i
                        style={{ background: colors[index % colors.length] }}
                      />
                      {item.name}
                    </span>
                    <b>
                      {number(item.value)} <small>لیتر</small>
                    </b>
                  </div>
                ))}
              </div>
              <div className="data-caption">
                <span className="live-dot" />
                منبع و تازگی داده در صفحه موجودی
              </div>
            </Panel>
          </div>
          <div className="insight-grid">
            <Link to="/alerts" className="insight-card">
              <span className="insight-icon amber">
                <ShieldAlert size={22} />
              </span>
              <div>
                <b>{number(data.open_alerts)} رویداد باز</b>
                <small>موارد نیازمند پیگیری تیم</small>
              </div>
              <ArrowLeft size={18} />
            </Link>
            <Link to="/deliveries" className="insight-card">
              <span className="insight-icon teal">
                <Truck size={22} />
              </span>
              <div>
                <b>{number(data.incoming_deliveries)} تحویل ورودی</b>
                <small>ثبت‌شده در بازه انتخابی</small>
              </div>
              <ArrowLeft size={18} />
            </Link>
            <Link to="/employees" className="insight-card">
              <span className="insight-icon violet">
                <Users size={22} />
              </span>
              <div>
                <b>{number(data.employee_count)} همکار فعال</b>
                <small>تیم عملیاتی جایگاه</small>
              </div>
              <ArrowLeft size={18} />
            </Link>
          </div>
          <div className="dashboard-secondary">
            <Panel
              title="مقایسه جایگاه‌ها"
              subtitle="حجم تأمین و خروجی ثبت‌شده · لیتر"
            >
              <div className="chart chart-small" dir="ltr">
                {data.stations.length ? (
                  <ResponsiveContainer width="100%" height="100%">
                    <BarChart
                      data={data.stations.map((item) => ({
                        name:
                          stations.data?.items.find(
                            (station) => station.id === item.name,
                          )?.data.name ?? item.name,
                        dispensed: item.dispensed_ml / 1000,
                        delivery: item.delivery_ml / 1000,
                      }))}
                    >
                      <CartesianGrid
                        vertical={false}
                        strokeDasharray="3 5"
                        stroke="var(--chart-grid)"
                      />
                      <XAxis dataKey="name" axisLine={false} tickLine={false} />
                      <YAxis
                        axisLine={false}
                        tickLine={false}
                        tickFormatter={(value) => number(Number(value))}
                      />
                      <Tooltip contentStyle={tooltipStyle} />
                      <Legend />
                      <Bar
                        dataKey="dispensed"
                        name="خروجی"
                        stackId="volume"
                        fill="#0d9488"
                        radius={[0, 0, 4, 4]}
                        maxBarSize={42}
                      />
                      <Bar
                        dataKey="delivery"
                        name="ورودی"
                        stackId="volume"
                        fill="#8dd7ca"
                        radius={[5, 5, 0, 0]}
                        maxBarSize={42}
                      />
                    </BarChart>
                  </ResponsiveContainer>
                ) : (
                  <Empty />
                )}
              </div>
            </Panel>
            <Panel
              title="تازه‌ترین فعالیت‌ها"
              subtitle="ردپای روشن تغییرات عملیاتی"
              actions={
                <Link className="text-link" to="/audit">
                  همه فعالیت‌ها
                  <ArrowLeft size={14} />
                </Link>
              }
            >
              <div className="activity-list">
                {data.recent.length ? (
                  data.recent.map((item, index) => (
                    <div key={item.id} className="activity-item">
                      <span className={`activity-icon activity-${index % 3}`}>
                        <ClipboardCheck size={17} />
                      </span>
                      <div>
                        <b>
                          {label(item.collection)} · {label(item.action)}
                        </b>
                        <small>
                          {item.actor_name} <i />
                          {date(item.created_at)}
                        </small>
                      </div>
                      <time>{time(item.created_at)}</time>
                    </div>
                  ))
                ) : (
                  <Empty title="فعالیتی ثبت نشده" />
                )}
              </div>
            </Panel>
          </div>
          <div className="dashboard-tertiary">
            <CategoryChart title="وضعیت شیفت‌ها" data={data.shift_status} />
            <CategoryChart
              title="دسته‌بندی رویدادها"
              data={data.issue_categories}
            />
            <CategoryChart
              title="وضعیت تطبیق‌ها"
              data={data.discrepancy_status}
            />
          </div>
          <Panel
            title="روند تاریخی موجودی"
            subtitle="آخرین مانده ثبت‌شده هر روز، با حفظ آخرین مقدار در روزهای بدون گردش؛ سنجش زنده تجهیزات نیست"
          >
            <div className="chart chart-small" dir="ltr">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart
                  data={data.inventory_history.map((item) => ({
                    name: date(item.at),
                    volume: item.volume_ml / 1000,
                  }))}
                >
                  <CartesianGrid
                    strokeDasharray="3 5"
                    stroke="var(--chart-grid)"
                  />
                  <XAxis dataKey="name" minTickGap={38} />
                  <YAxis tickFormatter={(value) => number(Number(value))} />
                  <Tooltip
                    contentStyle={tooltipStyle}
                    formatter={(value) => `${number(Number(value))} لیتر`}
                  />
                  <Line
                    dataKey="volume"
                    name="موجودی ثبت‌شده"
                    type="monotone"
                    stroke="#7a8ce0"
                    strokeWidth={2}
                    dot={false}
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </Panel>
          <div className="report-foot">
            <span>
              به‌روزرسانی: {date(data.generated_at)}، {time(data.generated_at)}
            </span>
            <span>
              اختلاف مالی: {money(data.financial_difference_irr)} · اصلاحات
              ثبت‌شده: {money(data.correction_irr)}
            </span>
          </div>
        </>
      ) : (
        <Empty
          title="گزارش در دسترس نیست"
          description="اتصال به سرور و مجوز دسترسی را بررسی کنید."
        />
      )}
      <Modal
        open={filtersOpen}
        onClose={() => setFiltersOpen(false)}
        title="فیلترهای گزارش"
      >
        <Form
          onSave={(form) => {
            const next: Record<string, string> = {};
            for (const [key, value] of form.entries())
              if (String(value)) next[key] = String(value);
            if (form.get("custom") !== "on") {
              delete next.from;
              delete next.to;
            }
            delete next.custom;
            setFilters(next);
            setFiltersOpen(false);
          }}
          submitLabel="اعمال فیلترها"
        >
          <div className="form-grid">
            <Field label="جایگاه">
              <Select name="station" defaultValue={filters.station ?? ""}>
                <option value="">جایگاه جاری</option>
                <option value="all">تمام جایگاه‌های مجاز</option>
                {stations.data?.items.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.data.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="مخزن">
              <Select name="tank" defaultValue={filters.tank ?? ""}>
                <option value="">همه مخازن</option>
                {station?.data.tanks.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="سکو">
              <Select name="pump" defaultValue={filters.pump ?? ""}>
                <option value="">همه سکوها</option>
                {station?.data.pumps.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="نازل">
              <Select name="nozzle" defaultValue={filters.nozzle ?? ""}>
                <option value="">همه نازل‌ها</option>
                {station?.data.nozzles.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="نوع سوخت">
              <Select name="fuel_type" defaultValue={filters.fuel_type ?? ""}>
                <option value="">همه سوخت‌ها</option>
                {station?.data.fuel_types.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="وضعیت شیفت">
              <Select name="status" defaultValue={filters.status ?? ""}>
                <option value="">همه وضعیت‌ها</option>
                {[
                  "Scheduled",
                  "Open",
                  "PendingReview",
                  "Approved",
                  "Closed",
                ].map((status) => (
                  <option key={status} value={status}>
                    {label(status)}
                  </option>
                ))}
              </Select>
            </Field>
          </div>
          <label className="check-option">
            <input
              type="checkbox"
              name="custom"
              defaultChecked={Boolean(filters.from)}
            />
            بازه تاریخ دلخواه
          </label>
          <div className="form-grid">
            <DateField
              name="from"
              title="از تاریخ"
              value={
                filters.from ? Number(filters.from) : Date.now() - 7 * 86400000
              }
            />
            <DateField
              name="to"
              title="تا تاریخ (انحصاری)"
              value={filters.to ? Number(filters.to) : Date.now() + 86400000}
            />
          </div>
        </Form>
      </Modal>
    </>
  );
}
const tooltipStyle = {
  background: "var(--surface)",
  border: "1px solid var(--border)",
  borderRadius: 12,
  color: "var(--text)",
  fontFamily: "Vazirmatn",
  fontSize: 12,
  direction: "rtl" as const,
};
function Kpi({
  title,
  value,
  unit,
  icon: Icon,
  tone,
  foot,
  trend = false,
}: {
  title: string;
  value: string;
  unit: string;
  icon: typeof Droplets;
  tone: string;
  foot: string;
  trend?: boolean;
}) {
  return (
    <div className={`kpi-card kpi-${tone}`}>
      <div className="kpi-top">
        <span>{title}</span>
        <span className={`kpi-icon ${tone}`}>
          <Icon size={20} />
        </span>
      </div>
      <div className="kpi-value">
        <strong>{value}</strong>
        <span>{unit}</span>
      </div>
      <div className="kpi-foot">
        {trend ? <TrendingUp size={14} /> : <i />}
        {foot}
      </div>
    </div>
  );
}
function CategoryChart({ title, data }: { title: string; data: Category[] }) {
  return (
    <Panel title={title}>
      <div className="category-chart" dir="ltr">
        {data.length ? (
          <ResponsiveContainer width="100%" height="100%">
            <PieChart>
              <Pie
                data={data.map((item) => ({ ...item, name: label(item.name) }))}
                dataKey="value"
                innerRadius={38}
                outerRadius={59}
                paddingAngle={3}
              >
                {data.map((_, index) => (
                  <Cell
                    key={index}
                    fill={colors[index % colors.length]}
                    stroke="none"
                  />
                ))}
              </Pie>
              <Tooltip contentStyle={tooltipStyle} />
              <Legend
                formatter={(value) => (
                  <span className="legend-text">{value}</span>
                )}
              />
            </PieChart>
          </ResponsiveContainer>
        ) : (
          <Empty />
        )}
      </div>
    </Panel>
  );
}
