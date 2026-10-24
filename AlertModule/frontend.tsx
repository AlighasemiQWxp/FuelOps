import { useState } from "react";
import { Bell, Plus } from "lucide-react";
import { Can, usePermission } from "../AuthModule/frontend";
import type { Employee } from "../EmployeeModule/frontend";
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
import { date, label, time } from "../web/src/localization";
interface Alert {
  kind: string;
  severity: string;
  title: string;
  description: string;
  entity_id: string;
  assigned_to: string | null;
  status: string;
  resolution: string;
  source: string;
  history: { at: number; actor: string; status: string; note: string }[];
}
export default function Alerts() {
  const [page, setPage] = useState(1);
  const query = useResource<Alert>("alerts", page);
  const canEmployees = usePermission("employee.view");
  const employees = useResource<Employee>("employees", 1, canEmployees);
  const command = useCommand("alerts");
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<Entity<Alert> | null>(null);
  const [to, setTo] = useState("");
  return (
    <>
      <PageHeader
        title="هیچ موضوعی از قلم نمی‌افتد"
        description="رویدادهای عملیاتی را ثبت، به همکاران واگذار و تا رفع کامل پیگیری کنید."
        actions={
          <Can permission="alert.manage">
            <Button
              onClick={() => {
                setCreating(true);
                command.clearError();
              }}
            >
              <Plus size={17} />
              ثبت رویداد
            </Button>
          </Can>
        }
      />
      <div className="source-notice">
        <Bell size={17} />
        هشدارها بر پایه اطلاعات دستی یا شبیه‌سازی‌شده هستند؛ این سامانه پایش
        ایمنی صنعتی نیست.
      </div>
      <ErrorNotice message={query.error?.message} />
      <Panel title="رویدادهای جایگاه">
        {query.isPending ? (
          <Loading />
        ) : (
          <>
            <Table
              rows={query.data?.items ?? []}
              rowKey={(row) => row.id}
              columns={[
                {
                  title: "موضوع",
                  render: (row) => (
                    <div className="cell-stack">
                      <b>{row.data.title}</b>
                      <small>
                        {label(row.data.kind)} · {date(row.created_at)}
                      </small>
                    </div>
                  ),
                },
                {
                  title: "اولویت",
                  render: (row) => <Badge value={row.data.severity} />,
                },
                {
                  title: "مسئول",
                  render: (row) =>
                    employees.data?.items.find(
                      (employee) => employee.id === row.data.assigned_to,
                    )?.data.name ?? "تعیین نشده",
                },
                {
                  title: "وضعیت",
                  render: (row) => <Badge value={row.data.status} />,
                },
                {
                  title: "",
                  render: (row) => (
                    <div className="row-actions">
                      <Can permission="alert.manage">
                        <Button
                          variant="secondary"
                          size="small"
                          onClick={() => {
                            setSelected(row);
                            setTo(
                              row.data.status === "Open"
                                ? "Acknowledged"
                                : row.data.status === "Acknowledged"
                                  ? "Resolved"
                                  : "Open",
                            );
                            command.clearError();
                          }}
                        >
                          {row.data.status === "Open"
                            ? "پذیرش و پیگیری"
                            : row.data.status === "Acknowledged"
                              ? "ثبت رفع مشکل"
                              : "بازگشایی"}
                        </Button>
                      </Can>
                      <Button
                        variant="ghost"
                        size="small"
                        onClick={() => {
                          setSelected(row);
                          setTo("");
                        }}
                      >
                        تاریخچه
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
        title="ثبت رویداد عملیاتی"
      >
        <Form
          pending={command.pending}
          error={command.error}
          onSave={async (data) => {
            const result = await command.execute("/alerts", {
              title: data.get("title"),
              description: data.get("description"),
              kind: data.get("kind"),
              severity: data.get("severity"),
              entity_id: data.get("entity") || "",
              assigned_to: data.get("employee") || null,
            });
            if (result) setCreating(false);
          }}
        >
          <Field label="عنوان">
            <Input name="title" required />
          </Field>
          <div className="form-grid">
            <Field label="دسته‌بندی">
              <Select name="kind">
                {[
                  "nozzle",
                  "reading",
                  "inventory",
                  "overdue_shift",
                  "handover",
                  "discrepancy",
                  "delivery",
                  "other",
                ].map((kind) => (
                  <option key={kind} value={kind}>
                    {label(kind)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="اولویت">
              <Select name="severity">
                {["medium", "low", "high"].map((value) => (
                  <option key={value} value={value}>
                    {label(value)}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="مسئول پیگیری">
              <Select name="employee">
                <option value="">فعلاً تعیین نشود</option>
                {employees.data?.items
                  .filter((employee) => employee.data.active)
                  .map((employee) => (
                    <option key={employee.id} value={employee.id}>
                      {employee.data.name}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field label="شناسه تجهیز یا سند مرتبط (اختیاری)">
              <Input name="entity" />
            </Field>
          </div>
          <Field label="شرح رویداد">
            <Textarea name="description" required />
          </Field>
        </Form>
      </Modal>
      <Modal
        open={Boolean(selected)}
        onClose={() => setSelected(null)}
        title={selected?.data.title ?? "رویداد"}
      >
        {selected && (
          <>
            <p className="detail-description">{selected.data.description}</p>
            {to ? (
              <Form
                pending={command.pending}
                error={command.error}
                submitLabel={label(to)}
                onSave={async (data) => {
                  const result = await command.execute(
                    `/alerts/${selected.id}/transition`,
                    {
                      version: selected.version,
                      to,
                      note: data.get("note"),
                      assigned_to: data.get("employee") || null,
                    },
                  );
                  if (result) setSelected(null);
                }}
              >
                <Field label="مسئول پیگیری">
                  <Select
                    name="employee"
                    defaultValue={selected.data.assigned_to ?? ""}
                  >
                    <option value="">بدون مسئول</option>
                    {employees.data?.items.map((employee) => (
                      <option key={employee.id} value={employee.id}>
                        {employee.data.name}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field label="توضیح اقدام انجام‌شده">
                  <Textarea name="note" required={to === "Resolved"} />
                </Field>
              </Form>
            ) : (
              <div className="timeline">
                {selected.data.history.map((entry, index) => (
                  <div key={index}>
                    <span className="timeline-dot" />
                    <Badge value={entry.status} />
                    <p>{entry.note}</p>
                    <small>
                      {entry.actor} · {date(entry.at)}، {time(entry.at)}
                    </small>
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
