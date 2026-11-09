// A run where its work is talked about: a note under a post, and what the thread used so far.

import { Button } from "@astryxdesign/core/Button";
import { MetadataList, MetadataListItem } from "@astryxdesign/core/MetadataList";
import { expectBody } from "@aurelius/client";
import type { MessageView, TokenUsage } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import { toast } from "../ui/toast";
import { TypingDots } from "../ui/typing";
import { odometerEmpty, odometerItems, runState } from "./run-card-logic";

const styles = stylex.create({
  note: { marginTop: 4, display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--precision-text3)" },
  failed: { marginTop: 4, fontSize: 12, color: "var(--color-text-error)" },
});

/** Under a post: only that its author is still at work, or could not finish. How it went is on the ticket's Activity. */
export function RunNote({ m }: { m: MessageView }) {
  const connection = useConnection();
  const brief = m.run;
  const run = m.run_id;
  if (!brief || !run) return null;
  if (brief.status === "failed") return <div {...stylex.props(styles.failed)}>This did not finish.</div>;
  const state = runState(brief.status, Boolean(m.awaiting_user));
  if (!state.running || m.awaiting_user) return null;
  const stop = () =>
    connection
      .write("run.stop", { run_id: run }, { retry: false })
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  return (
    <div data-run-note {...stylex.props(styles.note)}>
      <TypingDots />
      <span>Working</span>
      <Button size="sm" variant="ghost" label="Stop" onClick={() => void stop()} />
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
