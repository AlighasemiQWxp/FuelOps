export const number = (value: number, maximumFractionDigits = 0) =>
  new Intl.NumberFormat("fa-IR", { maximumFractionDigits }).format(value);
export const liters = (ml: number) => number(ml / 1000, 2);
export const money = (irr: number) => `${number(irr)} ریال`;
export const date = (timestamp: number) =>
  new Intl.DateTimeFormat("fa-IR-u-ca-persian", {
    timeZone: "Asia/Tehran",
    year: "numeric",
    month: "short",
    day: "numeric",
  }).format(timestamp);
export const time = (timestamp: number) =>
  new Intl.DateTimeFormat("fa-IR", {
    timeZone: "Asia/Tehran",
    hour: "2-digit",
    minute: "2-digit",
  }).format(timestamp);
export function latin(value: string) {
  return value
    .replace(/[۰-۹]/g, (digit) => String(digit.charCodeAt(0) - 1776))
    .replace(/[٠-٩]/g, (digit) => String(digit.charCodeAt(0) - 1632))
    .replace(/٬/g, "")
    .replace(/٫/g, ".");
}
export function integer(value: FormDataEntryValue | null, fallback = 0) {
  const parsed = Number(latin(String(value ?? fallback)));
  if (!Number.isSafeInteger(parsed))
    throw new Error("مقدار باید یک عدد صحیح معتبر باشد");
  return parsed;
}
export function milliliters(value: FormDataEntryValue | null) {
  const normalized = latin(String(value ?? "0"));
  if (!/^\d+(\.\d{1,3})?$/.test(normalized))
    throw new Error("حجم را با حداکثر سه رقم اعشار وارد کنید");
  const [whole, fraction = ""] = normalized.split(".");
  const ml = Number(whole) * 1000 + Number(fraction.padEnd(3, "0"));
  if (!Number.isSafeInteger(ml)) throw new Error("حجم بیش از حد مجاز است");
  return ml;
}
export const labels: Record<string, string> = {
  Scheduled: "برنامه‌ریزی‌شده",
  Open: "باز",
  PendingReview: "در انتظار بررسی",
  Approved: "تأیید‌شده",
  Closed: "بسته‌شده",
  Cancelled: "لغوشده",
  CollectingReadings: "جمع‌آوری قرائت‌ها",
  Calculating: "محاسبه",
  ReviewingDiscrepancies: "بررسی اختلاف",
  Approval: "تأیید سرپرست",
  Completed: "تکمیل‌شده",
  Registered: "ثبت‌شده",
  AwaitingMeasurements: "در انتظار اندازه‌گیری",
  Measuring: "اندازه‌گیری",
  Reviewing: "بررسی",
  Acknowledged: "در حال پیگیری",
  Resolved: "رفع‌شده",
  manual: "ثبت دستی",
  simulated: "شبیه‌سازی",
  calculated: "محاسبه‌شده",
  high: "زیاد",
  medium: "متوسط",
  low: "کم",
  discrepancy: "اختلاف تسویه",
  inventory: "موجودی",
  nozzle: "نازل",
  delivery: "تحویل",
  overdue_shift: "تأخیر شیفت",
  reading: "کنتور",
  handover: "تحویل شیفت",
  other: "سایر",
  created: "ایجاد",
  updated: "ویرایش",
  demo_seeded: "آماده‌سازی اطلاعات نمایشی",
  shifts: "شیفت",
  stations: "جایگاه",
  users: "کارکنان",
  roles: "نقش‌ها",
  deliveries: "تحویل سوخت",
  reconciliations: "تطبیق شیفت",
  alerts: "رویداد",
  inventory_balances: "موجودی",
  inventory_movements: "گردش موجودی",
  refresh_sessions: "ورود به سامانه",
};
export const label = (value: string) => labels[value] ?? value;

export function signedMilliliters(value: FormDataEntryValue | null) {
  const normalized = latin(String(value ?? "0"));
  if (normalized.startsWith("-")) return -milliliters(normalized.slice(1));
  return milliliters(normalized);
}
