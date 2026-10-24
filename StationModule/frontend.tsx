import { useEffect, useRef, useState } from "react";
import {
  Droplets,
  Fuel,
  MapPin,
  Plus,
  Settings2,
  ArrowRight,
} from "lucide-react";
import { BasePhasedController } from "../BasePhasedController/frontend/controller";
import { useHandleOwner } from "../HandleSystem/react";
import type { LockHandle } from "../HandleSystem/handles";
import { Can } from "../AuthModule/frontend";
import { scope, useCommand, useResource, type Entity } from "../web/src/api";
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
  Panel,
  Select,
} from "../web/src/ui";
import { integer, liters, milliliters, number } from "../web/src/localization";
export interface Price {
  id: string;
  name: string;
  irr_per_liter: number;
}
export interface Tank {
  id: string;
  name: string;
  fuel_type_id: string;
  capacity_ml: number;
  low_threshold_ml: number;
  calibration_reference: string | null;
}
export interface Nozzle {
  id: string;
  name: string;
  pump_id: string;
  tank_id: string;
  meter_modulus_ml: number;
  active: boolean;
  prices: Price[];
}
export interface Station {
  name: string;
  city: string;
  timezone: string;
  demo: boolean;
  fuel_types: { id: string; name: string; color: string }[];
  tanks: Tank[];
  pumps: { id: string; name: string }[];
  nozzles: Nozzle[];
  discrepancy_tolerance_irr: number;
  delivery_tolerance_ml: number;
}
export function useStation() {
  const result = useResource<Station>("stations");
  return {
    ...result,
    station: result.data?.items.find((item) => item.id === scope()),
  };
}
type Phase = "BasicInformation" | "Configuration" | "Review" | "Confirmation";
const phaseLabels: Record<Phase, string> = {
  BasicInformation: "اطلاعات پایه",
  Configuration: "تجهیزات و قیمت‌ها",
  Review: "بازبینی",
  Confirmation: "ثبت نهایی",
};
const blank: Station = {
  name: "",
  city: "",
  timezone: "Asia/Tehran",
  demo: false,
  fuel_types: [{ id: "petrol", name: "بنزین معمولی", color: "#0d9488" }],
  tanks: [],
  pumps: [],
  nozzles: [],
  discrepancy_tolerance_irr: 10000,
  delivery_tolerance_ml: 100000,
};
export default function Stations() {
  const query = useStation();
  const command = useCommand("stations");
  const [editing, setEditing] = useState<Entity<Station> | null>(null);
  const [draft, setDraft] = useState<Station>(blank);
  const [open, setOpen] = useState(false);
  const [phase, setPhase] = useState<Phase>("BasicInformation");
  const controller = useRef<BasePhasedController<Phase> | null>(null);
  const review = useRef<LockHandle | null>(null);
  const { owner, system } = useHandleOwner();
  useEffect(
    () => () => {
      void controller.current?.cancel().catch(() => undefined);
    },
    [],
  );
  async function begin(row: Entity<Station> | null) {
    command.clearError();
    setEditing(row);
    setDraft(structuredClone(row?.data ?? blank));
    setPhase("BasicInformation");
    controller.current = new BasePhasedController<Phase>(
      "BasicInformation",
      {
        BasicInformation: ["Configuration"],
        Configuration: ["BasicInformation", "Review"],
        Review: ["Configuration", "Confirmation"],
        Confirmation: [],
      },
      {
        enter: (next) => {
          if (next === "Review") {
            review.current = system.getLockHandle(owner, "station-review");
            review.current.use();
          }
        },
        exit: () => {
          review.current?.release();
          review.current = null;
        },
        cleanup: () => {
          review.current?.release();
          review.current = null;
        },
      },
    );
    controller.current.subscribe((event) => {
      if (event.current) setPhase(event.current);
    });
    await controller.current.initialize();
    setOpen(true);
  }
  function read(data: FormData): Station {
    return {
      ...draft,
      name: String(data.get("name")),
      city: String(data.get("city")),
      discrepancy_tolerance_irr: integer(data.get("tolerance")),
      delivery_tolerance_ml: milliliters(data.get("deliveryTolerance")),
      tanks: draft.tanks.map((tank) => ({
        ...tank,
        name: String(data.get(`tank-name-${tank.id}`)),
        capacity_ml: milliliters(data.get(`capacity-${tank.id}`)),
        low_threshold_ml: milliliters(data.get(`low-${tank.id}`)),
        fuel_type_id: String(data.get(`fuel-${tank.id}`)),
      })),
      pumps: draft.pumps.map((pump) => ({
        ...pump,
        name: String(data.get(`pump-${pump.id}`)),
      })),
      nozzles: draft.nozzles.map((nozzle) => ({
        ...nozzle,
        name: String(data.get(`nozzle-${nozzle.id}`)),
        pump_id: String(data.get(`pump-id-${nozzle.id}`)),
        tank_id: String(data.get(`tank-id-${nozzle.id}`)),
        meter_modulus_ml: milliliters(data.get(`modulus-${nozzle.id}`)),
        active: data.get(`active-${nozzle.id}`) === "on",
        prices: nozzle.prices.map((price) => ({
          ...price,
          name: String(data.get(`price-name-${nozzle.id}-${price.id}`)),
          irr_per_liter: integer(data.get(`price-${nozzle.id}-${price.id}`)),
        })),
      })),
    };
  }
  return (
    <>
      <PageHeader
        title="جایگاه‌ها و تجهیزات"
        description="ساختار جایگاه، مخازن، نازل‌ها و قواعد قیمت‌گذاری را مدیریت کنید."
        actions={
          <Can permission="station.manage">
            <Button onClick={() => void begin(null)}>
              <Plus size={17} />
              جایگاه جدید
            </Button>
          </Can>
        }
      />
      <ErrorNotice message={query.error?.message} />
      {query.isPending ? (
        <Loading />
      ) : (
        <div className="station-grid">
          {query.data?.items.map((row) => (
            <Panel key={row.id} className="station-card">
              <div className="station-card-top">
                <span className="station-symbol">
                  <Fuel size={28} />
                </span>
                <div>
                  <h2>{row.data.name}</h2>
                  <p>
                    <MapPin size={13} />
                    {row.data.city} · Asia/Tehran
                  </p>
                </div>
                {row.data.demo && <Badge value="simulated" />}
              </div>
              <div className="station-stats">
                <div>
                  <b>{number(row.data.tanks.length)}</b>
                  <span>مخزن</span>
                </div>
                <div>
                  <b>{number(row.data.pumps.length)}</b>
                  <span>سکو</span>
                </div>
                <div>
                  <b>{number(row.data.nozzles.length)}</b>
                  <span>نازل</span>
                </div>
              </div>
              <div className="equipment-list">
                {row.data.tanks.map((tank) => (
                  <div key={tank.id}>
                    <Droplets size={17} />
                    <span>{tank.name}</span>
                    <b>{liters(tank.capacity_ml)} لیتر</b>
                  </div>
                ))}
              </div>
              <Can permission="station.manage">
                <Button
                  variant="secondary"
                  disabled={row.id !== scope()}
                  onClick={() => void begin(row)}
                >
                  <Settings2 size={16} />
                  ویرایش تنظیمات
                </Button>
              </Can>
              {row.id !== scope() && (
                <small className="muted">
                  برای ویرایش، ابتدا این جایگاه را از نوار بالا انتخاب کنید.
                </small>
              )}
            </Panel>
          ))}
        </div>
      )}
      <Modal
        open={open}
        onClose={() => {
          void controller.current?.cancel();
          setOpen(false);
        }}
        title={editing ? "تنظیمات جایگاه" : "راه‌اندازی جایگاه"}
        description="تغییر تجهیزات و قیمت‌ها پس از پایان شیفت‌های فعال امکان‌پذیر است."
      >
        <div className="phase-steps">
          {(["BasicInformation", "Configuration", "Review"] as Phase[]).map(
            (item, index) => (
              <span
                key={item}
                className={phase === item ? "phase-current" : ""}
              >
                <i>{number(index + 1)}</i>
                {phaseLabels[item]}
              </span>
            ),
          )}
        </div>
        <Form
          pending={command.pending}
          error={command.error}
          submitLabel={phase === "Review" ? "تأیید و ذخیره" : "مرحله بعد"}
          onSave={async (data) => {
            if (phase !== "Review") {
              await controller.current?.nextPhase();
              return;
            }
            const value = read(data);
            const result = await command.execute(
              editing ? `/stations/${editing.id}` : "/stations",
              editing ? { version: editing.version, data: value } : value,
              editing ? "PUT" : "POST",
            );
            if (result) {
              await controller.current?.transitionTo("Confirmation");
              setOpen(false);
            }
          }}
        >
          <div hidden={phase !== "BasicInformation"}>
            <div className="form-grid">
              <Field label="نام جایگاه">
                <Input name="name" defaultValue={draft.name} required />
              </Field>
              <Field label="شهر">
                <Input name="city" defaultValue={draft.city} required />
              </Field>
              <Field label="آستانه اختلاف مالی · ریال">
                <Input
                  name="tolerance"
                  inputMode="numeric"
                  defaultValue={draft.discrepancy_tolerance_irr}
                />
              </Field>
              <Field label="آستانه اختلاف تحویل · لیتر">
                <Input
                  name="deliveryTolerance"
                  inputMode="decimal"
                  defaultValue={draft.delivery_tolerance_ml / 1000}
                />
              </Field>
            </div>
          </div>
          <div hidden={phase !== "Configuration"}>
            <div className="editor-section">
              <h3>مخازن</h3>
              {draft.tanks.map((tank) => (
                <div className="editor-row" key={tank.id}>
                  <Field label="نام">
                    <Input
                      name={`tank-name-${tank.id}`}
                      defaultValue={tank.name}
                    />
                  </Field>
                  <Field label="سوخت">
                    <Select
                      name={`fuel-${tank.id}`}
                      defaultValue={tank.fuel_type_id}
                    >
                      {draft.fuel_types.map((fuel) => (
                        <option key={fuel.id} value={fuel.id}>
                          {fuel.name}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <Field label="ظرفیت · لیتر">
                    <Input
                      name={`capacity-${tank.id}`}
                      defaultValue={tank.capacity_ml / 1000}
                    />
                  </Field>
                  <Field label="آستانه پایین · لیتر">
                    <Input
                      name={`low-${tank.id}`}
                      defaultValue={tank.low_threshold_ml / 1000}
                    />
                  </Field>
                </div>
              ))}
              <Button
                type="button"
                size="small"
                variant="secondary"
                onClick={() =>
                  setDraft((value) => ({
                    ...value,
                    tanks: [
                      ...value.tanks,
                      {
                        id: crypto.randomUUID(),
                        name: "مخزن جدید",
                        fuel_type_id: value.fuel_types[0]?.id ?? "petrol",
                        capacity_ml: 50000000,
                        low_threshold_ml: 5000000,
                        calibration_reference: null,
                      },
                    ],
                  }))
                }
              >
                <Plus size={14} />
                افزودن مخزن
              </Button>
            </div>
            <div className="editor-section">
              <h3>سکوها</h3>
              {draft.pumps.map((pump) => (
                <Field key={pump.id} label="نام سکو">
                  <Input name={`pump-${pump.id}`} defaultValue={pump.name} />
                </Field>
              ))}
              <Button
                type="button"
                size="small"
                variant="secondary"
                onClick={() =>
                  setDraft((value) => ({
                    ...value,
                    pumps: [
                      ...value.pumps,
                      { id: crypto.randomUUID(), name: "سکوی جدید" },
                    ],
                  }))
                }
              >
                <Plus size={14} />
                افزودن سکو
              </Button>
            </div>
            <div className="editor-section">
              <h3>نازل‌ها و قیمت تاریخی هر لیتر</h3>
              {draft.nozzles.map((nozzle) => (
                <div className="editor-box" key={nozzle.id}>
                  <div className="form-grid">
                    <Field label="نام نازل">
                      <Input
                        name={`nozzle-${nozzle.id}`}
                        defaultValue={nozzle.name}
                      />
                    </Field>
                    <Field label="سکو">
                      <Select
                        name={`pump-id-${nozzle.id}`}
                        defaultValue={nozzle.pump_id}
                      >
                        {draft.pumps.map((pump) => (
                          <option key={pump.id} value={pump.id}>
                            {pump.name}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field label="مخزن">
                      <Select
                        name={`tank-id-${nozzle.id}`}
                        defaultValue={nozzle.tank_id}
                      >
                        {draft.tanks.map((tank) => (
                          <option key={tank.id} value={tank.id}>
                            {tank.name}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field label="دوره گردش کنتور · لیتر">
                      <Input
                        name={`modulus-${nozzle.id}`}
                        defaultValue={nozzle.meter_modulus_ml / 1000}
                      />
                    </Field>
                  </div>
                  <label className="check-option">
                    <input
                      type="checkbox"
                      name={`active-${nozzle.id}`}
                      defaultChecked={nozzle.active}
                    />
                    نازل فعال است
                  </label>
                  {nozzle.prices.map((price) => (
                    <div className="form-grid" key={price.id}>
                      <Field label="نام تعرفه">
                        <Input
                          name={`price-name-${nozzle.id}-${price.id}`}
                          defaultValue={price.name}
                        />
                      </Field>
                      <Field label="ریال برای هر لیتر">
                        <Input
                          name={`price-${nozzle.id}-${price.id}`}
                          defaultValue={price.irr_per_liter}
                        />
                      </Field>
                    </div>
                  ))}
                </div>
              ))}
              <Button
                type="button"
                size="small"
                variant="secondary"
                disabled={!draft.tanks.length || !draft.pumps.length}
                onClick={() =>
                  setDraft((value) => ({
                    ...value,
                    nozzles: [
                      ...value.nozzles,
                      {
                        id: crypto.randomUUID(),
                        name: "نازل جدید",
                        pump_id: value.pumps[0].id,
                        tank_id: value.tanks[0].id,
                        meter_modulus_ml: 1000000000,
                        active: true,
                        prices: [
                          {
                            id: "quota",
                            name: "سهمیه‌ای",
                            irr_per_liter: 15000,
                          },
                          { id: "free", name: "آزاد", irr_per_liter: 30000 },
                        ],
                      },
                    ],
                  }))
                }
              >
                <Plus size={14} />
                افزودن نازل
              </Button>
            </div>
          </div>
          <div hidden={phase !== "Review"} className="review-summary">
            <Shield />
            <h3>آماده ثبت تنظیمات</h3>
            <p>
              {number(draft.tanks.length)} مخزن، {number(draft.pumps.length)}{" "}
              سکو و {number(draft.nozzles.length)} نازل تعریف شده است. تغییرات
              در تاریخچه ثبت می‌شود و قیمت شیفت‌های قبلی را تغییر نمی‌دهد.
            </p>
          </div>
          {phase !== "BasicInformation" && (
            <Button
              type="button"
              variant="ghost"
              onClick={() => void controller.current?.previousPhase()}
            >
              <ArrowRight size={15} />
              مرحله قبل
            </Button>
          )}
        </Form>
      </Modal>
    </>
  );
}
function Shield() {
  return <Settings2 size={36} />;
}
