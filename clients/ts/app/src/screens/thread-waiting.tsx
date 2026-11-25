// The line under a thread's title that says who waits to pick it up and what is scheduled in it.

import { expectBody } from "@aurelius/client";
import * as stylex from "@stylexjs/stylex";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import { when } from "./settings-kit";
import { scheduledLine, shownWaiters, waiterLine } from "./thread-waiting-logic";

const styles = stylex.create({
  thread: { display: "flex", flexDirection: "column", gap: 2, fontSize: 12, color: "var(--precision-text3)" },
});

/** Under a thread's title: who waits to pick it up, and what is scheduled in it. */
export function ThreadWaiting({
  ticketId = null,
  conversationId = null,
}: {
  ticketId?: string | null;
  conversationId?: string | null;
}) {
  const connection = useConnection();
  const { features } = useLinkState();
  const wanted = features.includes("scheduled_messages") && Boolean(ticketId || conversationId);
  const waiting = useKeyed(
    ["scheduled", "thread", ticketId, conversationId],
    async () =>
      expectBody(
        await connection.call("thread.waiting", { ticket_id: ticketId, conversation_id: conversationId }),
        "thread_waiting",
      ).value,
    wanted,
  );
  const lines = [
    ...shownWaiters(waiting.data?.waiters ?? []).map(waiterLine),
    ...(waiting.data?.scheduled ?? []).map((m) => scheduledLine(m, when(m.due_at_ms))),
  ];
  if (lines.length === 0) return null;
  return (
    <output aria-label="Who waits for this thread" {...stylex.props(styles.thread)}>
      {lines.map((line) => (
        <span key={line}>{line}</span>
      ))}
    </output>
  );
}
