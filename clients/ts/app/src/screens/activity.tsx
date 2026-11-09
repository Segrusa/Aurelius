// The activity log (R8), a section of Settings: every delivery, queue step and run step, narrowed
// per employee, thread and run. What is chosen is in the URL, so a line elsewhere can link to it.

import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { TextInput } from "@astryxdesign/core/TextInput";
import { expectBody } from "@aurelius/client";
import type { ActivityLogEntry } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEmployees } from "../data";
import { useWorkspace } from "../workspace";
import {
  ACTIVITY_CATEGORIES,
  type ActivityCategory,
  type ActivityFilters,
  categoryLabel,
  detailText,
  parseActivitySearch,
  shortRun,
  stepLabel,
} from "./activity-logic";
import { failureOf, Hint, Loading, Problem } from "./org-kit";
import { when } from "./settings-kit";

const ALL = "all";

const styles = stylex.create({
  page: { display: "flex", flexDirection: "column", gap: 12 },
  filters: { display: "flex", flexWrap: "wrap", alignItems: "flex-end", gap: 12 },
  search: { width: 280 },
  table: { fontSize: 12 },
  headCell: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  primary: { color: "var(--color-text-primary)" },
  quiet: { color: "var(--color-text-secondary)", overflowWrap: "anywhere" },
  when: { whiteSpace: "nowrap", color: "var(--precision-text3)" },
  active: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8 },
  more: { display: "flex", justifyContent: "center" },
});

function useActivityLog(filters: ActivityFilters, wanted: boolean, project: string | null) {
  const connection = useConnection();
  const { status, user } = useLinkState();
  return useInfiniteQuery({
    queryKey: ["employees", "activity_log", filters, project],
    enabled: wanted && status === "online" && user !== null,
    staleTime: 30_000,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam }) =>
      expectBody(
        await connection.call("activity_log.list", {
          ...(project ? { project_id: project } : {}),
          ...(filters.employee ? { employee_id: filters.employee } : {}),
          ...(filters.thread ? { thread_key: filters.thread } : {}),
          ...(filters.ticket ? { ticket_number: filters.ticket } : {}),
          ...(filters.run ? { run_id: filters.run } : {}),
          ...(filters.category ? { category: filters.category } : {}),
          ...(pageParam ? { before_seq: pageParam } : {}),
        }),
        "activity_log",
      ).value,
    getNextPageParam: (last) => (last.has_more ? last.entries.at(-1)?.seq : undefined),
  });
}

function Row({ entry, go }: { entry: ActivityLogEntry; go: (change: ActivityFilters) => void }) {
  const { thread_key: thread, run_id: run, place } = entry;
  return (
    <TableRow>
      <TableCell xstyle={styles.when}>{when(entry.at_ms)}</TableCell>
      <TableCell xstyle={styles.quiet}>
        {entry.employee_id ? (
          <Button
            size="sm"
            variant="ghost"
            label={entry.employee_name ?? "A removed employee"}
            onClick={() => go({ employee: entry.employee_id as string })}
          />
        ) : null}
      </TableCell>
      <TableCell xstyle={styles.quiet}>{categoryLabel(entry.category)}</TableCell>
      <TableCell xstyle={styles.primary}>{stepLabel(entry)}</TableCell>
      <TableCell xstyle={styles.quiet}>{detailText(entry.detail)}</TableCell>
      <TableCell>
        {thread ? (
          <Button size="sm" variant="ghost" label={place?.label ?? thread} onClick={() => go({ thread })} />
        ) : null}
      </TableCell>
      <TableCell>
        {run ? (
          <Button
            size="sm"
            variant="ghost"
            label={shortRun(run)}
            tooltip="Only this run"
            aria-label={`Only run ${shortRun(run)}`}
            onClick={() => go({ run })}
          />
        ) : null}
      </TableCell>
    </TableRow>
  );
}

export function ActivitySection({ filters }: { filters: ActivityFilters }) {
  const { features } = useLinkState();
  const navigate = useNavigate();
  const employees = useEmployees();
  const supported = features.includes("activity_log");
  const workspace = useWorkspace();
  const log = useActivityLog(filters, supported, workspace.id);
  const [query, setQuery] = useState("");
  const go = (next: ActivityFilters) =>
    void navigate({ to: "/settings/$section", params: { section: "activity" }, search: { ...next } });
  const change = (part: ActivityFilters) => go({ ...filters, ...part });
  const parsed = parseActivitySearch(query);
  const search = (typed: string) => {
    setQuery(typed);
    const found = parseActivitySearch(typed);
    if (!found) return;
    const { employee, category } = filters;
    go({ ...(employee ? { employee } : {}), ...(category ? { category } : {}), ...found });
  };
  const pages = log.data?.pages ?? [];
  const entries = pages.flatMap((p) => p.entries);
  const narrowed = Boolean(filters.thread || filters.run || filters.ticket);
  const thread = entries.find((e) => e.thread_key === filters.thread)?.place?.label ?? filters.thread;
  const clearNarrowing = () => {
    setQuery("");
    const { employee, category } = filters;
    go({ ...(employee ? { employee } : {}), ...(category ? { category } : {}) });
  };
  return (
    <div {...stylex.props(styles.page)}>
      {supported ? (
        <>
          {/* The switcher is not in sight here: Settings has its own sidebar. */}
          {workspace.project ? (
            <Hint>Only {workspace.project.name}. Switch to All projects to see everything.</Hint>
          ) : null}
          <div {...stylex.props(styles.filters)}>
            <Selector
              label="Employee"
              size="sm"
              value={filters.employee ?? ALL}
              onChange={(v) => {
                const { employee: _, ...rest } = filters;
                go(v === ALL ? rest : { ...rest, employee: v });
              }}
              options={[
                { value: ALL, label: "Everyone" },
                ...(employees.data ?? []).map((e) => ({ value: e.id, label: e.name })),
              ]}
            />
            <SegmentedControl
              label="Kind of step"
              size="sm"
              value={filters.category ?? ALL}
              onChange={(v) => {
                const { category: _, ...rest } = filters;
                go(v === ALL ? rest : { ...rest, category: v as ActivityCategory });
              }}
            >
              <SegmentedControlItem value={ALL} label="All" />
              {ACTIVITY_CATEGORIES.map((c) => (
                <SegmentedControlItem key={c.value} value={c.value} label={c.label} />
              ))}
            </SegmentedControl>
            <div {...stylex.props(styles.search)}>
              <TextInput
                label="Ticket, thread or run"
                size="sm"
                placeholder="#21, ticket:…, or a run id"
                value={query}
                onChange={search}
              />
            </div>
          </div>
          {parsed === null ? <Hint>Give a ticket number like #21, a thread key or a whole run id.</Hint> : null}
          {narrowed ? (
            <div {...stylex.props(styles.active)}>
              <Hint>
                {filters.ticket ? `Ticket #${filters.ticket}` : null}
                {filters.thread ? `Thread ${thread}` : null}
                {filters.run ? ` Run ${shortRun(filters.run)}` : null}
              </Hint>
              <Button size="sm" variant="ghost" label="Show all" onClick={clearNarrowing} />
            </div>
          ) : null}
          {log.isPending ? <Loading rows={4} /> : null}
          {log.error ? <Problem>{`The activity log did not load: ${failureOf(log.error)}`}</Problem> : null}
          {log.data && entries.length === 0 ? (
            <EmptyState
              isCompact
              title="Nothing here."
              description="Every message, queue step and run step of an employee shows up here."
            />
          ) : null}
          {entries.length > 0 ? (
            <Table density="compact" xstyle={styles.table}>
              <TableHeader>
                <TableRow isHeaderRow>
                  <TableHeaderCell xstyle={styles.headCell}>When</TableHeaderCell>
                  <TableHeaderCell xstyle={styles.headCell}>Who</TableHeaderCell>
                  <TableHeaderCell xstyle={styles.headCell}>Kind</TableHeaderCell>
                  <TableHeaderCell xstyle={styles.headCell}>Step</TableHeaderCell>
                  <TableHeaderCell xstyle={styles.headCell}>Detail</TableHeaderCell>
                  <TableHeaderCell xstyle={styles.headCell}>Thread</TableHeaderCell>
                  <TableHeaderCell xstyle={styles.headCell}>Run</TableHeaderCell>
                </TableRow>
              </TableHeader>
              <TableBody>
                {entries.map((e) => (
                  <Row key={e.seq} entry={e} go={change} />
                ))}
              </TableBody>
            </Table>
          ) : null}
          {log.hasNextPage ? (
            <div {...stylex.props(styles.more)}>
              <Button
                size="sm"
                label="Load older"
                isDisabled={log.isFetchingNextPage}
                onClick={() => void log.fetchNextPage()}
              />
            </div>
          ) : null}
        </>
      ) : (
        <Hint>This server does not have the activity log yet.</Hint>
      )}
    </div>
  );
}
