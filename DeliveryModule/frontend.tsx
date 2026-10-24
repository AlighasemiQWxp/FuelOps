import { useState } from "react";
import { Plus } from "lucide-react";
import { Can } from "../AuthModule/frontend";
import { useStation } from "../StationModule/frontend";
import { useCommand, useResource, type Entity } from "../web/src/api";
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
import { date, label, liters, milliliters } from "../web/src/localization";
interface Delivery {
  document_number: string;
  supplier: string;
  tank_id: string;
  fuel_type_id: string;
  declared_ml: number;
  before_ml: number | null;
  after_ml: number | null;
  measured_ml: number | null;
  difference_ml: number | null;
  source: string;
  status: string;
  note: string;
}
const next: Record<string, string> = {
  Registered: "AwaitingMeasurements",
  AwaitingMeasurements: "Measuring",
  Measuring: "Reviewing",
  Reviewing: "Approved",
  Approved: "Completed",
};
export default function Deliveries() {
  const [page, setPage] = useState(1);
  const query = useResource<Delivery>("deliveries", page);
  const { station } = useStation();
  const command = useCommand("deliveries");
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<Entity<Delivery> | null>(null);
  const [to, setTo] = useState("");
  const act = (row: Entity<Delivery>, target: string) => {
    setSelected(row);
    setTo(target);
    command.clearError();
  };
  return (
    <>
      <PageHeader
        title="از سند تحویل تا موجودی"
        description="هر محموله را ثبت کنید، حجم‌ها را مقایسه کنید و با تأیید سرپرست نهایی کنید."
        actions={
          <Can permission="delivery.create">
            <Button
              onClick={() => {
                setCreating(true);
                command.clearError();
              }}
            >
              <Plus size={17} />
              ثبت محموله
            </Button>
          </Can>
        }
      />
      <ErrorNotice message={query.error?.message} />
      <Panel
        title="محموله‌های ورودی"
        subtitle="حجم اندازه‌گیری‌شده، محاسبه‌شده و شبیه‌سازی‌شده جدا نگهداری می‌شوند"
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
                  title: "سند و تأمین‌کننده",
                  render: (row) => (
                    <div className="cell-stack">
                      <b>{row.data.document_number}</b>
                      <small>{row.data.supplier}</small>
                    </div>
                  ),
                },
                {
                  title: "مخزن",
                  render: (row) =>
                    station?.data.tanks.find(
                      (tank) => tank.id === row.data.tank_id,
                    )?.name ?? row.data.tank_id,
                },
                {
                  title: "اعلامی · لیتر",
                  render: (row) => liters(row.data.declared_ml),
                },
                {
                  title: "اندازه‌گیری · لیتر",
                  render: (row) =>
                    row.data.measured_ml === null
                      ? "در انتظار ثبت"
                      : liters(row.data.measured_ml),
                },
                {
                  title: "منبع",
                  render: (row) => <Badge value={row.data.source} />,
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
                            ["Reviewing", "Approved"].includes(row.data.status)
                              ? "delivery.approve"
                              : "delivery.create"
                          }
                        >
                          <Button
                            size="small"
                            variant="secondary"
                            onClick={() => act(row, next[row.data.status])}
                          >
                            {label(next[row.data.status])}
                          </Button>
                        </Can>
                      )}
                      {row.data.status === "Reviewing" && (
                        <Can permission="delivery.create">
                          <Button
                            size="small"
                            variant="ghost"
                            onClick={() => act(row, "Measuring")}
                          >
                            بازبینی
                          </Button>
                        </Can>
                      )}
                      <Button
                        variant="ghost"
                        size="small"
                        onClick={() => act(row, "")}
                      >
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
        title="ثبت محموله جدید"
      >
        <Form
          error={command.error}
          pending={command.pending}
          onSave={async (data) => {
            const tank = station?.data.tanks.find(
              (tank) => tank.id === data.get("tank"),
            );
            const result = await command.execute("/deliveries", {
              document_number: data.get("document"),
              supplier: data.get("supplier"),
              tank_id: tank?.id,
              fuel_type_id: tank?.fuel_type_id,
              declared_ml: milliliters(data.get("declared")),
              source: data.get("source"),
            });
            if (result) setCreating(false);
          }}
        >
          <div className="form-grid">
            <Field label="شماره سند تحویل">
              <Input name="document" required />
            </Field>
            <Field label="تأمین‌کننده">
              <Input name="supplier" required />
            </Field>
            <Field label="مخزن و نوع سوخت">
              <Select name="tank" required>
                {station?.data.tanks.map((tank) => (
                  <option key={tank.id} value={tank.id}>
                    {tank.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="حجم اعلامی · لیتر">
              <Input name="declared" inputMode="decimal" required />
            </Field>
            <Field label="منبع اطلاعات">
              <Select name="source">
                <option value="manual">ثبت دستی</option>
                <option value="simulated">شبیه‌سازی</option>
              </Select>
            </Field>
          </div>
        </Form>
      </Modal>
      <Modal
        open={Boolean(selected)}
        onClose={() => setSelected(null)}
        title={`${selected?.data.document_number ?? ""} · ${to ? label(to) : "جزئیات تحویل"}`}
        description="حجم مستقیم وارد کنید؛ تبدیل ارتفاع به حجم بدون کالیبراسیون معتبر انجام نمی‌شود."
      >
        {selected && (
          <>
            <div className="settlement-summary">
              <div>
                <span>حجم اعلامی</span>
                <b>{liters(selected.data.declared_ml)} لیتر</b>
              </div>
              <div>
                <span>اختلاف</span>
                <b>
                  {selected.data.difference_ml === null
                    ? "—"
                    : `${liters(selected.data.difference_ml)} لیتر`}
                </b>
              </div>
            </div>
            {to ? (
              <Form
                error={command.error}
                pending={command.pending}
                submitLabel={label(to)}
                onSave={async (data) => {
                  const result = await command.execute(
                    `/deliveries/${selected.id}/transition`,
                    {
                      version: selected.version,
                      to,
                      before_ml:
                        to === "Reviewing"
                          ? milliliters(data.get("before"))
                          : null,
                      after_ml:
                        to === "Reviewing"
                          ? milliliters(data.get("after"))
                          : null,
                      note: data.get("note"),
                    },
                  );
                  if (result) setSelected(null);
                }}
              >
                {to === "Reviewing" && (
                  <div className="form-grid">
                    <Field label="موجودی قبل · لیتر">
                      <Input
                        name="before"
                        inputMode="decimal"
                        defaultValue={(selected.data.before_ml ?? 0) / 1000}
                        required
                      />
                    </Field>
                    <Field label="موجودی بعد · لیتر">
                      <Input
                        name="after"
                        inputMode="decimal"
                        defaultValue={(selected.data.after_ml ?? 0) / 1000}
                        required
                      />
                    </Field>
                  </div>
                )}
                <Field label="توضیح و نتیجه بررسی">
                  <Textarea name="note" defaultValue={selected.data.note} />
                </Field>
                {to === "Completed" && (
                  <p className="helper-text">
                    حجم تحویلی تأییدشده به موجودی ثبت‌شده اضافه می‌شود.
                  </p>
                )}
              </Form>
            ) : (
              <div className="detail-body">
                <Badge value={selected.data.status} />
                <p>{selected.data.note || "یادداشتی ثبت نشده است."}</p>
                <small>{date(selected.created_at)}</small>
              </div>
            )}
          </>
        )}
      </Modal>
    </>
  );
}
