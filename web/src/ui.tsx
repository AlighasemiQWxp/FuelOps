import {
  useEffect,
  useState,
  type ButtonHTMLAttributes,
  type FormHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import {
  AlertCircle,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  Inbox,
  LoaderCircle,
  X,
} from "lucide-react";
import DatePicker, { type DateObject } from "react-multi-date-picker";
import persian from "react-date-object/calendars/persian";
import persianFa from "react-date-object/locales/persian_fa";
import CalendarDate from "react-date-object";
import gregorian from "react-date-object/calendars/gregorian";
import gregorianEn from "react-date-object/locales/gregorian_en";
import { useHandleOwner, useLockState } from "../../HandleSystem/react";
import { label, number } from "./localization";
export const cn = (...values: ClassValue[]) => twMerge(clsx(values));
const buttonStyles = cva("button", {
  variants: {
    variant: {
      default: "button-primary",
      secondary: "button-secondary",
      ghost: "button-ghost",
      danger: "button-danger",
    },
    size: { default: "", small: "button-small", icon: "button-icon" },
  },
  defaultVariants: { variant: "default", size: "default" },
});
export function Button({
  className,
  variant,
  size,
  asChild,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> &
  VariantProps<typeof buttonStyles> & { asChild?: boolean }) {
  const Component = asChild ? Slot : "button";
  return (
    <Component
      className={cn(buttonStyles({ variant, size }), className)}
      {...props}
    />
  );
}
export function Field({
  label: title,
  children,
  hint,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
}) {
  return (
    <label className="field">
      <span>{title}</span>
      {children}
      {hint && <small>{hint}</small>}
    </label>
  );
}
export function Input(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={cn("input", props.className)} />;
}
export function Select(props: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...props} className={cn("input", props.className)} />;
}
export function Textarea(props: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea {...props} className={cn("input textarea", props.className)} />
  );
}
export function DateField({
  name,
  value,
  title = "تاریخ",
  withTime = false,
}: {
  name: string;
  value?: number;
  title?: string;
  withTime?: boolean;
}) {
  const [selected, setSelected] = useState<DateObject | null>(null);
  const [clock, setClock] = useState(() =>
    new Intl.DateTimeFormat("en-GB", {
      timeZone: "Asia/Tehran",
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    }).format(value ?? Date.now()),
  );
  const base = value ?? Date.now();
  const parts = new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Tehran",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).formatToParts(base);
  const part = (type: string) =>
    parts.find((p) => p.type === type)?.value ?? "";
  const isoDate = selected
    ? new CalendarDate(selected)
        .convert(gregorian, gregorianEn)
        .format("YYYY-MM-DD")
    : `${part("year")}-${part("month")}-${part("day")}`;
  const timestamp = Date.parse(
    `${isoDate}T${withTime ? clock : "00:00"}:00+03:30`,
  );
  const displayDate =
    selected ??
    new CalendarDate({
      date: isoDate,
      format: "YYYY-MM-DD",
      calendar: gregorian,
    }).convert(persian, persianFa);
  return (
    <Field label={title}>
      <div className="date-field">
        <DatePicker
          calendar={persian}
          locale={persianFa}
          value={displayDate}
          onChange={(value) => setSelected(value as DateObject | null)}
          inputClass="input"
          calendarPosition="bottom-right"
          format="YYYY/MM/DD"
        />
        {withTime && (
          <Input
            type="time"
            value={clock}
            onChange={(e) => setClock(e.target.value)}
            aria-label="ساعت به وقت تهران"
          />
        )}
      </div>
      <input type="hidden" name={name} value={timestamp} />
    </Field>
  );
}
export function Badge({ value }: { value: string }) {
  return (
    <span className={cn("badge", `badge-${value.toLowerCase()}`)}>
      <i />
      {label(value)}
    </span>
  );
}
export function ErrorNotice({ message }: { message?: string }) {
  if (!message) return null;
  return (
    <div className="notice notice-error" role="alert">
      <AlertCircle size={18} />
      <span>{message}</span>
    </div>
  );
}
export function Success({ children }: { children: ReactNode }) {
  return (
    <div className="notice notice-success" role="status">
      <CheckCircle2 size={18} />
      {children}
    </div>
  );
}
export function Loading() {
  return (
    <div
      className="skeleton-grid"
      aria-label="در حال دریافت اطلاعات"
      aria-busy="true"
    >
      {[1, 2, 3, 4].map((i) => (
        <div className="skeleton" key={i} />
      ))}
    </div>
  );
}
export function Empty({
  title = "هنوز اطلاعاتی ثبت نشده",
  description = "با ثبت اولین مورد، اطلاعات این بخش نمایش داده می‌شود.",
}: {
  title?: string;
  description?: string;
}) {
  return (
    <div className="empty">
      <span>
        <Inbox size={28} />
      </span>
      <h3>{title}</h3>
      <p>{description}</p>
    </div>
  );
}
export function PageHeader({
  eyebrow = "عملیات جایگاه",
  title,
  description,
  actions,
}: {
  eyebrow?: string;
  title: string;
  description: string;
  actions?: ReactNode;
}) {
  return (
    <header className="page-heading">
      <div>
        <span className="eyebrow">{eyebrow}</span>
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      <div className="page-actions">{actions}</div>
    </header>
  );
}
export function Panel({
  title,
  subtitle,
  children,
  actions,
  className,
}: {
  title?: string;
  subtitle?: string;
  children: ReactNode;
  actions?: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("panel", className)}>
      {title && (
        <div className="panel-heading">
          <div>
            <h2>{title}</h2>
            {subtitle && <p>{subtitle}</p>}
          </div>
          {actions}
        </div>
      )}
      {children}
    </section>
  );
}
export function Modal({
  open,
  onClose,
  title,
  description = "اطلاعات را بررسی و ثبت کنید.",
  children,
}: {
  open: boolean;
  onClose(): void;
  title: string;
  description?: string;
  children: ReactNode;
}) {
  const { owner, system } = useHandleOwner();
  const busy = useLockState("navigation");
  useEffect(() => {
    if (!open) return;
    const handle = system.getLockHandle(owner, "modal-interaction");
    handle.use();
    return () => handle.release();
  }, [open, owner, system]);
  return (
    <Dialog.Root
      open={open}
      onOpenChange={(value) => {
        if (!value && !busy) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content
          className="dialog"
          dir="rtl"
          onEscapeKeyDown={(event) => {
            if (busy) event.preventDefault();
          }}
        >
          <div className="dialog-heading">
            <div>
              <Dialog.Title>{title}</Dialog.Title>
              <Dialog.Description>{description}</Dialog.Description>
            </div>
            <Dialog.Close asChild>
              <Button
                variant="ghost"
                size="icon"
                aria-label="بستن"
                disabled={busy}
              >
                <X size={19} />
              </Button>
            </Dialog.Close>
          </div>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export function Form({
  children,
  onSave,
  error,
  pending,
  submitLabel = "ذخیره اطلاعات",
  ...props
}: Omit<FormHTMLAttributes<HTMLFormElement>, "onSubmit"> & {
  children: ReactNode;
  onSave(data: FormData): Promise<void> | void;
  error?: string;
  pending?: boolean;
  submitLabel?: string;
}) {
  const [localError, setLocalError] = useState("");
  return (
    <form
      {...props}
      onSubmit={async (event) => {
        event.preventDefault();
        setLocalError("");
        const data = new FormData(event.currentTarget);
        try {
          await onSave(data);
        } catch (error) {
          setLocalError(
            error instanceof Error ? error.message : "اطلاعات معتبر نیست",
          );
        }
      }}
    >
      <fieldset disabled={pending} className="form-fields">
        {children}
      </fieldset>
      <ErrorNotice message={localError || error} />
      <div className="form-footer">
        <span>اطلاعات در جایگاه انتخاب‌شده ثبت می‌شود</span>
        <Button type="submit" disabled={pending}>
          {pending && <LoaderCircle size={17} className="spin" />}
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}
export function Table<T>({
  rows,
  columns,
  rowKey,
}: {
  rows: T[];
  columns: { title: string; render(row: T): ReactNode }[];
  rowKey(row: T): string;
}) {
  if (!rows.length) return <Empty />;
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            {columns.map((column, i) => (
              <th key={i}>{column.title}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={rowKey(row)}>
              {columns.map((column, i) => (
                <td key={i}>{column.render(row)}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
export function Pagination({
  page,
  total,
  onPage,
}: {
  page: number;
  total: number;
  onPage(page: number): void;
}) {
  return (
    <div className="pagination">
      <span>{number(total)} مورد ثبت‌شده</span>
      <div>
        <Button
          variant="ghost"
          size="small"
          disabled={page <= 1}
          onClick={() => onPage(page - 1)}
        >
          <ArrowRight size={16} />
          قبلی
        </Button>
        <span>{number(page)}</span>
        <Button
          variant="ghost"
          size="small"
          disabled={page * 30 >= total}
          onClick={() => onPage(page + 1)}
        >
          بعدی
          <ArrowLeft size={16} />
        </Button>
      </div>
    </div>
  );
}
export function MultiSelect({
  name,
  items,
  selected = [],
}: {
  name: string;
  items: { id: string; name: string }[];
  selected?: string[];
}) {
  return (
    <div className="check-grid">
      {items.map((item) => (
        <label key={item.id} className="check-option">
          <input
            type="checkbox"
            name={name}
            value={item.id}
            defaultChecked={selected.includes(item.id)}
          />
          <span>{item.name}</span>
        </label>
      ))}
    </div>
  );
}
