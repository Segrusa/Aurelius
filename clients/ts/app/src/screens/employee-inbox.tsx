// The Inbox and Memory tabs of the employee page (U6): what is in its mailbox, and what it remembers.

import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Link } from "@astryxdesign/core/Link";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { expectBody } from "@aurelius/client";
import type { EmployeeQueueItem, MemoryView, ThreadPlace } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import { toast } from "../ui/toast";
import { firstInLine, queueState, queueWhat } from "./activity-logic";
import { Entry, ReviseDialog } from "./memory";
import { Confirm, Eyebrow, failureOf, Hint, Loading, Problem } from "./org-kit";
import { ScratchpadSection } from "./scratchpad";
import { when } from "./settings-kit";

const styles = stylex.create({
  column: { display: "flex", flexDirection: "column", gap: 12 },
  table: { fontSize: 12 },
  headCell: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  primary: { color: "var(--color-text-primary)" },
  quiet: { color: "var(--color-text-secondary)" },
  entries: { display: "flex", flexDirection: "column" },
});

function placeLink(place: ThreadPlace) {
  if (place.ticket_id) return <Link href={`/board?ticket=${place.ticket_id}`}>{place.label}</Link>;
  if (place.channel_id) return <Link href={`/chat?channel=${place.channel_id}`}>{place.label}</Link>;
  if (place.chat_id) return <Link href={`/talk/${place.chat_id}`}>{place.label}</Link>;
  return place.label;
}

function Item({ item, next }: { item: EmployeeQueueItem; next: boolean }) {
  return (
    <TableRow>
      <TableCell xstyle={styles.primary}>{placeLink(item.place)}</TableCell>
      <TableCell xstyle={styles.quiet}>{queueWhat(item)}</TableCell>
      <TableCell xstyle={styles.quiet}>{queueState(item, next, when)}</TableCell>
      <TableCell xstyle={styles.quiet}>{when(item.created_at_ms)}</TableCell>
    </TableRow>
  );
}

export function EmployeeInboxTab({ employeeId, name, logs }: { employeeId: string; name: string; logs: boolean }) {
  const connection = useConnection();
  const scheduled = useLinkState().features.includes("scheduled_messages");
  const queue = useKeyed(
    ["employees", "queue", employeeId],
    async () =>
      expectBody(await connection.call("employee.queue", { employee_id: employeeId }), "employee_queue").value,
  );
  const items = queue.data?.items ?? [];
  const next = firstInLine(items);
  return (
    <div {...stylex.props(styles.column)}>
      {queue.isPending ? <Loading rows={2} /> : null}
      {queue.error ? <Problem>{`The inbox did not load: ${failureOf(queue.error)}`}</Problem> : null}
      {queue.data && items.length === 0 ? (
        <EmptyState
          isCompact
          title={`Nothing is waiting for ${name}.`}
          description="Messages, tasks and handoffs for this employee wait here until a run picks them up."
        />
      ) : null}
      {items.length > 0 ? (
        <Table density="compact" xstyle={styles.table}>
          <TableHeader>
            <TableRow isHeaderRow>
              <TableHeaderCell xstyle={styles.headCell}>Where</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>What</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>State</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>Since</TableHeaderCell>
            </TableRow>
          </TableHeader>
          <TableBody>
            {items.map((item) => (
              <Item key={item.id} item={item} next={item.id === next} />
            ))}
          </TableBody>
        </Table>
      ) : null}
      {logs ? <Link href={`/settings/activity?employee=${employeeId}`}>What {name} did: the activity log</Link> : null}
      {scheduled ? <Link href="/scheduled">Scheduled messages</Link> : null}
    </div>
  );
}

export function EmployeeMemoryTab({ employeeId, name }: { employeeId: string; name: string }) {
  const connection = useConnection();
  const client = useQueryClient();
  const memory = useKeyed(
    ["memory", "employee", employeeId],
    async () => expectBody(await connection.call("memory.list", { employee_id: employeeId }), "memory").value.items,
  );
  const [revising, setRevising] = useState<MemoryView | null>(null);
  const [forgetting, setForgetting] = useState<MemoryView | null>(null);
  const items = memory.data ?? [];
  const proposals = items.filter((m) => m.status === "proposed");
  const active = items.filter((m) => m.status === "active");
  const act = async (run: () => Promise<unknown>, done: string) => {
    try {
      await run();
      await client.invalidateQueries({ queryKey: ["memory"] });
      toast(done);
    } catch (e) {
      toast(failureOf(e));
    }
  };
  const decide = (m: MemoryView, accept: boolean) =>
    act(
      () => connection.write("memory.decide", { memory_id: m.id, accept }, { retry: false }),
      accept ? "Proposal accepted; it is used as context from now on." : "Proposal rejected.",
    );
  return (
    <div {...stylex.props(styles.column)}>
      <ScratchpadSection employeeId={employeeId} />
      {memory.isPending ? <Loading rows={2} /> : null}
      {memory.error ? <Problem>{`The memory did not load: ${failureOf(memory.error)}`}</Problem> : null}
      {memory.data && items.length === 0 ? (
        <Hint>{name} has no memories of its own yet. What it learns on its work shows here.</Hint>
      ) : null}
      {proposals.length > 0 ? (
        <section aria-label="Proposals" {...stylex.props(styles.entries)}>
          <Eyebrow>PROPOSALS</Eyebrow>
          {proposals.map((m) => (
            <Entry key={m.id} m={m} selected={false} onSelect={() => undefined}>
              <Button size="sm" variant="primary" label="Accept" onClick={() => void decide(m, true)} />
              <Button size="sm" label="Reject" onClick={() => void decide(m, false)} />
            </Entry>
          ))}
        </section>
      ) : null}
      {active.length > 0 ? (
        <section aria-label="In use" {...stylex.props(styles.entries)}>
          <Eyebrow>IN USE</Eyebrow>
          {active.map((m) => (
            <Entry key={m.id} m={m} selected={false} onSelect={() => undefined}>
              <Button size="sm" label="Revise" onClick={() => setRevising(m)} />
              <Button size="sm" variant="destructive" label="Forget…" onClick={() => setForgetting(m)} />
            </Entry>
          ))}
        </section>
      ) : null}
      {revising ? <ReviseDialog m={revising} onClose={() => setRevising(null)} /> : null}
      {forgetting ? (
        <Confirm
          danger
          title="Forget this memory?"
          body="It is never used as context again, leaves the search index and its content is erased. The timeline keeps the fact that it existed."
          action="Forget"
          onClose={() => setForgetting(null)}
          onConfirm={() => {
            const m = forgetting;
            setForgetting(null);
            void act(
              () => connection.write("memory.forget", { memory_id: m.id }, { retry: false }),
              "Forgotten: no longer in context or the search index; content erased.",
            );
          }}
        />
      ) : null}
    </div>
  );
}
