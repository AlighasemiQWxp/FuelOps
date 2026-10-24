import { useState } from "react";
import { Pencil, Plus } from "lucide-react";
import { Can, usePermission } from "../AuthModule/frontend";
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
  MultiSelect,
  PageHeader,
  Pagination,
  Panel,
  Table,
} from "../web/src/ui";
import type { Role } from "../PermissionModule/frontend";
import type { Station } from "../StationModule/frontend";
export interface Employee {
  name: string;
  email: string;
  active: boolean;
  station_ids: string[];
  role_ids: string[];
}
export default function Employees() {
  const [page, setPage] = useState(1);
  const query = useResource<Employee>("employees", page);
  const canRoles = usePermission("permission.manage");
  const roles = useResource<Role>("roles", 1, canRoles);
  const stations = useResource<Station>("stations");
  const command = useCommand("employees");
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<Entity<Employee> | null>(null);
  return (
    <>
      <PageHeader
        title="تیم جایگاه"
        description="حساب همکاران، جایگاه‌های مجاز و نقش‌های کاری را مدیریت کنید."
        actions={
          <Can permission="employee.create">
            <Button
              onClick={() => {
                setEditing(null);
                command.clearError();
                setOpen(true);
              }}
            >
              <Plus size={17} />
              همکار جدید
            </Button>
          </Can>
        }
      />
      <ErrorNotice message={query.error?.message} />
      <Panel
        title="همکاران"
        subtitle="دسترسی‌ها در هر درخواست توسط سرور بررسی می‌شوند"
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
                  title: "نام و حساب",
                  render: (row) => (
                    <div className="person-cell">
                      <span className="avatar">{row.data.name[0]}</span>
                      <div>
                        <b>{row.data.name}</b>
                        <small dir="ltr">{row.data.email}</small>
                      </div>
                    </div>
                  ),
                },
                {
                  title: "نقش‌های جایگاه",
                  render: (row) =>
                    roles.data?.items
                      .filter((role) => row.data.role_ids.includes(role.id))
                      .map((role) => role.data.name)
                      .join("، ") || "نقش سازمانی",
                },
                {
                  title: "وضعیت",
                  render: (row) => (
                    <Badge value={row.data.active ? "Open" : "Closed"} />
                  ),
                },
                {
                  title: "",
                  render: (row) => (
                    <Can permission="employee.update">
                      <Button
                        variant="ghost"
                        size="small"
                        onClick={() => {
                          setEditing(row);
                          command.clearError();
                          setOpen(true);
                        }}
                      >
                        <Pencil size={15} />
                        ویرایش
                      </Button>
                    </Can>
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
        open={open}
        onClose={() => setOpen(false)}
        title={editing ? "ویرایش حساب همکار" : "افزودن همکار"}
      >
        <Form
          key={editing?.id ?? "new"}
          error={command.error}
          pending={command.pending}
          onSave={async (data) => {
            const body = {
              name: data.get("name"),
              email: data.get("email"),
              password: data.get("password") || null,
              active: data.get("active") === "on",
              station_ids: data.getAll("station_ids"),
              role_ids: data.getAll("role_ids"),
              version: editing?.version,
            };
            const result = await command.execute(
              editing ? `/employees/${editing.id}` : "/employees",
              body,
              editing ? "PUT" : "POST",
            );
            if (result) setOpen(false);
          }}
        >
          <div className="form-grid">
            <Field label="نام و نام خانوادگی">
              <Input name="name" defaultValue={editing?.data.name} required />
            </Field>
            <Field label="ایمیل">
              <Input
                name="email"
                type="email"
                dir="ltr"
                defaultValue={editing?.data.email}
                required
              />
            </Field>
          </div>
          <Field
            label={editing ? "رمز عبور جدید (اختیاری)" : "رمز عبور"}
            hint="حداقل ۱۲ نویسه؛ تغییر حساب، نشست‌های قبلی را لغو می‌کند."
          >
            <Input
              name="password"
              type="password"
              autoComplete="new-password"
              minLength={12}
              required={!editing}
            />
          </Field>
          <h3>جایگاه‌های مجاز</h3>
          <MultiSelect
            name="station_ids"
            items={
              stations.data?.items.map((row) => ({
                id: row.id,
                name: row.data.name,
              })) ?? []
            }
            selected={editing?.data.station_ids ?? [scope()]}
          />
          <h3>نقش‌های جایگاه جاری</h3>
          <MultiSelect
            name="role_ids"
            items={
              roles.data?.items.map((row) => ({
                id: row.id,
                name: row.data.name,
              })) ?? []
            }
            selected={editing?.data.role_ids}
          />
          {editing?.data.role_ids
            .filter((id) => !roles.data?.items.some((role) => role.id === id))
            .map((id) => (
              <input key={id} type="hidden" name="role_ids" value={id} />
            ))}
          <label className="check-option">
            <input
              name="active"
              type="checkbox"
              defaultChecked={editing?.data.active ?? true}
            />
            حساب فعال باشد
          </label>
        </Form>
      </Modal>
    </>
  );
}
