// The status bar under every screen (ticket #50): who is busy and what waits for a slot.

import { Popover } from "@astryxdesign/core/Popover";
import type { ActivityBusy, ActivityOverview, ActivityWaiting, TicketActivity } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { type ButtonHTMLAttributes, forwardRef, useState } from "react";
import { useActivityOverview } from "../data";
import { activityCounts, ticketActivityLine, waitLabel, workLabel } from "./activity-bar-logic";
import { Eyebrow } from "./org-kit";
import { when } from "./settings-kit";

const styles = stylex.create({
  bar: {
    display: "flex",
    flexShrink: 0,
    height: 24,
    alignItems: "center",
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
  dot: { width: 6, height: 6, borderRadius: "50%", backgroundColor: "var(--precision-text3)" },
  dotBusy: { backgroundColor: "var(--color-text-accent)" },
  panel: { display: "flex", flexDirection: "column", gap: 12, maxHeight: 420, overflowY: "auto" },
  section: { display: "flex", flexDirection: "column", gap: 6 },
  row: { display: "flex", flexDirection: "column", fontSize: 12 },
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

function BusyRow({ row }: { row: ActivityBusy }) {
  return (
    <div {...stylex.props(styles.row)}>
      <span {...stylex.props(styles.who)}>
        {row.employee_name}
        {row.employee_paused ? " (paused, finishes this run)" : ""}
      </span>
      <span {...stylex.props(styles.quiet)}>{workLabel(row.work)}</span>
      <span {...stylex.props(styles.quiet)}>Since {when(row.since_ms)}</span>
    </div>
  );
}

function WaitingRow({ row }: { row: ActivityWaiting }) {
  return (
    <div {...stylex.props(styles.row)}>
      <span {...stylex.props(styles.who)}>{workLabel(row.work)}</span>
      <span
        {...stylex.props(styles.quiet)}
      >{`${row.employee_name}: ${waitLabel(row.waits_for, row.employee_name)}`}</span>
      <span {...stylex.props(styles.quiet)}>Since {when(row.since_ms)}</span>
    </div>
  );
}

function Details({ overview }: { overview: ActivityOverview }) {
  return (
    <div {...stylex.props(styles.panel)}>
      <section {...stylex.props(styles.section)}>
        <Eyebrow>BUSY</Eyebrow>
        {overview.busy.length === 0 ? <p {...stylex.props(styles.empty)}>Nobody is working right now.</p> : null}
        {overview.busy.map((row) => (
          <BusyRow key={rowKey(row)} row={row} />
        ))}
      </section>
      <section {...stylex.props(styles.section)}>
        <Eyebrow>IN LINE</Eyebrow>
        {overview.waiting.length === 0 ? <p {...stylex.props(styles.empty)}>Nothing is waiting.</p> : null}
        {overview.waiting.map((row) => (
          <WaitingRow key={rowKey(row)} row={row} />
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

/** Hidden on a server without the feature (no data ever comes). */
export function ActivityBar() {
  const overview = useActivityOverview();
  const [open, setOpen] = useState(false);
  if (!overview.data) return null;
  const { busy, waitingForSlot } = activityCounts(overview.data);
  return (
    <div {...stylex.props(styles.bar)}>
      <Popover
        label="Activity"
        isOpen={open}
        onOpenChange={setOpen}
        width={340}
        placement="above"
        alignment="start"
        content={<Details overview={overview.data} />}
      >
        <Trigger busy={busy} waiting={waitingForSlot} />
      </Popover>
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
