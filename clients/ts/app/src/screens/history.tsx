// The History (ticket #57): every ticket on Done, newest first and per day, with what it delivered, and
// (server feature `session_history`) every work session on the day it was last active, with what was done
// in it. What it narrows on is in the URL, as in the Activity log, so a link keeps the filtered view.

import { Button } from "@astryxdesign/core/Button";
import { DateRangeInput } from "@astryxdesign/core/DateRangeInput";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Layout, LayoutContent, LayoutHeader } from "@astryxdesign/core/Layout";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { TextInput } from "@astryxdesign/core/TextInput";
import { expectBody } from "@aurelius/client";
import type { HistoryEntry, SessionHistoryEntry } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEmployees, useProjects } from "../data";
import { useWorkspace } from "../workspace";
import {
  ALL_PROJECTS,
  emptyText,
  groupAt,
  HISTORY_PERIODS,
  HISTORY_TYPES,
  type HistoryFilters,
  type HistoryPeriod,
  type HistoryRow,
  type HistoryType,
  historyProject,
  mergeHistory,
  narrowed,
  nextPage,
  nextSessionPage,
  ownerName,
  periodRange,
  projectInUrl,
  starterName,
  summaryShown,
} from "./history-logic";
import { failureOf, Hint, Loading, Page, Problem } from "./org-kit";
import { ticketPlace } from "./ticket-session-logic";
import { useTicketWrite } from "./tickets-kit";

const ALL = "all";
/** The server feature that puts work sessions in the History. */
const SESSION_HISTORY = "session_history";
/** Typing waits this long before the URL (and so the read) follows. */
const SEARCH_DELAY_MS = 300;

const styles = stylex.create({
  head: { display: "flex", minHeight: 44, alignItems: "center", gap: 10, paddingInlineStart: 18, paddingInlineEnd: 12 },
  title: { margin: 0, fontSize: 13, fontWeight: 500 },
  body: { position: "relative", minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  fill: { position: "absolute", inset: 0 },
  scroll: { height: "100%", overflowY: "auto" },
  main: { display: "flex", flexDirection: "column", gap: 16, padding: 18, maxWidth: 920 },
  filters: { display: "flex", flexWrap: "wrap", alignItems: "flex-end", gap: 12 },
  search: { width: 240 },
  day: { display: "flex", flexDirection: "column", gap: 2 },
  dayLabel: {
    margin: 0,
    paddingBlock: 6,
    fontSize: 12,
    fontWeight: 500,
    color: "var(--precision-text3)",
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
  },
  row: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
    paddingBlock: 10,
    paddingInline: 8,
    borderRadius: 6,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  rowOn: { backgroundColor: "var(--color-overlay-hover)" },
  top: { display: "flex", alignItems: "baseline", gap: 8, minWidth: 0 },
  open: {
    display: "flex",
    alignItems: "baseline",
    gap: 8,
    minWidth: 0,
    padding: 0,
    borderWidth: 0,
    backgroundColor: "transparent",
    cursor: "pointer",
    textAlign: "start",
    color: { default: "var(--color-text-primary)", ":hover": "var(--color-text-accent)" },
  },
  number: { flexShrink: 0, color: "var(--precision-text3)", fontVariantNumeric: "tabular-nums" },
  kind: { flexShrink: 0, fontSize: 12, color: "var(--precision-text3)" },
  name: { fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  meta: { fontSize: 12, color: "var(--precision-text3)" },
  summary: {
    margin: 0,
    padding: 0,
    borderWidth: 0,
    backgroundColor: "transparent",
    textAlign: "start",
    font: "inherit",
    lineHeight: 1.45,
    color: "var(--color-text-secondary)",
    cursor: "pointer",
  },
  clamped: { display: "-webkit-box", WebkitBoxOrient: "vertical", WebkitLineClamp: 3, overflow: "hidden" },
  quiet: { margin: 0, fontStyle: "italic", color: "var(--precision-text3)" },
  failed: { display: "flex", alignItems: "center", gap: 8, color: "var(--color-text-secondary)" },
  more: { display: "flex", justifyContent: "center", paddingBlock: 8 },
});

function useHistory(filters: HistoryFilters, project: string | null, wanted: boolean) {
  const connection = useConnection();
  const { status, user } = useLinkState();
  const { employee, period, from, to, q } = filters;
  // The period counts from today: a new day gives a new key, so the list follows.
  const today = new Date().toDateString();
  return useInfiniteQuery({
    queryKey: ["tickets", "history", { project, employee, period, from, to, q, today }],
    enabled: wanted && status === "online" && user !== null,
    staleTime: 30_000,
    initialPageParam: undefined as { before_done_at_ms: number; before_number: number } | undefined,
    queryFn: async ({ pageParam }) =>
      expectBody(
        await connection.call("ticket.history", {
          ...(project ? { project_id: project } : {}),
          ...(employee ? { employee_id: employee } : {}),
          ...periodRange(filters, new Date()),
          ...(q ? { query: q } : {}),
          ...(pageParam ?? {}),
        }),
        "history",
      ).value,
    getNextPageParam: nextPage,
  });
}

/** Work sessions in the History; under "messages", so a conversation's event (a new summary) refetches it. */
function useSessionHistory(filters: HistoryFilters, project: string | null, wanted: boolean) {
  const connection = useConnection();
  const { status, user } = useLinkState();
  const { employee, period, from, to, q } = filters;
  const today = new Date().toDateString();
  return useInfiniteQuery({
    queryKey: ["messages", "session_history", { project, employee, period, from, to, q, today }],
    enabled: wanted && status === "online" && user !== null,
    staleTime: 30_000,
    initialPageParam: undefined as { before_last_activity_ms: number; before_conversation_id: string } | undefined,
    queryFn: async ({ pageParam }) =>
      expectBody(
        await connection.call("work_session.history", {
          ...(project ? { project_id: project } : {}),
          ...(employee ? { employee_id: employee } : {}),
          ...periodRange(filters, new Date()),
          ...(q ? { query: q } : {}),
          ...(pageParam ?? {}),
        }),
        "session_history",
      ).value,
    getNextPageParam: nextSessionPage,
  });
}

function Summary({
  entry,
  of,
  retry,
}: {
  entry: Pick<HistoryEntry, "summary" | "summary_state">;
  of: "ticket" | "session";
  retry: () => void;
}) {
  const [open, setOpen] = useState(false);
  const shown = summaryShown(entry, of);
  if (shown.kind === "summary") {
    return (
      <button
        type="button"
        aria-expanded={open}
        title={open ? "Show less" : "Show all"}
        onClick={() => setOpen(!open)}
        {...stylex.props(styles.summary, !open && styles.clamped)}
      >
        {shown.text}
      </button>
    );
  }
  if (shown.kind === "failed" || shown.kind === "missing") {
    return (
      <div {...stylex.props(styles.failed)}>
        <span>{shown.text}</span>
        <Button
          size="sm"
          variant="ghost"
          label={shown.kind === "missing" ? "Summarize" : "Try again"}
          onClick={retry}
        />
      </div>
    );
  }
  return <p {...stylex.props(styles.quiet)}>{shown.text}</p>;
}

function Row({
  entry,
  selected,
  onOpen,
  retry,
}: {
  entry: HistoryEntry;
  selected: boolean;
  onOpen: (entry: HistoryEntry) => void;
  retry: (id: string) => void;
}) {
  const time = new Date(entry.done_at_ms).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  return (
    <div {...stylex.props(styles.row, selected && styles.rowOn)}>
      <div {...stylex.props(styles.top)}>
        <button type="button" onClick={() => onOpen(entry)} {...stylex.props(styles.open)}>
          <span {...stylex.props(styles.number)}>#{entry.number}</span>
          <span {...stylex.props(styles.name)}>{entry.title}</span>
        </button>
      </div>
      <span {...stylex.props(styles.meta)}>
        {ownerName(entry)} · {entry.project_name} · Done at {time}
      </span>
      <Summary entry={entry} of="ticket" retry={() => retry(entry.ticket_id)} />
    </div>
  );
}

function SessionRow({
  entry,
  onOpen,
  summarize,
}: {
  entry: SessionHistoryEntry;
  onOpen: (entry: SessionHistoryEntry) => void;
  summarize: (id: string) => Promise<void>;
}) {
  const time = new Date(entry.last_activity_ms).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  return (
    <div {...stylex.props(styles.row)}>
      <div {...stylex.props(styles.top)}>
        <button type="button" onClick={() => onOpen(entry)} {...stylex.props(styles.open)}>
          <span {...stylex.props(styles.kind)}>Session</span>
          <span {...stylex.props(styles.name)}>{entry.title || "Session"}</span>
        </button>
      </div>
      <span {...stylex.props(styles.meta)}>
        {entry.employee_name} · {entry.project_name} · started by {starterName(entry)}
        {entry.branch ? ` · ${entry.branch}` : ""} · Last active at {time}
      </span>
      <Summary entry={entry} of="session" retry={() => void summarize(entry.conversation_id)} />
    </div>
  );
}

export function HistoryScreen({ filters }: { filters: HistoryFilters }) {
  const navigate = useNavigate();
  const { features } = useLinkState();
  const supported = features.includes("ticket_history");
  const withSessions = supported && features.includes(SESSION_HISTORY);
  // Without sessions on the server, the History lists tickets whatever the URL says.
  const type: HistoryType | undefined = withSessions ? filters.type : undefined;
  const workspace = useWorkspace();
  // The project in the URL, else the workspace's; "all" in the URL wins over the workspace.
  const project = historyProject(filters, workspace.id);
  const history = useHistory(filters, project, supported && type !== "sessions");
  const sessionHistory = useSessionHistory(filters, project, withSessions && type !== "tickets");
  const projects = useProjects();
  const employees = useEmployees();
  const write = useTicketWrite();

  const go = (next: HistoryFilters) => void navigate({ to: "/history", search: { ...next } });
  const change = (part: Partial<Record<keyof HistoryFilters, string | undefined>>) => {
    const merged: Record<string, string | undefined> = { ...filters, ...part };
    go(Object.fromEntries(Object.entries(merged).filter(([, v]) => v !== undefined)) as HistoryFilters);
  };
  // A ticket opens as its session (ticket #70).
  const open = (entry: HistoryEntry) => void navigate(ticketPlace({ id: entry.ticket_id, number: entry.number }));
  const clear = () => go({});
  const retry = (id: string) => void write("ticket.retry_summary", { ticket_id: id });
  const openSession = (entry: SessionHistoryEntry) =>
    void navigate({ to: "/session/$conversationId", params: { conversationId: entry.conversation_id } });
  const client = useQueryClient();
  // The server's event refetches it too; this shows "Summarizing…" at once.
  const summarize = async (id: string) => {
    if (await write("work_session.retry_summary", { conversation_id: id }))
      void client.invalidateQueries({ queryKey: ["messages", "session_history"] });
  };

  const [typed, setTyped] = useState(filters.q ?? "");
  useEffect(() => setTyped(filters.q ?? ""), [filters.q]);
  useEffect(() => {
    if (typed.trim() === (filters.q ?? "")) return;
    const timer = setTimeout(() => change({ q: typed.trim() || undefined }), SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
  });

  const tickets = type === "sessions" ? undefined : history.data;
  const sessions = type === "tickets" || !withSessions ? undefined : sessionHistory.data;
  const list = useMemo(() => mergeHistory(tickets, sessions), [tickets, sessions]);
  const groups = useMemo(() => groupAt(list.rows, (r: HistoryRow) => r.at_ms, new Date()), [list]);
  const asked = [
    ...(type === "sessions" ? [] : [history]),
    ...(type === "tickets" || !withSessions ? [] : [sessionHistory]),
  ];
  const pending = asked.some((q) => q.isPending);
  const failed = asked.find((q) => q.error);
  const loaded = asked.length > 0 && asked.every((q) => q.data);
  const fetchingMore = asked.some((q) => q.isFetchingNextPage);
  const loadMore = () => {
    if (list.more.includes("tickets")) void history.fetchNextPage();
    if (list.more.includes("sessions")) void sessionHistory.fetchNextPage();
  };
  const period: HistoryPeriod = filters.period ?? "all";
  const range = filters.from && filters.to ? { start: filters.from, end: filters.to } : null;

  return (
    <Page>
      <LayoutHeader hasDivider padding={0} data-titlebar>
        <div {...stylex.props(styles.head)}>
          <Heading level={1} xstyle={styles.title}>
            History
          </Heading>
        </div>
      </LayoutHeader>
      <div {...stylex.props(styles.body)}>
        <div {...stylex.props(styles.fill)}>
          <Layout
            height="fill"
            content={
              <LayoutContent isScrollable={false} padding={0}>
                <div {...stylex.props(styles.scroll)}>
                  <div {...stylex.props(styles.main)}>
                    {!supported ? (
                      <EmptyState isCompact title="This server does not have the History yet." />
                    ) : (
                      <>
                        <div {...stylex.props(styles.filters)}>
                          <Selector
                            label="Project"
                            size="sm"
                            value={project ?? ALL_PROJECTS}
                            onChange={(v) => change({ project: projectInUrl(v, workspace.id) })}
                            options={[
                              { value: ALL_PROJECTS, label: "All projects" },
                              ...(projects.data ?? []).map((p) => ({ value: p.id, label: p.name })),
                            ]}
                          />
                          <Selector
                            label="Employee"
                            size="sm"
                            value={filters.employee ?? ALL}
                            onChange={(v) => change({ employee: v === ALL ? undefined : v })}
                            options={[
                              { value: ALL, label: "Everyone" },
                              ...(employees.data ?? []).map((e) => ({ value: e.id, label: e.name })),
                            ]}
                          />
                          <SegmentedControl
                            label="Period"
                            size="sm"
                            value={period}
                            onChange={(v) => {
                              const next = v as HistoryPeriod;
                              change({
                                period: next === "all" ? undefined : next,
                                from: next === "custom" ? filters.from : undefined,
                                to: next === "custom" ? filters.to : undefined,
                              });
                            }}
                          >
                            {HISTORY_PERIODS.map((p) => (
                              <SegmentedControlItem key={p.value} value={p.value} label={p.label} />
                            ))}
                          </SegmentedControl>
                          {period === "custom" ? (
                            <DateRangeInput
                              label="Days"
                              size="sm"
                              value={range}
                              onChange={(r) => change({ from: r?.start, to: r?.end })}
                            />
                          ) : null}
                          {withSessions ? (
                            <SegmentedControl
                              label="Type"
                              size="sm"
                              value={type ?? "all"}
                              onChange={(v) => change({ type: v === "all" ? undefined : (v as HistoryType) })}
                            >
                              {HISTORY_TYPES.map((t) => (
                                <SegmentedControlItem key={t.value} value={t.value} label={t.label} />
                              ))}
                            </SegmentedControl>
                          ) : null}
                          <div {...stylex.props(styles.search)}>
                            <TextInput
                              label="Search"
                              size="sm"
                              placeholder="Title or summary"
                              value={typed}
                              onChange={setTyped}
                            />
                          </div>
                        </div>
                        {workspace.project && !filters.project ? (
                          <Hint>Only {workspace.project.name}. Choose All projects to see everything.</Hint>
                        ) : null}
                        {pending ? <Loading rows={6} /> : null}
                        {failed ? (
                          <div {...stylex.props(styles.failed)}>
                            <Problem>{`The History did not load: ${failureOf(failed.error)}`}</Problem>
                            <Button
                              size="sm"
                              label="Try again"
                              onClick={() => asked.forEach((q) => void q.refetch())}
                            />
                          </div>
                        ) : null}
                        {loaded && list.rows.length === 0 ? (
                          <EmptyState
                            isCompact
                            title="Nothing in this period"
                            description={emptyText(withSessions ? type : "tickets")}
                            actions={
                              narrowed(filters) ? <Button size="sm" label="Clear filters" onClick={clear} /> : undefined
                            }
                          />
                        ) : null}
                        {groups.map((g) => (
                          <section key={g.key} aria-label={g.label} {...stylex.props(styles.day)}>
                            <h2 {...stylex.props(styles.dayLabel)}>{g.label}</h2>
                            {g.entries.map((r) =>
                              r.kind === "ticket" ? (
                                <Row
                                  key={r.key}
                                  entry={r.ticket}
                                  selected={r.ticket.ticket_id === filters.ticket}
                                  onOpen={open}
                                  retry={retry}
                                />
                              ) : (
                                <SessionRow key={r.key} entry={r.session} onOpen={openSession} summarize={summarize} />
                              ),
                            )}
                          </section>
                        ))}
                        {list.more.length > 0 ? (
                          <div {...stylex.props(styles.more)}>
                            <Button size="sm" label="Load more" isDisabled={fetchingMore} onClick={loadMore} />
                          </div>
                        ) : null}
                      </>
                    )}
                  </div>
                </div>
              </LayoutContent>
            }
          />
        </div>
      </div>
    </Page>
  );
}
