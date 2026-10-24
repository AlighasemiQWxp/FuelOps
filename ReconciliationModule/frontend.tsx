import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { Can } from "../AuthModule/frontend";
import type { Shift } from "../ShiftModule/frontend";
import { useStation } from "../StationModule/frontend";
import {
  api,
  scope,
  useCommand,
  useResource,
  type Entity,
} from "../web/src/api";
import {
  Badge,
  Button,
  ErrorNotice,
  Field,
  Form,
  Input,
  Loading,
  Modal,
  PageHeader,
  Pagination,
  Panel,
  Select,
  Table,
  Textarea,
} from "../web/src/ui";
import {
  date,
  integer,
  label,
  liters,
  money,
  signedMilliliters,
} from "../web/src/localization";
interface Reconciliation {
  shift_id: string;
  shift_name: string;
  status: string;
  volume_ml: number;
  expected_irr: number;
  settled_irr: number;
  difference_irr: number;
  tolerance_irr: number;
  investigation: string;
  approved_by: string | null;
}
interface Correction {
  reconciliation_id: string;
  delta_irr: number;
  reason: string;
}
const next: Record<string, string> = {
  CollectingReadings: "Calculating",
  Calculating: "ReviewingDiscrepancies",
  ReviewingDiscrepancies: "Approval",
  Approval: "Completed",
};
export default function Reconciliations() {
  const [page, setPage] = useState(1);
  const query = useResource<Reconciliation>("reconciliations", page);
  const shifts = useResource<Shift>("shifts");
  const command = useCommand("reconciliations");
  const { station } = useStation();
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<Entity<Reconciliation> | null>(null);
  const [target, setTarget] = useState("");
  const corrections = useQuery({
    queryKey: [scope(), "corrections", selected?.id],
    queryFn: () =>
      api<Entity<Correction>[]>(`/reconciliations/${selected?.id}/corrections`),
    enabled: selected?.data.status === "Completed",
  });
  const open = (row: Entity<Reconciliation>, to: string) => {
    setSelected(row);
    setTarget(to);
    command.clearError();
  };
  return (
    <>
      <PageHeader
        title="تطبیق دقیق، تسویه مطمئن"
        description="قرائت‌ها، تعرفه‌های تاریخی و تسویه‌های هر شیفت را بررسی و تأیید کنید."
        actions={
          <Can permission="reconciliation.manage">
            <Button
              onClick={() => {
                setCreating(true);
                command.clearError();
              }}
            >
              <Plus size={17} />
              تطبیق شیفت جدید
            </Button>
          </Can>
        }
      />
      <ErrorNotice message={query.error?.message} />
      <Panel
        title="پرونده‌های تطبیق"
        subtitle="اختلاف به معنی نیاز به بررسی است؛ علت آن از پیش فرض نمی‌شود."
      >
        {query.isPending ? (
          <Loading />
        ) : (
          <>
            <Table
              rows={query.data?.items ?? []}
              rowKey={(row) => row.id}
              columns={[
                {
                  title: "شیفت",
                  render: (row) => (
                    <div className="cell-stack">
                      <b>{row.data.shift_name}</b>
                      <small>{date(row.created_at)}</small>
                    </div>
                  ),
                },
                {
                  title: "حجم · لیتر",
                  render: (row) => liters(row.data.volume_ml),
                },
                {
                  title: "مبلغ مورد انتظار",
                  render: (row) => money(row.data.expected_irr),
                },
                {
                  title: "اختلاف",
                  render: (row) => (
                    <span
                      className={
                        Math.abs(row.data.difference_irr) >
                        row.data.tolerance_irr
                          ? "text-amber"
                          : "text-teal"
                      }
                    >
                      {money(row.data.difference_irr)}
                    </span>
                  ),
                },
                {
                  title: "وضعیت",
                  render: (row) => <Badge value={row.data.status} />,
                },
                {
                  title: "",
                  render: (row) => (
                    <div className="row-actions">
                      {next[row.data.status] && (
                        <Can
                          permission={
                            row.data.status === "Approval"
                              ? "reconciliation.approve"
                              : "reconciliation.manage"
                          }
                        >
                          <Button
                            variant="secondary"
                            size="small"
                            onClick={() => open(row, next[row.data.status])}
                          >
                            {label(next[row.data.status])}
                          </Button>
                        </Can>
                      )}
                      {row.data.status === "Approval" && (
                        <Can permission="reconciliation.manage">
                          <Button
                            variant="ghost"
                            size="small"
                            onClick={() => open(row, "ReviewingDiscrepancies")}
                          >
                            بازگشت برای بررسی
                          </Button>
                        </Can>
                      )}
                      <Button
                        variant="ghost"
                        size="small"
                        onClick={() => open(row, "")}
                      >
                        جزئیات
                      </Button>
                      {row.data.status === "Completed" && (
                        <Can permission="reconciliation.approve">
                          <Button
                            size="small"
                            variant="ghost"
                            onClick={() => open(row, "correction")}
                          >
                            اصلاحیه
                          </Button>
                        </Can>
                      )}
                    </div>
                  ),
                },
              ]}
            />
            <Pagination
              page={page}
              total={query.data?.total ?? 0}
              onPage={setPage}
            />
          </>
        )}
      </Panel>
      <Modal
        open={creating}
        onClose={() => setCreating(false)}
        title="آغاز تطبیق شیفت"
      >
        <Form
          error={command.error}
          pending={command.pending}
          onSave={async (data) => {
            const result = await command.execute("/reconciliations", {
              shift_id: data.get("shift"),
            });
            if (result) setCreating(false);
          }}
        >
          <Field label="شیفت آماده بررسی">
            <Select name="shift" required>
              <option value="">انتخاب شیفت</option>
              {shifts.data?.items
                .filter((row) => row.data.status === "PendingReview")
                .map((row) => (
                  <option key={row.id} value={row.id}>
                    {row.data.name}
                  </option>
                ))}
            </Select>
          </Field>
          <p className="helper-text">
            ابتدا قرائت نهایی و تسویه‌ها را در بخش شیفت‌ها ثبت کنید.
          </p>
        </Form>
      </Modal>
      <Modal
        open={Boolean(selected)}
        onClose={() => setSelected(null)}
        title={
          target === "correction"
            ? "ثبت اصلاحیه قابل پیگیری"
            : (selected?.data.shift_name ?? "تطبیق")
        }
      >
        {selected && (
          <>
            <div className="settlement-summary">
              <div>
                <span>مورد انتظار</span>
                <b>{money(selected.data.expected_irr)}</b>
              </div>
              <div>
                <span>ثبت‌شده</span>
                <b>{money(selected.data.settled_irr)}</b>
              </div>
              <div>
                <span>اختلاف</span>
                <b>{money(selected.data.difference_irr)}</b>
              </div>
            </div>
            {target ? (
              <Form
                error={command.error}
                pending={command.pending}
                submitLabel={
                  target === "correction" ? "ثبت اصلاحیه" : label(target)
                }
                onSave={async (data) => {
                  const correction = target === "correction";
                  const result = await command.execute(
                    `/reconciliations/${selected.id}/${correction ? "corrections" : "transition"}`,
                    correction
                      ? {
                          version: selected.version,
                          delta_irr: integer(data.get("delta")),
                          inventory_delta_ml: signedMilliliters(
                            data.get("inventory_delta"),
                          ),
                          tank_id: data.get("tank") || null,
                          reason: data.get("note"),
                        }
                      : {
                          version: selected.version,
                          to: target,
                          investigation: data.get("note"),
                        },
                  );
                  if (result) setSelected(null);
                }}
              >
                {target === "correction" && (
                  <Field
                    label="تعدیل مالی · ریال"
                    hint="عدد مثبت یا منفی؛ سند اصلی بدون تغییر باقی می‌ماند."
                  >
                    <Input
                      name="delta"
                      inputMode="numeric"
                      defaultValue="0"
                      required
                    />
                  </Field>
                )}
                {target === "correction" && (
                  <Can permission="inventory.manage">
                    <div className="form-grid">
                      <Field
                        label="تعدیل موجودی · لیتر"
                        hint="مثبت: افزایش موجودی؛ منفی: کاهش موجودی. قرائت اصلی تغییر نمی‌کند."
                      >
                        <Input
                          name="inventory_delta"
                          defaultValue="0"
                          inputMode="decimal"
                        />
                      </Field>
                      <Field label="مخزن مرتبط">
                        <Select name="tank">
                          <option value="">بدون تعدیل حجمی</option>
                          {station?.data.tanks.map((tank) => (
                            <option key={tank.id} value={tank.id}>
                              {tank.name}
                            </option>
                          ))}
                        </Select>
                      </Field>
                    </div>
                  </Can>
                )}
                <Field
                  label={
                    target === "correction"
                      ? "علت و مرجع اصلاحیه"
                      : "یادداشت بررسی اختلاف"
                  }
                >
                  <Textarea
                    name="note"
                    defaultValue={selected.data.investigation}
                    required={
                      target === "correction" ||
                      Math.abs(selected.data.difference_irr) >
                        selected.data.tolerance_irr
                    }
                  />
                </Field>
                {target === "Completed" && (
                  <p className="helper-text">
                    تأیید نهایی، موجودی ثبت‌شده مخزن را به‌روز می‌کند و شیفت را
                    برای بستن آماده می‌کند.
                  </p>
                )}
              </Form>
            ) : (
              <div className="detail-body">
                <Badge value={selected.data.status} />
                <p>
                  {selected.data.investigation || "یادداشت بررسی ثبت نشده است."}
                </p>
                {corrections.data?.map((row) => (
                  <div className="correction-card" key={row.id}>
                    <b>{money(row.data.delta_irr)}</b>
                    <p>{row.data.reason}</p>
                    <small>{date(row.created_at)}</small>
                  </div>
                ))}
              </div>
            )}
          </>
        )}
      </Modal>
    </>
  );
}
