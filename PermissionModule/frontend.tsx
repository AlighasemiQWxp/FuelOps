import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Pencil, Plus, ShieldCheck } from "lucide-react";
import {
  api,
  scope,
  useCommand,
  useResource,
  type Entity,
} from "../web/src/api";
import {
  Button,
  ErrorNotice,
  Field,
  Form,
  Input,
  Loading,
  Modal,
  MultiSelect,
  PageHeader,
  Panel,
} from "../web/src/ui";
import { number } from "../web/src/localization";
export interface Role {
  name: string;
  permissions: string[];
}
const names: Record<string, string> = {
  station: "جایگاه",
  employee: "کارکنان",
  permission: "دسترسی",
  shift: "شیفت",
  inventory: "موجودی",
  delivery: "تحویل",
  reconciliation: "تطبیق",
  alert: "رویداد",
  dashboard: "داشبورد",
  report: "گزارش",
  audit: "تاریخچه",
  view: "مشاهده",
  manage: "مدیریت",
  create: "ایجاد",
  update: "ویرایش",
  delete: "غیرفعال‌سازی",
  open: "باز کردن",
  close: "بستن",
  approve: "تأیید",
  export: "خروجی",
};
const permissionName = (permission: string) =>
  permission
    .split(".")
    .map((part) => names[part] ?? part)
    .join(" · ");
export default function Permissions() {
  const query = useResource<Role>("roles");
  const catalog = useQuery({
    queryKey: [scope(), "permissions"],
    queryFn: () => api<{ permissions: string[] }>("/permissions"),
  });
  const command = useCommand("roles");
  const [editing, setEditing] = useState<Entity<Role> | null>(null);
  const [open, setOpen] = useState(false);
  return (
    <>
      <PageHeader
        title="نقش‌ها و دسترسی‌ها"
        description="برای هر مسئولیت، دسترسی روشن و متناسب تعریف کنید."
        actions={
          <Button
            onClick={() => {
              setEditing(null);
              command.clearError();
              setOpen(true);
            }}
          >
            <Plus size={17} />
            نقش جدید
          </Button>
        }
      />
      <ErrorNotice message={query.error?.message || catalog.error?.message} />
      {query.isPending ? (
        <Loading />
      ) : (
        <div className="role-grid">
          {query.data?.items.map((role) => (
            <Panel key={role.id} className="role-card">
              <span className="role-symbol">
                <ShieldCheck size={24} />
              </span>
              <h2>{role.data.name}</h2>
              <p>{number(role.data.permissions.length)} مجوز فعال</p>
              <div className="permission-tags">
                {role.data.permissions.slice(0, 6).map((permission) => (
                  <span key={permission}>{permissionName(permission)}</span>
                ))}
                {role.data.permissions.length > 6 && (
                  <span>+{number(role.data.permissions.length - 6)}</span>
                )}
              </div>
              <Button
                variant="secondary"
                onClick={() => {
                  setEditing(role);
                  command.clearError();
                  setOpen(true);
                }}
              >
                <Pencil size={15} />
                ویرایش دسترسی‌ها
              </Button>
            </Panel>
          ))}
        </div>
      )}
      <Modal
        open={open}
        onClose={() => setOpen(false)}
        title={editing ? "ویرایش نقش" : "تعریف نقش جدید"}
        description="فقط مجوزهایی را می‌توانید اعطا کنید که خودتان در اختیار دارید."
      >
        <Form
          key={editing?.id ?? "new"}
          error={command.error}
          pending={command.pending}
          onSave={async (data) => {
            const result = await command.execute(
              editing ? `/roles/${editing.id}` : "/roles",
              {
                name: data.get("name"),
                permissions: data.getAll("permissions"),
                version: editing?.version,
              },
              editing ? "PUT" : "POST",
            );
            if (result) setOpen(false);
          }}
        >
          <Field label="نام نقش">
            <Input name="name" defaultValue={editing?.data.name} required />
          </Field>
          <MultiSelect
            name="permissions"
            items={
              catalog.data?.permissions.map((permission) => ({
                id: permission,
                name: permissionName(permission),
              })) ?? []
            }
            selected={editing?.data.permissions}
          />
        </Form>
      </Modal>
    </>
  );
}
