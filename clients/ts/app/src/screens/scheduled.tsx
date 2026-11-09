// One page for what comes back by itself: scheduled messages and recurring tasks, and the line
// under a thread that says who waits for it.

import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Link } from "@astryxdesign/core/Link";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { expectBody } from "@aurelius/client";
import type { ScheduledMessageView, TaskScheduleView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import { hidden } from "../hidden";
import { toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import { Confirm, Eyebrow, failureOf, Hint, Loading, Page, PageHead, Problem } from "./org-kit";
import { outcomeText, scheduledLine, splitScheduled, waiterLine } from "./scheduled-logic";
import { ScheduleDialog } from "./settings-advanced";
import { Switch, when } from "./settings-kit";
import { recurrenceText } from "./settings-logic";

const styles = stylex.create({
  scroller: { minHeight: 0, flex: 1, overflowY: "auto" },
  page: { display: "flex", maxWidth: 1040, flexDirection: "column", gap: 24, padding: 24 },
  section: { display: "flex", flexDirection: "column", gap: 8 },
  head: { display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8 },
  table: { fontSize: 12 },
  headCell: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  message: { whiteSpace: "pre-wrap", overflowWrap: "anywhere", color: "var(--color-text-primary)" },
  quiet: { color: "var(--color-text-secondary)" },
  actions: { display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 8 },
  thread: { display: "flex", flexDirection: "column", gap: 2, fontSize: 12, color: "var(--precision-text3)" },
});

function placeLink(m: ScheduledMessageView) {
  if (m.ticket_id) return <Link href={`/board?ticket=${m.ticket_id}`}>{m.place}</Link>;
  if (m.channel_id) return <Link href={`/chat?channel=${m.channel_id}`}>{m.place}</Link>;
  return m.place;
}

function who(m: ScheduledMessageView): string {
  return m.from_employee === m.to_employee ? m.from_name : `${m.from_name} → ${m.to_name}`;
}

function Messages({ list }: { list: ScheduledMessageView[] }) {
  const connection = useConnection();
  const [taking, setTaking] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const takeBack = async (id: string) => {
    setBusy(true);
    try {
      await connection.write("scheduled_message.cancel", { message_id: id }, { retry: false });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    } finally {
      setBusy(false);
      setTaking(null);
    }
  };
  const { waiting, past } = splitScheduled(list);
  return (
    <>
      <section aria-label="Scheduled messages" {...stylex.props(styles.section)}>
        <Eyebrow>Scheduled messages</Eyebrow>
        {waiting.length === 0 ? (
          <EmptyState
            isCompact
            title="No messages are scheduled."
            description="Employees schedule a message to check back on something later."
          />
        ) : (
          <Table density="compact" xstyle={styles.table}>
            <TableHeader>
              <TableRow isHeaderRow>
                <TableHeaderCell xstyle={styles.headCell}>Message</TableHeaderCell>
                <TableHeaderCell xstyle={styles.headCell}>Who</TableHeaderCell>
                <TableHeaderCell xstyle={styles.headCell}>Where</TableHeaderCell>
                <TableHeaderCell xstyle={styles.headCell}>When</TableHeaderCell>
                <TableHeaderCell xstyle={styles.headCell}>{""}</TableHeaderCell>
              </TableRow>
            </TableHeader>
            <TableBody>
              {waiting.map((m) => (
                <TableRow key={m.id}>
                  <TableCell xstyle={styles.message}>{m.text}</TableCell>
                  <TableCell xstyle={styles.quiet}>{who(m)}</TableCell>
                  <TableCell xstyle={styles.quiet}>{placeLink(m)}</TableCell>
                  <TableCell xstyle={styles.quiet}>{when(m.due_at_ms)}</TableCell>
                  <TableCell>
                    <div {...stylex.props(styles.actions)}>
                      <Button size="sm" label="Cancel" onClick={() => setTaking(m.id)} />
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </section>
      {past.length > 0 ? (
        <section aria-label="What came of them" {...stylex.props(styles.section)}>
          <Eyebrow>What came of them</Eyebrow>
          <Table density="compact" xstyle={styles.table}>
            <TableHeader>
              <TableRow isHeaderRow>
                <TableHeaderCell xstyle={styles.headCell}>Message</TableHeaderCell>
                <TableHeaderCell xstyle={styles.headCell}>Who</TableHeaderCell>
                <TableHeaderCell xstyle={styles.headCell}>When</TableHeaderCell>
                <TableHeaderCell xstyle={styles.headCell}>Outcome</TableHeaderCell>
              </TableRow>
            </TableHeader>
            <TableBody>
              {past.map((m) => (
                <TableRow key={m.id}>
                  <TableCell xstyle={styles.message}>{m.text}</TableCell>
                  <TableCell xstyle={styles.quiet}>{who(m)}</TableCell>
                  <TableCell xstyle={styles.quiet}>{when(m.due_at_ms)}</TableCell>
                  <TableCell xstyle={styles.quiet}>{outcomeText(m)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </section>
      ) : null}
      {taking ? (
        <Confirm
          title="Cancel this scheduled message?"
          body="It will not go out. What it already started stays."
          action="Take back"
          danger
          busy={busy}
          onConfirm={() => void takeBack(taking)}
          onClose={() => setTaking(null)}
        />
      ) : null}
    </>
  );
}

function RecurringTasks() {
  const connection = useConnection();
  const [editing, setEditing] = useState<TaskScheduleView | "new" | null>(null);
  const workspace = useWorkspace().id;
  const list = useKeyed(
    ["scheduled", "recurring", workspace],
    async () =>
      expectBody(
        await connection.call("recurring_task.list", workspace ? { project_id: workspace } : {}),
        "task_schedules",
      ).value.schedules,
  );
  const write = (run: Promise<unknown>) =>
    run.catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  return (
    <section aria-label="Recurring tasks" {...stylex.props(styles.section)}>
      <div {...stylex.props(styles.head)}>
        <Eyebrow>Recurring tasks</Eyebrow>
        <Button size="sm" label="New recurring task" onClick={() => setEditing("new")} />
      </div>
      {list.isPending ? <Loading rows={2} /> : null}
      {list.error ? <Problem>{`The recurring tasks did not load: ${failureOf(list.error)}`}</Problem> : null}
      {list.data?.length === 0 ? <EmptyState isCompact title="No recurring tasks yet." /> : null}
      {list.data?.length ? (
        <Table density="compact" xstyle={styles.table}>
          <TableHeader>
            <TableRow isHeaderRow>
              <TableHeaderCell xstyle={styles.headCell}>Task</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>Who</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>Schedule</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>Next</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>Last</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>{""}</TableHeaderCell>
            </TableRow>
          </TableHeader>
          <TableBody>
            {list.data.map((s) => (
              <TableRow key={s.id}>
                <TableCell xstyle={styles.message}>{s.title}</TableCell>
                <TableCell xstyle={styles.quiet}>{s.employee_name}</TableCell>
                <TableCell xstyle={styles.quiet}>{recurrenceText(s.recurrence)}</TableCell>
                <TableCell xstyle={styles.quiet}>{s.paused ? "Paused" : when(s.next_run_ms)}</TableCell>
                <TableCell xstyle={styles.quiet}>{when(s.last_run_ms)}</TableCell>
                <TableCell>
                  <div {...stylex.props(styles.actions)}>
                    <Button
                      size="sm"
                      label="Run now"
                      onClick={() =>
                        void write(connection.write("recurring_task.run_now", { schedule_id: s.id }, { retry: false }))
                      }
                    />
                    <Button size="sm" label="Edit" onClick={() => setEditing(s)} />
                    <Switch
                      label={`${s.title} is active`}
                      on={!s.paused}
                      onChange={(on) =>
                        void write(
                          connection.write(
                            "recurring_task.set_paused",
                            { schedule_id: s.id, paused: !on },
                            { retry: false },
                          ),
                        )
                      }
                    />
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      ) : null}
      {editing ? (
        <ScheduleDialog schedule={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      ) : null}
    </section>
  );
}

export function ScheduledScreen() {
  const connection = useConnection();
  const { features } = useLinkState();
  const supported = features.includes("scheduled_messages");
  const workspace = useWorkspace().id;
  const list = useKeyed(
    ["scheduled", "messages", workspace],
    async () =>
      expectBody(
        await connection.call("scheduled_message.list", workspace ? { project_id: workspace } : {}),
        "scheduled_messages",
      ).value.messages,
    supported,
  );
  return (
    <Page>
      <PageHead title="Scheduled" />
      <div {...stylex.props(styles.scroller)}>
        <div {...stylex.props(styles.page)}>
          {supported ? (
            <>
              {list.isPending ? <Loading rows={2} /> : null}
              {list.error ? <Problem>{`The scheduled messages did not load: ${failureOf(list.error)}`}</Problem> : null}
              {list.data ? <Messages list={list.data} /> : null}
            </>
          ) : (
            <Hint>This server does not have scheduled messages yet.</Hint>
          )}
          {hidden("RecurringTasks") ? null : <RecurringTasks />}
        </div>
      </div>
    </Page>
  );
}

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
    ...(waiting.data?.waiters ?? []).map(waiterLine),
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
