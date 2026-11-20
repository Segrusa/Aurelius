// The History (ticket #57): every ticket on Done, newest first and per day, with what it delivered.
// What it narrows on is in the URL, as in the Activity log, so a link keeps the filtered view.

import { Button } from "@astryxdesign/core/Button";
import { DateRangeInput } from "@astryxdesign/core/DateRangeInput";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { Layout, LayoutContent, LayoutHeader } from "@astryxdesign/core/Layout";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { TextInput } from "@astryxdesign/core/TextInput";
import { expectBody } from "@aurelius/client";
import type { HistoryEntry } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEmployees, useProjects } from "../data";
import { useWorkspace } from "../workspace";
import {
  ALL_PROJECTS,
  groupByDay,
  HISTORY_PERIODS,
  type HistoryFilters,
  type HistoryPeriod,
  historyProject,
  narrowed,
  nextPage,
  ownerName,
  periodRange,
  projectInUrl,
  summaryShown,
} from "./history-logic";
import { InboxBell } from "./inbox-bell";
import { failureOf, Hint, Loading, Page, Problem } from "./org-kit";
import { ticketPlace } from "./ticket-session-logic";
import { useTicketWrite } from "./tickets-kit";

const ALL = "all";
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

function Summary({ entry, retry }: { entry: HistoryEntry; retry: () => void }) {
  const [open, setOpen] = useState(false);
  const shown = summaryShown(entry);
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
  if (shown.kind === "failed") {
    return (
      <div {...stylex.props(styles.failed)}>
        <span>{shown.text}</span>
        <Button size="sm" variant="ghost" label="Try again" onClick={retry} />
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
      <Summary entry={entry} retry={() => retry(entry.ticket_id)} />
    </div>
  );
}

export function HistoryScreen({ filters }: { filters: HistoryFilters }) {
  const navigate = useNavigate();
  const { features } = useLinkState();
  const supported = features.includes("ticket_history");
  const workspace = useWorkspace();
  // The project in the URL, else the workspace's; "all" in the URL wins over the workspace.
  const project = historyProject(filters, workspace.id);
  const history = useHistory(filters, project, supported);
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

  const [typed, setTyped] = useState(filters.q ?? "");
  useEffect(() => setTyped(filters.q ?? ""), [filters.q]);
  useEffect(() => {
    if (typed.trim() === (filters.q ?? "")) return;
    const timer = setTimeout(() => change({ q: typed.trim() || undefined }), SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
  });

  const entries = useMemo(() => (history.data?.pages ?? []).flatMap((p) => p.entries), [history.data]);
  const groups = useMemo(() => groupByDay(entries, new Date()), [entries]);
  const period: HistoryPeriod = filters.period ?? "all";
  const range = filters.from && filters.to ? { start: filters.from, end: filters.to } : null;

  return (
    <Page>
      <LayoutHeader hasDivider padding={0} data-titlebar>
        <div {...stylex.props(styles.head)}>
          <Heading level={1} xstyle={styles.title}>
            History
          </Heading>
          <InboxBell />
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
                        {history.isPending ? <Loading rows={6} /> : null}
                        {history.error ? (
                          <div {...stylex.props(styles.failed)}>
                            <Problem>{`The History did not load: ${failureOf(history.error)}`}</Problem>
                            <Button size="sm" label="Try again" onClick={() => void history.refetch()} />
                          </div>
                        ) : null}
                        {history.data && entries.length === 0 ? (
                          <EmptyState
                            isCompact
                            title="Nothing finished in this period"
                            description="A ticket shows up here once it is on Done."
                            actions={
                              narrowed(filters) ? <Button size="sm" label="Clear filters" onClick={clear} /> : undefined
                            }
                          />
                        ) : null}
                        {groups.map((g) => (
                          <section key={g.key} aria-label={g.label} {...stylex.props(styles.day)}>
                            <h2 {...stylex.props(styles.dayLabel)}>{g.label}</h2>
                            {g.entries.map((e) => (
                              <Row
                                key={e.ticket_id}
                                entry={e}
                                selected={e.ticket_id === filters.ticket}
                                onOpen={open}
                                retry={retry}
                              />
                            ))}
                          </section>
                        ))}
                        {history.hasNextPage ? (
                          <div {...stylex.props(styles.more)}>
                            <Button
                              size="sm"
                              label="Load more"
                              isDisabled={history.isFetchingNextPage}
                              onClick={() => void history.fetchNextPage()}
                            />
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
