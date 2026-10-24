import { useState } from "react";
import { ClipboardList, Clock3, Plus } from "lucide-react";
import { Can } from "../AuthModule/frontend";
import { useStation, type Nozzle } from "../StationModule/frontend";
import type { Employee } from "../EmployeeModule/frontend";
import { useCommand, useResource, type Entity } from "../web/src/api";
import {
  Badge,
  Button,
  DateField,
  ErrorNotice,
  Field,
  Form,
  Input,
  Loading,
  Modal,
  MultiSelect,
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
  milliliters,
  money,
  time,
} from "../web/src/localization";
export interface Reading {
  nozzle_id: string;
  initial_ml: number;
  final_ml: number | null;
  rollovers: number;
  tiers: { tier_id: string; volume_ml: number }[];
}
export interface Shift {
  name: string;
  starts_at: number;
  ends_at: number;
  employee_ids: string[];
  nozzle_ids: string[];
  status: string;
  equipment: Nozzle[];
  readings: Reading[];
  settlements: { method: string; amount_irr: number; reference: string }[];
  handover_note: string;
}
export default function Shifts() {
  const [page, setPage] = useState(1);
  const query = useResource<Shift>("shifts", page);
  const employees = useResource<Employee>("employees");
  const { station } = useStation();
  const command = useCommand("shifts");
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<Entity<Shift> | null>(null);
  const [to, setTo] = useState("");
  function act(row: Entity<Shift>, target: string) {
    setSelected(row);
    setTo(target);
    command.clearError();
  }
  return (
    <>
      <PageHeader
        title="هر شیفت، یک جریان روشن"
        description="برنامه‌ریزی تیم، ثبت کنتورها و تحویل شیفت را از یک نقطه مدیریت کنید."
        actions={
          <Can permission="shift.create">
            <Button
              onClick={() => {
                command.clearError();
                setCreating(true);
              }}
            >
              <Plus size={17} />
              برنامه‌ریزی شیفت
            </Button>
          </Can>
        }
      />
      <ErrorNotice message={query.error?.message} />
      <Panel
        title="شیفت‌های جایگاه"
        subtitle="ساعت‌ها به وقت تهران نمایش داده می‌شوند"
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
                      <b>{row.data.name}</b>
                      <small>{date(row.data.starts_at)}</small>
                    </div>
                  ),
                },
                {
                  title: "بازه فعالیت",
                  render: (row) => (
                    <span className="time-range">
                      {time(row.data.starts_at)} ← {time(row.data.ends_at)}
                    </span>
                  ),
                },
                {
                  title: "همکاران",
                  render: (row) =>
                    row.data.employee_ids
                      .map(
                        (id) =>
                          employees.data?.items.find(
                            (employee) => employee.id === id,
                          )?.data.name ?? "همکار جایگاه",
                      )
                      .join("، "),
                },
                {
                  title: "وضعیت",
                  render: (row) => <Badge value={row.data.status} />,
                },
                {
                  title: "عملیات",
                  render: (row) => (
                    <div className="row-actions">
                      {row.data.status === "Scheduled" && (
                        <Can permission="shift.open">
                          <Button
                            size="small"
                            variant="secondary"
                            onClick={() => act(row, "Open")}
                          >
                            باز کردن شیفت
                          </Button>
                        </Can>
                      )}
                      {row.data.status === "Open" && (
                        <Can permission="shift.close">
                          <Button
                            size="small"
                            onClick={() => act(row, "PendingReview")}
                          >
                            ثبت پایان شیفت
                          </Button>
                        </Can>
                      )}
                      {row.data.status === "Approved" && (
                        <Can permission="shift.close">
                          <Button
                            size="small"
                            onClick={() => act(row, "Closed")}
                          >
                            بستن و تحویل
                          </Button>
                        </Can>
                      )}
                      {row.data.status === "Scheduled" && (
                        <Can permission="shift.create">
                          <Button
                            size="small"
                            variant="ghost"
                            onClick={() => act(row, "Cancelled")}
                          >
                            لغو
                          </Button>
                        </Can>
                      )}
                      <Button
                        variant="ghost"
                        size="small"
                        onClick={() => act(row, "")}
                      >
                        <ClipboardList size={15} />
                        جزئیات
                      </Button>
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
        title="برنامه‌ریزی شیفت جدید"
      >
        <Form
          error={command.error}
          pending={command.pending}
          onSave={async (data) => {
            const result = await command.execute("/shifts", {
              name: data.get("name"),
              starts_at: integer(data.get("starts_at")),
              ends_at: integer(data.get("ends_at")),
              employee_ids: data.getAll("employees"),
              nozzle_ids: data.getAll("nozzles"),
            });
            if (result) setCreating(false);
          }}
        >
          <Field label="نام شیفت">
            <Input name="name" placeholder="برای نمونه: شیفت صبح" required />
          </Field>
          <div className="form-grid">
            <DateField title="شروع شیفت" name="starts_at" withTime />
            <DateField
              title="پایان شیفت"
              name="ends_at"
              withTime
              value={Date.now() + 86400000}
            />
          </div>
          <h3>همکاران شیفت</h3>
          <MultiSelect
            name="employees"
            items={
              employees.data?.items
                .filter((row) => row.data.active)
                .map((row) => ({ id: row.id, name: row.data.name })) ?? []
            }
          />
          <h3>نازل‌های تحت مسئولیت</h3>
          <MultiSelect
            name="nozzles"
            items={
              station?.data.nozzles.filter((nozzle) => nozzle.active) ?? []
            }
          />
        </Form>
      </Modal>
      <Modal
        open={Boolean(selected)}
        onClose={() => setSelected(null)}
        title={
          to
            ? `${label(to)} · ${selected?.data.name}`
            : `جزئیات ${selected?.data.name}`
        }
        description="قرائت‌ها بر حسب لیتر و تسویه‌ها بر حسب ریال هستند."
      >
        {selected &&
          (to ? (
            <Form
              key={`${selected.id}:${to}`}
              error={command.error}
              pending={command.pending}
              submitLabel="ثبت و ادامه"
              onSave={async (data) => {
                const readings = selected.data.equipment.map((nozzle) => {
                  const before = selected.data.readings.find(
                    (reading) => reading.nozzle_id === nozzle.id,
                  );
                  return {
                    nozzle_id: nozzle.id,
                    initial_ml:
                      to === "Open"
                        ? milliliters(data.get(`initial-${nozzle.id}`))
                        : (before?.initial_ml ?? 0),
                    final_ml:
                      to === "PendingReview"
                        ? milliliters(data.get(`final-${nozzle.id}`))
                        : null,
                    rollovers:
                      to === "PendingReview"
                        ? integer(data.get(`rollover-${nozzle.id}`))
                        : 0,
                    tiers:
                      to === "PendingReview"
                        ? nozzle.prices.map((price) => ({
                            tier_id: price.id,
                            volume_ml: milliliters(
                              data.get(`tier-${nozzle.id}-${price.id}`),
                            ),
                          }))
                        : [],
                  };
                });
                const settlements = ["card", "cash", "credit"].map(
                  (method) => ({
                    method,
                    amount_irr: integer(data.get(method)),
                    reference: String(data.get("reference") ?? ""),
                  }),
                );
                const result = await command.execute(
                  `/shifts/${selected.id}/transition`,
                  {
                    version: selected.version,
                    to,
                    readings,
                    settlements,
                    handover_note: data.get("handover_note"),
                  },
                );
                if (result) setSelected(null);
              }}
            >
              {(to === "Open" || to === "PendingReview") &&
                selected.data.equipment.map((nozzle) => (
                  <div className="reading-card" key={nozzle.id}>
                    <h3>
                      <FuelIcon />
                      {nozzle.name}
                    </h3>
                    {to === "Open" ? (
                      <Field label="قرائت آغاز · لیتر">
                        <Input
                          name={`initial-${nozzle.id}`}
                          inputMode="decimal"
                          required
                        />
                      </Field>
                    ) : (
                      <>
                        <div className="reading-baseline">
                          قرائت آغاز:{" "}
                          <b>
                            {liters(
                              selected.data.readings.find(
                                (reading) => reading.nozzle_id === nozzle.id,
                              )?.initial_ml ?? 0,
                            )}{" "}
                            لیتر
                          </b>
                        </div>
                        <div className="form-grid">
                          <Field label="قرائت پایان · لیتر">
                            <Input
                              name={`final-${nozzle.id}`}
                              inputMode="decimal"
                              required
                            />
                          </Field>
                          <Field label="گردش تأییدشده کنتور">
                            <Select name={`rollover-${nozzle.id}`}>
                              <option value="0">بدون گردش</option>
                              <option value="1">یک گردش کامل</option>
                            </Select>
                          </Field>
                        </div>
                        <div className="form-grid">
                          {nozzle.prices.map((price) => (
                            <Field
                              key={price.id}
                              label={`${price.name} · حجم به لیتر`}
                              hint={`${money(price.irr_per_liter)} برای هر لیتر`}
                            >
                              <Input
                                name={`tier-${nozzle.id}-${price.id}`}
                                defaultValue="0"
                                inputMode="decimal"
                                required
                              />
                            </Field>
                          ))}
                        </div>
                      </>
                    )}
                  </div>
                ))}
              {to === "PendingReview" && (
                <>
                  <h3>ثبت تسویه</h3>
                  <div className="form-grid">
                    {[
                      ["card", "کارتخوان"],
                      ["cash", "نقدی"],
                      ["credit", "اعتباری"],
                    ].map(([key, title]) => (
                      <Field key={key} label={`${title} · ریال`}>
                        <Input
                          name={key}
                          defaultValue="0"
                          inputMode="numeric"
                        />
                      </Field>
                    ))}
                    <Field label="شماره پیگیری / مرجع">
                      <Input name="reference" required />
                    </Field>
                  </div>
                  <p className="helper-text">
                    مجموع حجم تعرفه‌ها باید دقیقاً با اختلاف قرائت کنتور برابر
                    باشد.
                  </p>
                </>
              )}
              {to === "Closed" && (
                <Field label="یادداشت تحویل شیفت">
                  <Textarea
                    name="handover_note"
                    required
                    placeholder="وضعیت تجهیزات و نکات لازم برای شیفت بعد"
                  />
                </Field>
              )}
              {to === "Cancelled" && (
                <p>
                  این شیفت برنامه‌ریزی‌شده لغو می‌شود و در تاریخچه باقی می‌ماند.
                </p>
              )}
            </Form>
          ) : (
            <div className="detail-body">
              <Badge value={selected.data.status} />
              <Table
                rows={selected.data.readings}
                rowKey={(row) => row.nozzle_id}
                columns={[
                  {
                    title: "نازل",
                    render: (row) =>
                      selected.data.equipment.find(
                        (nozzle) => nozzle.id === row.nozzle_id,
                      )?.name,
                  },
                  {
                    title: "آغاز · لیتر",
                    render: (row) => liters(row.initial_ml),
                  },
                  {
                    title: "پایان · لیتر",
                    render: (row) =>
                      row.final_ml === null ? "ثبت نشده" : liters(row.final_ml),
                  },
                ]}
              />
              <h3>تسویه‌ها</h3>
              {selected.data.settlements.map((settlement, index) => (
                <div className="detail-line" key={index}>
                  <span>
                    {settlement.method} · {settlement.reference}
                  </span>
                  <b>{money(settlement.amount_irr)}</b>
                </div>
              ))}
              {selected.data.handover_note && (
                <p>{selected.data.handover_note}</p>
              )}
            </div>
          ))}
      </Modal>
    </>
  );
}
function FuelIcon() {
  return <Clock3 size={17} />;
}
