// A run where its work is talked about: its card under a post, the card that asks the board member,
// and what the thread used so far.

import { Button } from "@astryxdesign/core/Button";
import { ChatToolCalls } from "@astryxdesign/core/Chat";
import { Collapsible } from "@astryxdesign/core/Collapsible";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { StatusDot } from "@astryxdesign/core/StatusDot";
import { expectBody } from "@aurelius/client";
import type { ActivityView, MessageView, TokenUsage } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { type ComponentType, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import { hidden } from "../hidden";
import { toast } from "../ui/toast";
import { Loading, Problem } from "./org-kit";
import { odometerEmpty, odometerItems, odometerLine, runLine, runState } from "./run-card-logic";
import { ScratchpadButton } from "./scratchpad";
import { stepLine } from "./tasks-logic";

const styles = stylex.create({
  card: {
    marginTop: 6,
    display: "flex",
    alignItems: "flex-start",
    gap: 8,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--precision-line2)",
    paddingBlock: 2,
    paddingInline: 8,
  },
  fold: { minWidth: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  head: { display: "inline-flex", alignItems: "center", gap: 8, fontSize: 12 },
  state: { fontWeight: 500, color: "var(--color-text-primary)" },
  line: { color: "var(--precision-text3)" },
  transcript: { paddingBottom: 6 },
  none: { fontSize: 12, color: "var(--precision-text3)" },
  odometer: { fontSize: 12, color: "var(--color-text-secondary)", fontVariantNumeric: "tabular-nums" },
});

/** One tool call's result, as the screen that shows the card renders it. */
export type CallOutput = ComponentType<{ step: ActivityView; messageId: string }>;

function Transcript({ run, Output }: { run: string; Output: CallOutput }) {
  const connection = useConnection();
  const activity = useKeyed(
    ["messages", "run-activity", run],
    async () => expectBody(await connection.call("run.get_activity", { run_id: run }), "run_activity").value,
  );
  if (activity.isPending) return <Loading />;
  if (activity.error) return <Problem>{activity.error.message}</Problem>;
  const calls = activity.data?.calls ?? [];
  if (calls.length === 0) return <span {...stylex.props(styles.none)}>No tool calls.</span>;
  return (
    <ChatToolCalls
      label="Tool calls"
      defaultIsExpanded
      calls={calls.map((step) => ({
        name: step.tool,
        status: step.ok == null ? ("running" as const) : step.ok ? ("complete" as const) : ("error" as const),
        target: stepLine(step),
        key: `${step.call_id}${step.offset}`,
        ...(step.ok === false && step.output ? { errorMessage: step.output } : {}),
        resultDetail: <Output step={step} messageId={activity.data?.message_id ?? ""} />,
      }))}
    />
  );
}

/** Folded: state, calls and tokens; opened: what the run did, call by call. */
export function RunCard({ m, Output }: { m: MessageView; Output: CallOutput }) {
  const connection = useConnection();
  const { features } = useLinkState();
  const [open, setOpen] = useState(false);
  const brief = m.run;
  const run = m.run_id;
  if (!brief || !run) return null;
  const state = runState(brief.status, Boolean(m.awaiting_user));
  const stop = () =>
    connection
      .write("run.stop", { run_id: run }, { retry: false })
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  return (
    <div data-run-card {...stylex.props(styles.card)}>
      <Collapsible
        isOpen={open}
        onOpenChange={setOpen}
        chevronPosition="start"
        xstyle={styles.fold}
        trigger={
          <span {...stylex.props(styles.head)}>
            <StatusDot variant={state.variant} label={state.label} isPulsing={state.running} />
            <span {...stylex.props(styles.state)}>{state.label}</span>
            <span {...stylex.props(styles.line)}>{runLine(brief, m.usage)}</span>
          </span>
        }
      >
        {open ? (
          <div {...stylex.props(styles.transcript)}>
            <Transcript run={run} Output={Output} />
          </div>
        ) : null}
      </Collapsible>
      {brief.employee_id && features.includes("employee_scratchpad") && !hidden("RunScratchpad") ? (
        <ScratchpadButton employeeId={brief.employee_id} />
      ) : null}
      {state.running ? <Button size="sm" label="Cancel" onClick={() => void stop()} /> : null}
    </div>
  );
}

function useOdometer(place: { ticketId?: string | null; conversationId?: string | null }): TokenUsage | null {
  const connection = useConnection();
  const { features } = useLinkState();
  const { ticketId = null, conversationId = null } = place;
  const totals = useKeyed(
    ["usage", "odometer", ticketId, conversationId],
    async () =>
      expectBody(
        await connection.call("usage.get_totals", { ticket_id: ticketId, conversation_id: conversationId }),
        "usage_totals",
      ).value,
    features.includes("run_cards") && Boolean(ticketId || conversationId),
  );
  const u = totals.data?.total;
  return u && !odometerEmpty(u) ? u : null;
}

/** What a thread used so far: a ticket's, in its head. */
export function TicketOdometer({ ticketId }: { ticketId: string }) {
  const u = useOdometer({ ticketId });
  if (!u) return null;
  return (
    <MetadataList orientation="horizontal" columns="multi">
      {odometerItems(u).map((i) => (
        <MetadataListItem key={i.label} label={i.label}>
          {i.value}
        </MetadataListItem>
      ))}
    </MetadataList>
  );
}

/** The same in one line, for a channel's head. */
export function ChannelOdometer({ conversationId }: { conversationId: string | null | undefined }) {
  const u = useOdometer({ conversationId: conversationId ?? null });
  if (!u) return null;
  return <span {...stylex.props(styles.odometer)}>{odometerLine(u)}</span>;
}
