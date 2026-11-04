// What the Employees, Knowledge and Projects screens share: the page's head, its tabs, and the confirm.

import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { Button } from "../ui/kit";
import { Dialog } from "../ui/overlay";

export const SMALL = "h-7 px-[10px] text-[12px]";
export const EYEBROW = "m-0 text-[11.5px] font-semibold tracking-[0.04em] text-text3";
export const FIELD =
  "h-8 w-full rounded-[6px] border border-btn-line bg-sunk px-2 text-text outline-none placeholder:text-text3 focus:border-acc-text focus-visible:outline-none";
export const AREA =
  "w-full resize-y rounded-[6px] border border-btn-line bg-sunk p-2 leading-[1.5] text-text outline-none placeholder:text-text3 focus:border-acc-text focus-visible:outline-none";

export function PageHead({
  title,
  sub,
  tabs,
  children,
}: {
  title: ReactNode;
  sub?: ReactNode;
  tabs?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <header className="flex min-h-11 flex-none flex-wrap items-center gap-[10px] border-b border-line py-[6px] pr-3 pl-[18px]">
      <h1 className="m-0 flex min-w-0 items-center gap-2 text-[13px] font-medium">{title}</h1>
      {sub ? <span className="text-[12px] text-text3">{sub}</span> : null}
      {tabs}
      <div className="ml-auto flex flex-wrap items-center gap-2">{children}</div>
    </header>
  );
}

export function TabPills({ label, tabs }: { label: string; tabs: { key: string; label: string; link: ReactNode }[] }) {
  return (
    <nav aria-label={label} className="flex gap-px rounded-[6px] border border-line2 bg-sunk p-[2px]">
      {tabs.map((t) => (
        <span key={t.key} className="contents">
          {t.link}
        </span>
      ))}
    </nav>
  );
}

export const PILL =
  "flex h-[22px] items-center rounded-[4px] px-2 text-[12px] font-medium whitespace-nowrap text-text2 no-underline hover:text-text";
export const PILL_ON = { className: "bg-sel !text-text" };

const KNOWLEDGE = [
  { key: "guidelines", label: "Guidelines" },
  { key: "memory", label: "Memory" },
  { key: "ideas", label: "Ideas" },
  { key: "vault", label: "Vault" },
] as const;

export function KnowledgeTabs() {
  return (
    <TabPills
      label="Knowledge"
      tabs={KNOWLEDGE.map((t) => ({
        ...t,
        link: (
          <Link to="/knowledge/$tab" params={{ tab: t.key }} className={PILL} activeProps={PILL_ON}>
            {t.label}
          </Link>
        ),
      }))}
    />
  );
}

export function EmployeesTabs() {
  return (
    <TabPills
      label="Employees"
      tabs={[
        {
          key: "list",
          label: "List",
          link: (
            <Link to="/employees" className={PILL} activeOptions={{ exact: true }} activeProps={PILL_ON}>
              List
            </Link>
          ),
        },
        {
          key: "org",
          label: "Org chart",
          link: (
            <Link to="/employees/org" className={PILL} activeProps={PILL_ON}>
              Org chart
            </Link>
          ),
        },
        {
          key: "guidelines",
          label: "Guidelines",
          link: (
            <Link to="/knowledge/$tab" params={{ tab: "guidelines" }} className={PILL}>
              Guidelines
            </Link>
          ),
        },
      ]}
    />
  );
}

export function Confirm({
  title,
  body,
  action,
  danger = false,
  busy = false,
  onConfirm,
  onClose,
}: {
  title: string;
  body: ReactNode;
  action: string;
  danger?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <Dialog open onClose={onClose} title={title} width={460}>
      <div className="flex flex-col gap-2 p-4">
        <p className="m-0 text-[15px] font-semibold">{title}</p>
        <div className="leading-[1.5] text-text2">{body}</div>
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button data-autofocus kind={danger ? "danger" : "primary"} disabled={busy} onClick={onConfirm}>
          {action}
        </Button>
      </div>
    </Dialog>
  );
}

/** "Sun 1 Nov", or with the year when it is not this one. */
export function shortDay(ms: number): string {
  const d = new Date(ms);
  const weekday = d.toLocaleDateString("en-GB", { weekday: "short" });
  const month = d.toLocaleDateString("en-GB", { month: "short" });
  const year = d.getFullYear() === new Date().getFullYear() ? "" : ` ${d.getFullYear()}`;
  return `${weekday} ${d.getDate()} ${month}${year}`;
}

export const hhmm = (ms: number) => new Date(ms).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });

export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="mx-auto flex max-w-[460px] flex-col items-center gap-3 p-10 text-center">
      <p className="m-0 text-[15px] font-semibold">{title}</p>
      {children}
    </div>
  );
}

export const failureOf = (e: unknown) => (e instanceof Error ? e.message : "This did not work.");
