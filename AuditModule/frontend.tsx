import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, scope, type Page } from "../web/src/api";
import {
  ErrorNotice,
  Loading,
  PageHeader,
  Pagination,
  Panel,
  Table,
} from "../web/src/ui";
import { date, label, number, time } from "../web/src/localization";
interface Audit {
  id: string;
  actor_name: string;
  entity_id: string;
  collection: string;
  action: string;
  before_version: number;
  after_version: number;
  created_at: number;
}
export default function Audit() {
  const [page, setPage] = useState(1);
  const query = useQuery({
    queryKey: [scope(), "audit", page],
    queryFn: ({ signal }) =>
      api<Page<Audit>>(`/audit?page=${page}&limit=30`, { signal }),
    refetchInterval: 30000,
  });
  return (
    <>
      <PageHeader
        title="ردپای روشن هر تغییر"
        description="چه کسی، چه زمانی و در کدام بخش تغییر ایجاد کرده است؟ همه‌چیز قابل پیگیری است."
      />
      <ErrorNotice message={query.error?.message} />
      <Panel
        title="تاریخچه فعالیت‌های جایگاه"
        subtitle="نسخه‌های قبل و بعد برای پیگیری تغییرات نگهداری می‌شوند"
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
                  title: "زمان",
                  render: (row) => (
                    <div className="cell-stack">
                      <b>{date(row.created_at)}</b>
                      <small>{time(row.created_at)}</small>
                    </div>
                  ),
                },
                { title: "همکار", render: (row) => row.actor_name },
                { title: "بخش", render: (row) => label(row.collection) },
                { title: "عملیات", render: (row) => label(row.action) },
                {
                  title: "نسخه",
                  render: (row) => (
                    <span>
                      {number(row.before_version)} ← {number(row.after_version)}
                    </span>
                  ),
                },
                {
                  title: "شناسه سند",
                  render: (row) => (
                    <span className="record-id" dir="ltr" title={row.entity_id}>
                      {row.entity_id}
                    </span>
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
    </>
  );
}
