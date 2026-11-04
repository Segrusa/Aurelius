// What the Board, My tickets and the ticket panel share: who needs what, the writes, and a card's marks.

import { expectBody } from "@aurelius/client";
import type { ColumnKind, TicketView, TimelineCard } from "@aurelius/protocol";
import { useQueries, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { useConnection, useLinkState } from "../connection";
import { useAllTasks, useCompany, useEmployees, useProjects } from "../data";
import { initial, Mark, markColor } from "../ui/kit";
import { toast } from "../ui/toast";
import {
  deadlineText,
  dispatcherGaveUp,
  dispatcherOf,
  GAVE_UP_LIMIT,
  isChoosing,
  type Needs,
  needsLine,
  needsMe,
  priorityColor,
  priorityLabel,
  ticketNeeds,
} from "./board-logic";

/** Who needs what on these tickets: one definition for the cards, the filter and the badges. */
export function useTicketNeeds(tickets: TicketView[]) {
  const connection = useConnection();
  const { user, status } = useLinkState();
  const me = user?.id ?? null;
  const company = useCompany();
  const projects = useProjects();
  const employees = useEmployees();
  const tasks = useAllTasks();
  const unowned = useMemo(() => tickets.filter((t) => t.column === "todo" && !t.owner_employee_id), [tickets]);
  // Only what the timeline says of a Todo ticket without an owner tells whether its dispatcher gave up.
  const timelines = useQueries({
    queries: unowned.map((t) => ({
      queryKey: ["tickets", "gave_up", t.id, t.version],
      enabled: status === "online" && user !== null,
      staleTime: Number.POSITIVE_INFINITY,
      queryFn: async (): Promise<TimelineCard[]> =>
        expectBody(
          await connection.call("timeline.list", { correlation_id: t.id, limit: GAVE_UP_LIMIT, activity: true }),
          "timeline",
        ).value.cards,
    })),
  });

  return useMemo(() => {
    const stopped = new Map<string, boolean>();
    unowned.forEach((t, i) => {
      const read = timelines[i];
      // A timeline that did not load: the board member is asked rather than told "is choosing…".
      stopped.set(t.id, read?.isError ? true : dispatcherGaveUp(read?.data ?? []));
    });
    const approvals = new Map((tasks.data ?? []).map((t) => [t.id, t.pending_approvals]));
    const dispatcher = (projectId: string) =>
      dispatcherOf(
        projects.data?.find((p) => p.id === projectId)?.dispatcher_employee_id,
        company.data?.company.dispatcher_employee_id,
        employees.data ?? [],
      );
    const choosing = (t: TicketView) => {
      const d = dispatcher(t.project_id);
      return isChoosing(t, d.kind === "project" || d.kind === "company", stopped.get(t.id) ?? false);
    };
    const needs = (t: TicketView): Needs | null =>
      ticketNeeds(t, me, choosing(t), t.root_task_id ? (approvals.get(t.root_task_id) ?? 0) : 0);
    return { me, dispatcher, choosing, needs, mine: (t: TicketView) => needsMe(needs(t), t, me) };
  }, [unowned, timelines, tasks.data, projects.data, company.data, employees.data, me]);
}

/** One write on a ticket: refreshes what shows it, and says what went wrong in a toast. */
export function useTicketWrite() {
  const connection = useConnection();
  const client = useQueryClient();
  return useCallback(
    async <M extends Parameters<typeof connection.write>[0]>(
      method: M,
      params: Parameters<typeof connection.write<M>>[1],
      options: { expectedVersion?: number; done?: string } = {},
    ): Promise<boolean> => {
      try {
        await connection.write(method, params, {
          retry: false,
          ...(options.expectedVersion === undefined ? {} : { expectedVersion: options.expectedVersion }),
        });
        await client.invalidateQueries({ queryKey: ["tickets"] });
        void client.invalidateQueries({ queryKey: ["tasks"] });
        if (options.done) toast(options.done);
        return true;
      } catch (e) {
        void client.invalidateQueries({ queryKey: ["tickets"] });
        toast(e instanceof Error ? e.message : "This did not work.");
        return false;
      }
    },
    [connection, client],
  );
}

const COLUMN_LOOK: Record<ColumnKind, { color: string; glyph: string }> = {
  backlog: { color: "var(--text3)", glyph: "" },
  todo: { color: "var(--text2)", glyph: "" },
  doing: { color: "var(--acc-text)", glyph: "M8 4.2a3.8 3.8 0 0 1 0 7.6z" },
  plan_review: { color: "var(--violet)", glyph: "M8 4.2a3.8 3.8 0 0 1 3.8 3.8H8z" },
  testing: { color: "var(--blue)", glyph: "M8 4.2a3.8 3.8 0 0 1 3.8 3.8A3.8 3.8 0 0 1 8 11.8z" },
  review: { color: "var(--amber)", glyph: "M8 4.2a3.8 3.8 0 1 1-3.8 3.8H8z" },
  done: { color: "var(--acc-text)", glyph: "M8 4.2a3.8 3.8 0 1 1 0 7.6a3.8 3.8 0 1 1 0-7.6z" },
};

export function ColumnGlyph({ kind }: { kind: ColumnKind }) {
  const { color, glyph } = COLUMN_LOOK[kind];
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true" className="flex-none">
      <circle cx="8" cy="8" r="6.3" fill="none" stroke={color} strokeWidth="1.6" />
      {glyph ? <path d={glyph} fill={color} /> : null}
    </svg>
  );
}

export function Tag({ color, children }: { color: string; children: string }) {
  return (
    <span className="inline-flex h-5 flex-none items-center gap-[5px] rounded-[10px] border border-line px-[7px] text-[11px] text-text2">
      <span className="size-[6px] rounded-full" style={{ background: color }} />
      {children}
    </span>
  );
}

export function NeedsTag({ needs, mine }: { needs: Needs; mine: boolean }) {
  return (
    <span className="inline-flex h-5 flex-none items-center gap-[5px] rounded-[10px] border border-amber-line px-[7px] text-[11px] text-amber">
      <span className="size-[6px] rounded-full bg-amber" />
      {needsLine(needs, mine)}
    </span>
  );
}

/** Priority (not Normal), deadline and labels, as tags. */
export function TicketTags({ t }: { t: TicketView }) {
  const offset = -new Date().getTimezoneOffset();
  const deadline = t.deadline_ms ? deadlineText(t.deadline_ms, Date.now(), t.column === "done", offset) : null;
  return (
    <>
      {t.priority !== "normal" ? <Tag color={priorityColor(t.priority)}>{priorityLabel(t.priority)}</Tag> : null}
      {deadline ? (
        <Tag
          color={deadline.tone === "over" ? "var(--red)" : deadline.tone === "soon" ? "var(--amber)" : "var(--text3)"}
        >
          {deadline.text}
        </Tag>
      ) : null}
      {t.labels.slice(0, 3).map((l) => (
        <Tag key={l} color="var(--text3)">
          {l}
        </Tag>
      ))}
    </>
  );
}

export function OwnerMark({ t, size = 18 }: { t: TicketView; size?: number }) {
  return t.owner_name ? (
    <Mark
      letter={initial(t.owner_name)}
      color={markColor(t.owner_employee_id ?? t.owner_name)}
      size={size}
      radius={5}
    />
  ) : null;
}

export const ticketNumber = (t: { number: number }) => `#${t.number}`;
