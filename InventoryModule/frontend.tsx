import { useState } from "react";
import { Droplets, Plus } from "lucide-react";
import { Can } from "../AuthModule/frontend";
import { useStation } from "../StationModule/frontend";
import { useCommand, useResource } from "../web/src/api";
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
  liters,
  milliliters,
  number,
  time,
} from "../web/src/localization";
interface Balance {
  tank_id: string;
  volume_ml: number;
  source: string;
  measured_at: number;
}
interface Movement {
  tank_id: string;
  quantity_ml: number;
  kind: string;
  reference: string;
  source: string;
  measured_at: number;
  resulting_volume_ml: number;
}
const movementNames: Record<string, string> = {
  dispensing: "خروجی شیفت",
  delivery: "تحویل ورودی",
  measurement: "اندازه‌گیری",
  correction: "اصلاحیه",
};
export default function Inventory() {
  const [page, setPage] = useState(1);
  const query = useResource<Balance>("inventory");
  const movements = useResource<Movement>("inventory/movements", page);
  const { station } = useStation();
  const command = useCommand("inventory");
  const [open, setOpen] = useState(false);
  return (
    <>
      <PageHeader
        title="موجودی، با منبع روشن"
        description="آخرین حجم ثبت‌شده، گردش سوخت و زمان اندازه‌گیری هر مخزن را دنبال کنید."
        actions={
          <Can permission="inventory.manage">
            <Button
              onClick={() => {
                setOpen(true);
                command.clearError();
              }}
            >
              <Plus size={17} />
              ثبت اندازه‌گیری
            </Button>
          </Can>
        }
      />
      <ErrorNotice message={query.error?.message || movements.error?.message} />
      {query.isPending ? (
        <Loading />
      ) : (
        <div className="tank-grid">
          {station?.data.tanks.map((tank) => {
            const row = query.data?.items.find(
              (item) => item.data.tank_id === tank.id,
            );
            const fill = Math.min(
              100,
              Math.max(
                0,
                ((row?.data.volume_ml ?? 0) / tank.capacity_ml) * 100,
              ),
            );
            return (
              <Panel key={tank.id} className="tank-card">
                <div className="tank-card-heading">
                  <span className="tank-icon">
                    <Droplets size={22} />
                  </span>
                  <div>
                    <h2>{tank.name}</h2>
                    <p>
                      {
                        station.data.fuel_types.find(
                          (fuel) => fuel.id === tank.fuel_type_id,
                        )?.name
                      }
                    </p>
                  </div>
                  {row && <Badge value={row.data.source} />}
                </div>
                <div className="tank-visual">
                  <div className="tank-level" style={{ height: `${fill}%` }} />
                  <div className="tank-value">
                    <strong>{row ? liters(row.data.volume_ml) : "—"}</strong>
                    <span>لیتر ثبت‌شده</span>
                  </div>
                  <div className="tank-lines">
                    <i />
                    <i />
                    <i />
                  </div>
                </div>
                <div className="tank-details">
                  <span>ظرفیت {liters(tank.capacity_ml)} لیتر</span>
                  <b>{number(fill)}٪</b>
                </div>
                <div className="capacity-track">
                  <span style={{ width: `${fill}%` }} />
                </div>
                <div className="tank-updated">
                  {row
                    ? `آخرین ثبت: ${date(row.data.measured_at)}، ${time(row.data.measured_at)}`
                    : "اندازه‌گیری اولیه ثبت نشده"}
                </div>
                <small className="muted">
                  این مقدار، سنجش زنده تجهیزات صنعتی نیست.
                </small>
              </Panel>
            );
          })}
        </div>
      )}
      <Panel
        title="گردش موجودی"
        subtitle="ورودی، خروجی و اصلاح اندازه‌گیری به صورت مستقل ثبت می‌شوند"
      >
        {movements.isPending ? (
          <Loading />
        ) : (
          <>
            <Table
              rows={movements.data?.items ?? []}
              rowKey={(row) => row.id}
              columns={[
                {
                  title: "زمان",
                  render: (row) => (
                    <div className="cell-stack">
                      <b>{date(row.data.measured_at)}</b>
                      <small>{time(row.data.measured_at)}</small>
                    </div>
                  ),
                },
                {
                  title: "مخزن",
                  render: (row) =>
                    station?.data.tanks.find(
                      (tank) => tank.id === row.data.tank_id,
                    )?.name,
                },
                {
                  title: "نوع ثبت",
                  render: (row) =>
                    movementNames[row.data.kind] ?? row.data.kind,
                },
                {
                  title: "تغییر · لیتر",
                  render: (row) => (
                    <span
                      className={
                        row.data.quantity_ml >= 0 ? "text-teal" : "text-amber"
                      }
                    >
                      {liters(row.data.quantity_ml)}
                    </span>
                  ),
                },
                {
                  title: "مانده · لیتر",
                  render: (row) => liters(row.data.resulting_volume_ml),
                },
                {
                  title: "منبع",
                  render: (row) => <Badge value={row.data.source} />,
                },
              ]}
            />
            <Pagination
              page={page}
              total={movements.data?.total ?? 0}
              onPage={setPage}
            />
          </>
        )}
      </Panel>
      <Modal
        open={open}
        onClose={() => setOpen(false)}
        title="ثبت اندازه‌گیری مخزن"
        description="حجم معتبر را مستقیم وارد کنید. ارتفاع مخزن بدون جدول کالیبراسیون به حجم تبدیل نمی‌شود."
      >
        <Form
          pending={command.pending}
          error={command.error}
          onSave={async (data) => {
            const existing = query.data?.items.find(
              (row) => row.data.tank_id === data.get("tank"),
            );
            const result = await command.execute("/inventory", {
              tank_id: data.get("tank"),
              volume_ml: milliliters(data.get("volume")),
              source: data.get("source"),
              measured_at: integer(data.get("measured_at")),
              note: data.get("note"),
              version: existing?.version ?? null,
            });
            if (result) setOpen(false);
          }}
        >
          <div className="form-grid">
            <Field label="مخزن">
              <Select name="tank" required>
                {station?.data.tanks.map((tank) => (
                  <option key={tank.id} value={tank.id}>
                    {tank.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="حجم ثبت‌شده · لیتر">
              <Input name="volume" inputMode="decimal" required />
            </Field>
            <Field label="منبع">
              <Select name="source">
                <option value="manual">اندازه‌گیری دستی</option>
                <option value="simulated">شبیه‌سازی</option>
              </Select>
            </Field>
            <DateField name="measured_at" title="زمان اندازه‌گیری" withTime />
          </div>
          <Field label="روش اندازه‌گیری و توضیح">
            <Textarea name="note" required />
          </Field>
        </Form>
      </Modal>
    </>
  );
}
