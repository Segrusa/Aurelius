// The status bar under every screen (ticket #50): left the board member's Claude usage, right the
// server, its CPU and memory, who is busy and what waits for a slot.

import { Popover } from "@astryxdesign/core/Popover";
import type { ActivityBusy, ActivityOverview, ActivityWaiting, TicketActivity } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import { type ButtonHTMLAttributes, forwardRef, type ReactNode, useMemo, useState } from "react";
import { useLinkState } from "../connection";
import { useActivityOverview, useChannels, useMachineLoad, useSubscription, useWorkSessions } from "../data";
import { Icon } from "../ui/icons";
import {
  activityCounts,
  machineFigures,
  serverLine,
  ticketActivityLine,
  waitLabel,
  workHref,
  workLabel,
  workPlaces,
} from "./activity-bar-logic";
import { Eyebrow } from "./org-kit";
import { when } from "./settings-kit";
import { subscriptionFor } from "./usage-logic";

const styles = stylex.create({
  bar: {
    display: "flex",
    flexShrink: 0,
    height: 24,
    alignItems: "center",
    justifyContent: "flex-end",
    gap: 12,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--color-border)",
    paddingInline: 8,
  },
  trigger: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    cursor: "pointer",
    borderWidth: 0,
    borderRadius: 4,
    paddingInline: 6,
    height: 20,
    fontSize: 12,
    color: "var(--color-text-secondary)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  figure: {
    display: "flex",
    alignItems: "center",
    gap: 4,
    fontSize: 12,
    color: "var(--color-text-secondary)",
    fontVariantNumeric: "tabular-nums",
    whiteSpace: "nowrap",
  },
  // The usage sits at the left end; everything after it is pushed to the right.
  usage: { marginInlineEnd: "auto", textDecoration: "none" },
  usageOn: { backgroundColor: "var(--color-overlay-hover)" },
  usageBar: { width: 48, height: 3, borderRadius: 2, backgroundColor: "var(--color-track)", overflow: "hidden" },
  usageFill: { height: 3, borderRadius: 2, backgroundColor: "var(--color-text-accent)" },
  quietText: { color: "var(--precision-text3)" },
  dot: { width: 6, height: 6, borderRadius: "50%", backgroundColor: "var(--precision-text3)" },
  dotBusy: { backgroundColor: "var(--color-text-accent)" },
  panel: { display: "flex", flexDirection: "column", gap: 12, maxHeight: 420, overflowY: "auto" },
  section: { display: "flex", flexDirection: "column", gap: 6 },
  row: { display: "flex", flexDirection: "column", fontSize: 12 },
  link: {
    textAlign: "start",
    cursor: "pointer",
    borderWidth: 0,
    borderRadius: 4,
    marginInline: -6,
    paddingBlock: 4,
    paddingInline: 6,
    font: "inherit",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  who: { color: "var(--color-text-primary)" },
  quiet: { color: "var(--color-text-secondary)" },
  empty: { margin: 0, fontSize: 12, color: "var(--color-text-secondary)" },
  ticketLine: {
    fontSize: 12,
    color: "var(--color-text-secondary)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  ticketSlot: { color: "var(--color-text-warning)" },
  ticketRunning: { color: "var(--color-text-accent)" },
});

const rowKey = (row: ActivityBusy | ActivityWaiting): string =>
  `${row.employee_id ?? "assistant"}|${row.since_ms}|${row.work.label ?? ""}|${"waits_for" in row ? row.waits_for : "busy"}`;

/** A row opens where its work is; another board member's chat has nowhere to go. */
function Row({ href, onOpen, children }: { href: string | null; onOpen: (href: string) => void; children: ReactNode }) {
  if (!href) return <div {...stylex.props(styles.row)}>{children}</div>;
  return (
    <button type="button" onClick={() => onOpen(href)} {...stylex.props(styles.row, styles.link)}>
      {children}
    </button>
  );
}

function BusyRow({ row }: { row: ActivityBusy }) {
  return (
    <>
      <span {...stylex.props(styles.who)}>
        {row.employee_name}
        {row.employee_paused ? " (paused, finishes this run)" : ""}
      </span>
      <span {...stylex.props(styles.quiet)}>{workLabel(row.work)}</span>
      <span {...stylex.props(styles.quiet)}>Since {when(row.since_ms)}</span>
    </>
  );
}

function WaitingRow({ row }: { row: ActivityWaiting }) {
  return (
    <>
      <span {...stylex.props(styles.who)}>{workLabel(row.work)}</span>
      <span
        {...stylex.props(styles.quiet)}
      >{`${row.employee_name}: ${waitLabel(row.waits_for, row.employee_name)}`}</span>
      <span {...stylex.props(styles.quiet)}>Since {when(row.since_ms)}</span>
    </>
  );
}

function Details({ overview, onOpen }: { overview: ActivityOverview; onOpen: (href: string) => void }) {
  const sessions = useWorkSessions().data;
  const channels = useChannels().data;
  const places = useMemo(() => workPlaces(sessions ?? [], channels ?? []), [sessions, channels]);
  return (
    <div {...stylex.props(styles.panel)}>
      <section {...stylex.props(styles.section)}>
        <Eyebrow>BUSY</Eyebrow>
        {overview.busy.length === 0 ? <p {...stylex.props(styles.empty)}>Nobody is working right now.</p> : null}
        {overview.busy.map((row) => (
          <Row key={rowKey(row)} href={workHref(row.work, places)} onOpen={onOpen}>
            <BusyRow row={row} />
          </Row>
        ))}
      </section>
      <section {...stylex.props(styles.section)}>
        <Eyebrow>IN LINE</Eyebrow>
        {overview.waiting.length === 0 ? <p {...stylex.props(styles.empty)}>Nothing is waiting.</p> : null}
        {overview.waiting.map((row) => (
          <Row key={rowKey(row)} href={workHref(row.work, places)} onOpen={onOpen}>
            <WaitingRow row={row} />
          </Row>
        ))}
      </section>
    </div>
  );
}

const Trigger = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { busy: number; waiting: number }
>(function Trigger({ busy, waiting, ...props }, ref) {
  return (
    <button ref={ref} type="button" {...props} {...stylex.props(styles.trigger)}>
      <span {...stylex.props(styles.dot, busy > 0 && styles.dotBusy)} />
      {`${busy} busy · ${waiting} waiting for a slot`}
    </button>
  );
});

/** The server's name and version, then its CPU and memory where the server measures them. */
function Machine() {
  const { hostName, daemonVersion } = useLinkState();
  const load = useMachineLoad().data;
  const figures = load ? machineFigures(load) : null;
  return (
    <>
      <span {...stylex.props(styles.figure)} title="Server">
        <Icon name="server" size={13} />
        {serverLine(hostName, daemonVersion)}
      </span>
      {figures ? (
        <>
          <span {...stylex.props(styles.figure)} title={figures.cpuHint}>
            <Icon name="cpu" size={13} />
            {figures.cpu}
          </span>
          <span {...stylex.props(styles.figure)} title={figures.memoryHint}>
            <Icon name="memory" size={13} />
            {figures.memory}
          </span>
        </>
      ) : null}
    </>
  );
}

/** The limit that resets furthest away (the week, when the plan has one); a click opens Usage. */
function Usage() {
  const { user } = useLinkState();
  const on = useRouterState({ select: (s) => s.location.pathname === "/usage" });
  const data = subscriptionFor(useSubscription().data, user?.id ?? null);
  const limit = data?.limits.find((l) => /week/i.test(l.label)) ?? data?.limits[0];
  if (!limit) return null;
  const percent = Math.max(0, Math.min(100, Math.round(limit.percent)));
  const hint = limit.resets
    ? `${limit.label}: ${percent}% used, resets ${limit.resets}`
    : `${limit.label}: ${percent}% used`;
  return (
    <Link
      to="/usage"
      title={hint}
      aria-label={hint}
      {...stylex.props(styles.trigger, styles.usage, on && styles.usageOn)}
    >
      <span>{limit.label}</span>
      <span {...stylex.props(styles.usageBar)}>
        <span {...stylex.props(styles.usageFill)} style={{ display: "block", width: `${percent}%` }} />
      </span>
      <span {...stylex.props(styles.figure)}>{percent}%</span>
      {limit.resets ? <span {...stylex.props(styles.quietText)}>{`· resets ${limit.resets}`}</span> : null}
    </Link>
  );
}

/** The activity part is hidden on a server without the feature (no data ever comes). */
export function ActivityBar() {
  const { status } = useLinkState();
  const overview = useActivityOverview();
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const openWork = (href: string) => {
    setOpen(false);
    void navigate({ href });
  };
  if (status !== "online") return null;
  const counts = overview.data ? activityCounts(overview.data) : null;
  return (
    <div {...stylex.props(styles.bar)}>
      <Usage />
      <Machine />
      {overview.data && counts ? (
        <Popover
          label="Activity"
          isOpen={open}
          onOpenChange={setOpen}
          width={340}
          placement="above"
          alignment="end"
          content={<Details overview={overview.data} onOpen={openWork} />}
        >
          <Trigger busy={counts.busy} waiting={counts.waitingForSlot} />
        </Popover>
      ) : null}
    </div>
  );
}

function stateStyle(a: TicketActivity) {
  if (a.state === "running") return styles.ticketRunning;
  if (a.state === "waiting_slot" || a.state === "paused") return styles.ticketSlot;
  return null;
}

/** Where a ticket stands now; nothing on an older server or when nothing goes on. */
export function TicketActivityLine({ ticketId }: { ticketId: string }) {
  const overview = useActivityOverview();
  const found = overview.data?.tickets.find((t) => t.ticket_id === ticketId);
  if (!found) return null;
  return <span {...stylex.props(styles.ticketLine, stateStyle(found))}>{ticketActivityLine(found)}</span>;
}
