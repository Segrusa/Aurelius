// The Code tab of a workspace (ticket #77): the git state of the project's folder. Its worktrees, its branches
// with where each stands against main, the newest commits of all of them as a graph, and beside it the
// details of one branch with what can be done with it (merge, open its ticket, remove its worktree, delete it).
// What it decides is in code-logic.ts; git itself runs on the server.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Heading } from "@astryxdesign/core/Heading";
import { LayoutHeader } from "@astryxdesign/core/Layout";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import { expectBody } from "@aurelius/client";
import type { CodeBranch, CodeView, MethodParams } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { Icon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/marks";
import { useWorkspace } from "../workspace";
import {
  ago,
  BRANCH_FILTERS,
  type BranchFilter,
  type BranchStatus,
  branchPalette,
  branchStatus,
  CODE_REFRESH_MS,
  CODE_VIEW,
  changesText,
  codeOf,
  type DangerAction,
  dangerAction,
  dirtyText,
  edgePath,
  fetchedText,
  filterBranches,
  graphHeight,
  laneX,
  layoutGraph,
  lostCommitsText,
  primaryAction,
  repoName,
  rowY,
  STATUS_LABEL,
  selectedBranch,
  shortHash,
  type WorktreeCard,
  worktreeCards,
} from "./code-logic";
import { Confirm, failureOf, Loading, Page } from "./org-kit";
import { ticketPlace } from "./ticket-session-logic";

type Palette = (name: string | null) => string;

/** Merge and fetch run git against the repository or a remote: they may take longer than a read. */
const GIT_WRITE_TIMEOUT_MS = 120_000;
const MONO = "var(--font-family-code)";

const styles = stylex.create({
  head: { display: "flex", minHeight: 44, alignItems: "center", gap: 10, paddingInlineStart: 18, paddingInlineEnd: 12 },
  title: { margin: 0, fontSize: 13, fontWeight: 500 },
  repo: {
    paddingInline: 8,
    paddingBlock: 2,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
    fontFamily: MONO,
    fontSize: 12,
    color: "var(--color-text-secondary)",
  },
  muted: { fontSize: 12, color: "var(--precision-text3)", fontVariantNumeric: "tabular-nums" },
  path: { minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  spacer: { flexGrow: 1 },
  body: { display: "flex", minHeight: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  scroll: { flexGrow: 1, minWidth: 0, overflowY: "auto", paddingBlock: 20, paddingInline: 24 },
  main: { display: "flex", flexDirection: "column", gap: 24, maxWidth: 860, marginInline: "auto" },
  section: { display: "flex", flexDirection: "column", gap: 10 },
  sectionHead: { display: "flex", alignItems: "center", gap: 8, minHeight: 30 },
  h2: { margin: 0, fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)" },
  // Worktrees
  cards: { display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))", gap: 10 },
  card: {
    display: "flex",
    flexDirection: "column",
    gap: 10,
    paddingBlock: 12,
    paddingInline: 14,
    borderRadius: 12,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border)", ":hover": "var(--color-border-emphasized)" },
    backgroundColor: "var(--color-background-card)",
    color: "inherit",
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
    minWidth: 0,
  },
  cardOn: { borderColor: "var(--color-border-success)", backgroundColor: "var(--color-overlay-hover)" },
  line: { display: "flex", alignItems: "center", gap: 8, width: "100%", minWidth: 0 },
  dot: { width: 8, height: 8, borderRadius: "50%", flexShrink: 0 },
  dotSmall: { width: 6, height: 6 },
  dotBig: { width: 10, height: 10 },
  mono: {
    fontFamily: MONO,
    fontSize: 12,
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  strong: { color: "var(--color-text-primary)" },
  faint: { color: "var(--precision-text3)" },
  small: { fontSize: 11 },
  meta: { display: "flex", alignItems: "center", gap: 8, width: "100%", fontSize: 12, minWidth: 0 },
  owner: { color: "var(--color-text-secondary)", minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" },
  working: { display: "inline-flex", alignItems: "center", gap: 5, flexShrink: 0 },
  numbers: {
    display: "flex",
    gap: 12,
    width: "100%",
    fontSize: 11.5,
    color: "var(--color-text-secondary)",
    fontVariantNumeric: "tabular-nums",
  },
  ahead: { color: "var(--color-text-success)" },
  behind: { color: "var(--color-text-warning)" },
  // Branches
  table: {
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 12,
    overflow: "hidden",
  },
  grid: {
    display: "grid",
    gridTemplateColumns: "minmax(0, 2.2fr) minmax(0, 1fr) 96px minmax(0, 1.6fr) 112px",
    gap: 12,
    alignItems: "center",
    paddingInline: 14,
  },
  tableHead: {
    paddingBlock: 8,
    backgroundColor: "var(--precision-sunk)",
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    fontSize: 11,
    fontWeight: 500,
    color: "var(--precision-text3)",
  },
  row: {
    width: "100%",
    paddingBlock: 10,
    borderWidth: 0,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: "inherit",
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  rowFirst: { borderTopWidth: 0 },
  rowOn: { backgroundColor: "var(--color-overlay-hover)" },
  cell: { display: "flex", alignItems: "center", gap: 8, minWidth: 0 },
  wt: {
    flexShrink: 0,
    paddingInline: 4,
    borderRadius: 4,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    fontSize: 10.5,
    color: "var(--color-text-secondary)",
  },
  vs: { display: "flex", alignItems: "center", gap: 6, fontSize: 11.5, fontVariantNumeric: "tabular-nums" },
  vsAhead: { minWidth: 22 },
  last: { display: "flex", flexDirection: "column", gap: 1, minWidth: 0 },
  subject: {
    fontSize: 12.5,
    color: "var(--color-text-secondary)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  empty: { margin: 0, padding: 14, fontSize: 12.5, color: "var(--precision-text3)" },
  // Status pills: the theme's muted backgrounds with their own text colour.
  pill: {
    justifySelf: "start",
    alignSelf: "flex-start",
    display: "inline-flex",
    alignItems: "center",
    height: 22,
    paddingInline: 8,
    borderRadius: 999,
    fontSize: 11.5,
    fontWeight: 500,
    whiteSpace: "nowrap",
  },
  pill_working: { backgroundColor: "var(--color-success-muted)", color: "var(--color-text-success)" },
  pill_ready: { backgroundColor: "var(--color-info-muted)", color: "var(--color-text-info)" },
  pill_behind: { backgroundColor: "var(--color-warning-muted)", color: "var(--color-text-warning)" },
  pill_fresh: { backgroundColor: "var(--color-background-muted)", color: "var(--color-text-info)" },
  pill_merged: { backgroundColor: "var(--color-background-muted)", color: "var(--precision-text3)" },
  pill_stale: { backgroundColor: "var(--color-background-muted)", color: "var(--precision-text3)" },
  pill_default: { backgroundColor: "var(--color-background-muted)", color: "var(--color-text-secondary)" },
  // Commits
  commits: {
    position: "relative",
    paddingBlock: 4,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 12,
    overflow: "hidden",
  },
  svg: { position: "absolute", insetInlineStart: 6, top: 0 },
  commit: { display: "flex", alignItems: "center", gap: 12, height: 40, paddingInlineEnd: 14 },
  commitText: { display: "flex", alignItems: "center", gap: 8, flexGrow: 1, minWidth: 0 },
  tag: {
    flexShrink: 0,
    paddingInline: 7,
    lineHeight: "18px",
    borderRadius: 999,
    borderWidth: 1,
    borderStyle: "solid",
    backgroundColor: "transparent",
    fontFamily: MONO,
    fontSize: 11,
    cursor: "pointer",
  },
  author: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    width: 96,
    flexShrink: 0,
    fontSize: 12,
    color: "var(--color-text-secondary)",
    overflow: "hidden",
    whiteSpace: "nowrap",
  },
  hash: { width: 58, flexShrink: 0, fontFamily: MONO, fontSize: 11.5, color: "var(--precision-text3)" },
  when: { width: 72, flexShrink: 0, textAlign: "end", fontSize: 11.5, color: "var(--precision-text3)" },
  // Details
  aside: { display: "flex", flexDirection: "column", width: 340, flexShrink: 0, padding: 12, paddingInlineStart: 0 },
  details: {
    display: "flex",
    flexDirection: "column",
    gap: 16,
    flexGrow: 1,
    minHeight: 0,
    overflowY: "auto",
    paddingBlock: 16,
    paddingInline: 18,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 14,
    backgroundColor: "var(--color-background-card)",
  },
  detailsHead: { display: "flex", flexDirection: "column", gap: 8 },
  detailsName: {
    margin: 0,
    flexGrow: 1,
    minWidth: 0,
    fontFamily: MONO,
    fontSize: 13,
    fontWeight: 500,
    color: "var(--color-text-primary)",
    wordBreak: "break-all",
  },
  props: { display: "flex", flexDirection: "column", gap: 9 },
  prop: { display: "flex", gap: 8, fontSize: 12.5 },
  propKey: { width: 96, flexShrink: 0, color: "var(--color-text-secondary)" },
  propValue: { flexGrow: 1, minWidth: 0, wordBreak: "break-all", fontVariantNumeric: "tabular-nums" },
  propMono: { fontFamily: MONO, fontSize: 12 },
  block: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    paddingTop: 14,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
  },
  h3: { margin: 0, fontSize: 12, fontWeight: 600, color: "var(--color-text-secondary)" },
  own: { display: "flex", alignItems: "baseline", gap: 8, fontSize: 12.5 },
  ownHash: { flexShrink: 0, fontFamily: MONO, fontSize: 11, color: "var(--precision-text3)" },
  ownSubject: { flexGrow: 1, minWidth: 0, color: "var(--color-text-secondary)" },
  center: { display: "flex", justifyContent: "center", paddingBlock: 48 },
  failed: { display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 10 },
});

/** The project's git state: read on open, every 15 s while the page is visible, and when the window comes back. */
function useCode(projectId: string | null, live: boolean) {
  const connection = useConnection();
  const { status, user, features } = useLinkState();
  const query = useQuery({
    queryKey: ["code", projectId],
    enabled: projectId !== null && features.includes(CODE_VIEW) && status === "online" && user !== null,
    staleTime: live ? 0 : 30_000,
    // Paused while the document is hidden (TanStack's default), so only while visible.
    refetchInterval: live ? CODE_REFRESH_MS : false,
    queryFn: async () =>
      expectBody(await connection.call("code.get", { project_id: projectId as string }), "code").value,
  });
  const { refetch } = query;
  useEffect(() => {
    if (!live) return;
    // A desktop window can regain focus without the document's visibility changing.
    const onFocus = () => void refetch();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [live, refetch]);
  return query;
}

/** The sidebar's count beside Code: the worktrees besides the project's folder. */
export function useCodeWorktreeCount(): number | null {
  const { id } = useWorkspace();
  const code = useCode(id, false);
  const n = code.data?.problem ? 0 : (code.data?.worktrees ?? []).filter((w) => !w.is_main).length;
  return id === null || !code.data ? null : n;
}

type CodeWrite = "code.fetch" | "code.merge" | "code.remove_worktree" | "code.delete_branch" | "code.prune_merged";

/** One git write at a time; its answer is the new view, a refusal stays on the page as text. */
function useCodeWrite(projectId: string | null) {
  const connection = useConnection();
  const client = useQueryClient();
  const [busy, setBusy] = useState<CodeWrite | null>(null);
  const [error, setError] = useState<string | null>(null);
  const run = async <M extends CodeWrite>(method: M, params: MethodParams[M]) => {
    if (busy || projectId === null) return;
    setBusy(method);
    setError(null);
    try {
      const receipt = await connection.write(method, params, { retry: false, timeoutMs: GIT_WRITE_TIMEOUT_MS });
      const view = codeOf(receipt.result);
      if (view) client.setQueryData(["code", projectId], view);
      else void client.invalidateQueries({ queryKey: ["code", projectId] });
    } catch (e) {
      setError(failureOf(e));
      void client.invalidateQueries({ queryKey: ["code", projectId] });
    } finally {
      setBusy(null);
    }
  };
  return { busy, error, clearError: () => setError(null), run };
}

function StatusPill({ status }: { status: BranchStatus }) {
  return <span {...stylex.props(styles.pill, styles[`pill_${status}`])}>{STATUS_LABEL[status]}</span>;
}

function Dot({ color, size }: { color: string; size?: "small" | "big" }) {
  return (
    <span
      aria-hidden="true"
      {...stylex.props(styles.dot, size === "small" && styles.dotSmall, size === "big" && styles.dotBig)}
      style={{ background: color }}
    />
  );
}

function Owner({ branch }: { branch: CodeBranch | null }) {
  const owner = branch?.owner;
  if (!owner) return <span {...stylex.props(styles.faint)}>—</span>;
  return (
    <>
      <Mark letter={initial(owner.name)} color={markColor(owner.employee_id)} size={16} radius={4} />
      <span {...stylex.props(styles.owner)}>{owner.name}</span>
    </>
  );
}

function AheadBehind({ ahead, behind }: { ahead: number; behind: number }) {
  return (
    <span>
      <span {...stylex.props(styles.ahead)}>↑{ahead}</span> <span {...stylex.props(styles.behind)}>↓{behind}</span>
    </span>
  );
}

function Worktrees({
  cards,
  color,
  selected,
  onSelect,
  onCleanUp,
  busy,
}: {
  cards: WorktreeCard[];
  color: Palette;
  selected: string | null;
  onSelect: (branch: string) => void;
  onCleanUp: () => void;
  busy: boolean;
}) {
  return (
    <section aria-label="Worktrees" {...stylex.props(styles.section)}>
      <div {...stylex.props(styles.sectionHead)}>
        <h2 {...stylex.props(styles.h2)}>Worktrees</h2>
        <span {...stylex.props(styles.muted)}>{cards.length} active</span>
        <span {...stylex.props(styles.spacer)} />
        <Button
          size="sm"
          label="Clean up merged"
          icon={<Icon name="trash" size={14} />}
          isDisabled={busy}
          onClick={onCleanUp}
        />
      </div>
      {cards.length === 0 ? (
        <p {...stylex.props(styles.empty)}>No worktrees besides the project's folder.</p>
      ) : (
        <div {...stylex.props(styles.cards)}>
          {cards.map(({ worktree: w, branch: b }) => {
            const name = w.branch ?? "detached HEAD";
            const on = w.branch !== null && w.branch !== undefined && w.branch === selected;
            return (
              <button
                key={w.path}
                type="button"
                aria-pressed={on}
                onClick={() => w.branch && onSelect(w.branch)}
                {...stylex.props(styles.card, on && styles.cardOn)}
              >
                <span {...stylex.props(styles.line)}>
                  <Dot color={color(w.branch ?? null)} />
                  <span {...stylex.props(styles.mono, styles.strong)}>{name}</span>
                </span>
                <span title={w.path} {...stylex.props(styles.mono, styles.faint, styles.small)}>
                  {w.path}
                </span>
                <span {...stylex.props(styles.meta)}>
                  <Owner branch={b} />
                  {b?.ticket ? <span {...stylex.props(styles.faint)}>#{b.ticket.number}</span> : null}
                  <span {...stylex.props(styles.spacer)} />
                  <span
                    {...stylex.props(styles.working)}
                    style={{ color: b?.working ? "var(--color-text-success)" : "var(--precision-text3)" }}
                  >
                    <Dot color="currentColor" size="small" />
                    {b?.working ? "Working" : "Idle"}
                  </span>
                </span>
                <span {...stylex.props(styles.numbers)}>
                  <span>{dirtyText(w)}</span>
                  <span {...stylex.props(styles.spacer)} />
                  {b ? <AheadBehind ahead={b.ahead} behind={b.behind} /> : null}
                </span>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}

function Branches({
  branches,
  color,
  now,
  selected,
  onSelect,
}: {
  branches: CodeBranch[];
  color: Palette;
  now: number;
  selected: string | null;
  onSelect: (branch: string) => void;
}) {
  const [filter, setFilter] = useState<BranchFilter>("active");
  const shown = filterBranches(branches, filter, now);
  return (
    <section aria-label="Branches" {...stylex.props(styles.section)}>
      <div {...stylex.props(styles.sectionHead)}>
        <h2 {...stylex.props(styles.h2)}>Branches</h2>
        <span {...stylex.props(styles.muted)}>{branches.length}</span>
        <span {...stylex.props(styles.spacer)} />
        <SegmentedControl
          label="Filter branches"
          size="sm"
          value={filter}
          onChange={(v) => setFilter(v as BranchFilter)}
        >
          {BRANCH_FILTERS.map((f) => (
            <SegmentedControlItem key={f.value} value={f.value} label={f.label} />
          ))}
        </SegmentedControl>
      </div>
      <div {...stylex.props(styles.table)}>
        <div {...stylex.props(styles.grid, styles.tableHead)}>
          <span>Branch</span>
          <span>Owner</span>
          <span>vs main</span>
          <span>Last commit</span>
          <span>Status</span>
        </div>
        {shown.length === 0 ? <p {...stylex.props(styles.empty)}>No branches in this filter.</p> : null}
        {shown.map((b, i) => {
          const status = branchStatus(b, now);
          const quiet = status === "merged" || status === "stale";
          return (
            <button
              key={b.name}
              type="button"
              aria-pressed={b.name === selected}
              onClick={() => onSelect(b.name)}
              {...stylex.props(
                styles.grid,
                styles.row,
                i === 0 && styles.rowFirst,
                b.name === selected && styles.rowOn,
              )}
            >
              <span {...stylex.props(styles.cell)}>
                <Dot color={color(b.name)} />
                <span title={b.name} {...stylex.props(styles.mono, quiet ? styles.faint : styles.strong)}>
                  {b.name}
                </span>
                {b.worktree_path && !b.is_main ? (
                  <span title="Has a worktree" {...stylex.props(styles.wt)}>
                    wt
                  </span>
                ) : null}
              </span>
              <span {...stylex.props(styles.cell)}>
                <Owner branch={b} />
              </span>
              <span {...stylex.props(styles.vs)}>
                <span {...stylex.props(styles.ahead, styles.vsAhead)}>↑{b.ahead}</span>
                <span {...stylex.props(styles.behind)}>↓{b.behind}</span>
              </span>
              <span {...stylex.props(styles.last)}>
                <span title={b.subject} {...stylex.props(styles.subject)}>
                  {b.subject}
                </span>
                <span {...stylex.props(styles.muted, styles.small)}>{ago(b.at_ms, now)}</span>
              </span>
              <StatusPill status={status} />
            </button>
          );
        })}
      </div>
    </section>
  );
}

function Commits({
  view,
  color,
  now,
  onSelect,
}: {
  view: CodeView;
  color: Palette;
  now: number;
  onSelect: (branch: string) => void;
}) {
  const main = view.main_branch ?? null;
  const commits = view.commits ?? [];
  const graph = useMemo(() => layoutGraph(commits, main), [commits, main]);
  const height = graphHeight(graph.rows.length);
  const width = laneX(graph.lanes - 1) + 12;
  // The text starts right of the widest lane, never left of the design's 88 px.
  const indent = Math.max(88, 6 + width + 10);
  const known = new Set((view.branches ?? []).map((b) => b.name));
  return (
    <section aria-label="Commits" {...stylex.props(styles.section)}>
      <div {...stylex.props(styles.sectionHead)}>
        <h2 {...stylex.props(styles.h2)}>Commits</h2>
        <span {...stylex.props(styles.muted)}>All branches</span>
      </div>
      {commits.length === 0 ? (
        <p {...stylex.props(styles.empty)}>No commits yet.</p>
      ) : (
        <div {...stylex.props(styles.commits)}>
          <svg
            aria-hidden="true"
            width={width}
            height={height}
            viewBox={`0 0 ${width} ${height}`}
            {...stylex.props(styles.svg)}
          >
            {graph.edges.map((e) => (
              <path
                key={`${e.fromRow}-${e.toRow}-${e.via}`}
                d={edgePath(e)}
                stroke={color(e.branch)}
                strokeWidth={2}
                fill="none"
                strokeLinecap="round"
              />
            ))}
            {graph.rows.map((r, i) => {
              const c = color(r.branch);
              return (
                <circle
                  key={r.commit.hash}
                  cx={laneX(r.lane)}
                  cy={rowY(i)}
                  r={r.merge ? 6 : 4.5}
                  // Main's commits are rings, a branch's are filled, as in the design.
                  fill={r.lane === 0 && r.branch === main ? "var(--color-background-body)" : c}
                  stroke={c}
                  strokeWidth={2}
                />
              );
            })}
          </svg>
          {graph.rows.map((r) => {
            const c = r.commit;
            return (
              <div key={c.hash} {...stylex.props(styles.commit)} style={{ paddingInlineStart: indent }}>
                <span {...stylex.props(styles.commitText)}>
                  {(c.branches ?? []).map((name) => {
                    const tint = color(name);
                    return (
                      <button
                        key={name}
                        type="button"
                        title={`Show ${name}`}
                        disabled={!known.has(name)}
                        onClick={() => onSelect(name)}
                        {...stylex.props(styles.tag)}
                        style={{ color: tint, borderColor: tint }}
                      >
                        {name}
                      </button>
                    );
                  })}
                  <span title={c.subject} {...stylex.props(styles.subject, styles.strong)}>
                    {c.subject}
                  </span>
                </span>
                <span title={c.author} {...stylex.props(styles.author)}>
                  <Mark letter={initial(c.author)} color={markColor(c.author)} size={16} radius={4} />
                  {c.author}
                </span>
                <span {...stylex.props(styles.hash)}>{shortHash(c.hash)}</span>
                <span {...stylex.props(styles.when)}>{ago(c.at_ms, now)}</span>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

function Prop({ name, children, mono, faint }: { name: string; children: ReactNode; mono?: boolean; faint?: boolean }) {
  return (
    <div {...stylex.props(styles.prop)}>
      <span {...stylex.props(styles.propKey)}>{name}</span>
      <span {...stylex.props(styles.propValue, mono && styles.propMono, faint ? styles.faint : styles.strong)}>
        {children}
      </span>
    </div>
  );
}

/** A button whose reason for being off shows on hover (a disabled button gets no hover itself). */
function WithReason({ why, children }: { why: string | null; children: ReactNode }) {
  if (!why) return <>{children}</>;
  return (
    <Tooltip content={why} delay={300}>
      <span>{children}</span>
    </Tooltip>
  );
}

function Details({
  branch,
  color,
  view,
  now,
  busy,
  onMerge,
  onOpenTicket,
  onDanger,
}: {
  branch: CodeBranch;
  color: Palette;
  view: CodeView;
  now: number;
  busy: boolean;
  onMerge: (b: CodeBranch) => void;
  onOpenTicket: (b: CodeBranch) => void;
  onDanger: (b: CodeBranch, action: DangerAction) => void;
}) {
  const main = view.main_branch ?? "main";
  const status = branchStatus(branch, now);
  const primary = primaryAction(branch, status, main);
  const danger = dangerAction(branch);
  const own = branch.commits ?? [];
  return (
    <aside aria-label="Branch details" {...stylex.props(styles.aside)}>
      <div {...stylex.props(styles.details)}>
        <div {...stylex.props(styles.detailsHead)}>
          <div {...stylex.props(styles.line)}>
            <Dot color={color(branch.name)} size="big" />
            <h2 {...stylex.props(styles.detailsName)}>{branch.name}</h2>
          </div>
          <StatusPill status={status} />
        </div>
        <div {...stylex.props(styles.props)}>
          <Prop name="Ticket" faint={!branch.ticket}>
            {branch.ticket ? `#${branch.ticket.number} ${branch.ticket.title}` : "None"}
          </Prop>
          <Prop name="Owner" faint={!branch.owner}>
            {branch.owner?.name ?? "None"}
          </Prop>
          <Prop name="Worktree" mono={!!branch.worktree_path} faint={!branch.worktree_path}>
            {branch.worktree_path ?? "None"}
          </Prop>
          <Prop name="vs main">
            ↑{branch.ahead} ahead · ↓{branch.behind} behind
          </Prop>
          <Prop name="Changes">{changesText(branch)}</Prop>
          <Prop name="Last commit">{ago(branch.at_ms, now)}</Prop>
        </div>
        <div {...stylex.props(styles.block)}>
          <h3 {...stylex.props(styles.h3)}>Commits on this branch</h3>
          {own.length === 0 ? (
            <span {...stylex.props(styles.muted)}>Nothing ahead of {main}.</span>
          ) : (
            own.map((c) => (
              <div key={c.hash} {...stylex.props(styles.own)}>
                <span {...stylex.props(styles.ownHash)}>{shortHash(c.hash)}</span>
                <span {...stylex.props(styles.ownSubject)}>{c.subject}</span>
              </div>
            ))
          )}
        </div>
        <div {...stylex.props(styles.block)}>
          {primary ? (
            <Button
              variant="primary"
              width="100%"
              label={primary.label}
              isDisabled={primary.kind === "merge" && busy}
              onClick={() => (primary.kind === "merge" ? onMerge(branch) : onOpenTicket(branch))}
            />
          ) : null}
          <WithReason why={danger.disabledWhy}>
            <Button
              variant="destructive"
              width="100%"
              label={danger.label}
              isDisabled={busy || danger.disabledWhy !== null}
              onClick={() => onDanger(branch, danger)}
            />
          </WithReason>
        </div>
      </div>
    </aside>
  );
}

type Asking =
  | { kind: "merge"; branch: CodeBranch }
  | { kind: "force_delete"; branch: CodeBranch }
  | { kind: "prune" }
  | null;

export function CodeScreen() {
  const navigate = useNavigate();
  const { features } = useLinkState();
  const supported = features.includes(CODE_VIEW);
  const workspace = useWorkspace();
  const projectId = workspace.id;
  const code = useCode(projectId, true);
  const write = useCodeWrite(projectId);
  const [wanted, setWanted] = useState<string | null>(null);
  const [detailsShown, setDetailsShown] = useState(true);
  const [asking, setAsking] = useState<Asking>(null);
  const now = Date.now();
  const view = code.data;
  const branches = view?.branches ?? [];
  const selected = selectedBranch(branches, wanted);
  const main = view?.main_branch ?? null;
  const busy = write.busy !== null;
  const cards = useMemo(() => (view ? worktreeCards(view) : []), [view]);
  const color = useMemo(
    () =>
      branchPalette(
        branches.map((b) => b.name),
        main,
      ),
    [branches, main],
  );

  const openTicket = (b: CodeBranch) => {
    if (b.ticket) void navigate(ticketPlace({ id: b.ticket.ticket_id, number: b.ticket.number }));
  };
  const onDanger = (b: CodeBranch, action: DangerAction) => {
    if (!projectId) return;
    if (action.kind === "prune_merged") setAsking({ kind: "prune" });
    else if (action.kind === "remove_worktree")
      void write.run("code.remove_worktree", { project_id: projectId, path: action.path });
    else if (action.force) setAsking({ kind: "force_delete", branch: b });
    else void write.run("code.delete_branch", { project_id: projectId, branch: b.name, force: false });
  };
  const confirmAsked = () => {
    if (!asking || !projectId) return;
    setAsking(null);
    if (asking.kind === "merge") void write.run("code.merge", { project_id: projectId, branch: asking.branch.name });
    else if (asking.kind === "force_delete")
      void write.run("code.delete_branch", { project_id: projectId, branch: asking.branch.name, force: true });
    else void write.run("code.prune_merged", { project_id: projectId });
  };

  const head = (
    <LayoutHeader hasDivider padding={0} data-titlebar>
      <div {...stylex.props(styles.head)}>
        <Heading level={1} xstyle={styles.title}>
          Code
        </Heading>
        {view ? (
          <>
            <span title={view.repo_path} {...stylex.props(styles.repo)}>
              {repoName(view.repo_path)}
            </span>
            <span title={view.repo_path} {...stylex.props(styles.muted, styles.path)}>
              {view.repo_path}
            </span>
          </>
        ) : null}
        <span {...stylex.props(styles.spacer)} />
        {view && !view.problem ? (
          <>
            <span {...stylex.props(styles.muted)}>{fetchedText(view, now)}</span>
            <Button
              size="sm"
              variant="secondary"
              label={write.busy === "code.fetch" ? "Fetching…" : "Fetch"}
              icon={<Icon name="refresh" size={14} />}
              isDisabled={busy || !view.has_remote}
              tooltip={view.has_remote ? "git fetch --prune" : "This repository has no remote."}
              onClick={() => projectId && void write.run("code.fetch", { project_id: projectId })}
            />
            <Button
              size="sm"
              variant="ghost"
              isIconOnly
              label={detailsShown ? "Hide details" : "Show details"}
              icon={<Icon name="properties" size={16} />}
              onClick={() => setDetailsShown(!detailsShown)}
            />
          </>
        ) : null}
      </div>
    </LayoutHeader>
  );

  let content: ReactNode;
  if (!supported) {
    content = <EmptyState isCompact title="This server does not have the Code tab yet." />;
  } else if (projectId === null) {
    content = (
      <EmptyState
        isCompact
        title="Pick a workspace"
        description="All workspaces has no single repository. Choose a workspace in the rail to see its code."
      />
    );
  } else if (code.isPending) {
    content = <Loading rows={8} />;
  } else if (!view) {
    content = (
      <div {...stylex.props(styles.failed)}>
        <Banner status="error" role="alert" title={`The repository did not load: ${failureOf(code.error)}`} />
        <Button size="sm" label="Try again" onClick={() => void code.refetch()} />
      </div>
    );
  } else if (view.problem) {
    content = (
      <EmptyState
        isCompact
        title="Nothing to show"
        description={view.problem}
        actions={
          <Button size="sm" label="Try again" isDisabled={code.isFetching} onClick={() => void code.refetch()} />
        }
      />
    );
  } else {
    content = (
      <>
        {write.error ? (
          <Banner
            status="error"
            role="alert"
            title={write.error}
            isDismissable
            dismissLabel="Dismiss"
            onDismiss={write.clearError}
          />
        ) : null}
        {code.error ? (
          <div {...stylex.props(styles.failed)}>
            <Banner status="warning" title={`Could not refresh: ${failureOf(code.error)}`} />
            <Button size="sm" label="Try again" onClick={() => void code.refetch()} />
          </div>
        ) : null}
        <Worktrees
          cards={cards}
          color={color}
          selected={selected?.name ?? null}
          onSelect={setWanted}
          onCleanUp={() => setAsking({ kind: "prune" })}
          busy={busy}
        />
        <Branches branches={branches} color={color} now={now} selected={selected?.name ?? null} onSelect={setWanted} />
        <Commits view={view} color={color} now={now} onSelect={setWanted} />
      </>
    );
  }

  const showDetails = supported && view && !view.problem && detailsShown && selected;
  return (
    <Page>
      {head}
      <div {...stylex.props(styles.body)}>
        <div {...stylex.props(styles.scroll)}>
          <div {...stylex.props(styles.main)}>{content}</div>
        </div>
        {showDetails ? (
          <Details
            branch={selected}
            color={color}
            view={view}
            now={now}
            busy={busy}
            onMerge={(b) => setAsking({ kind: "merge", branch: b })}
            onOpenTicket={openTicket}
            onDanger={onDanger}
          />
        ) : null}
      </div>
      {asking?.kind === "merge" ? (
        <Confirm
          title={`Merge ${asking.branch.name} into ${main ?? "main"}?`}
          body={`Its ${asking.branch.ahead} ${asking.branch.ahead === 1 ? "commit goes" : "commits go"} into ${main ?? "main"} with a merge commit, in the project's folder. A conflict stops the merge and leaves ${main ?? "main"} as it was.`}
          action="Merge"
          busy={busy}
          onConfirm={confirmAsked}
          onClose={() => setAsking(null)}
        />
      ) : null}
      {asking?.kind === "force_delete" ? (
        <Confirm
          danger
          title={`Delete ${asking.branch.name}?`}
          body={lostCommitsText(asking.branch)}
          action="Delete branch"
          busy={busy}
          onConfirm={confirmAsked}
          onClose={() => setAsking(null)}
        />
      ) : null}
      {asking?.kind === "prune" ? (
        <Confirm
          danger
          title="Clean up merged branches?"
          body={`Removes the clean worktrees of merged branches nobody works on, then deletes every merged branch. ${main ?? "main"} stays. Nothing that ${main ?? "main"} lacks is lost.`}
          action="Clean up merged"
          busy={busy}
          onConfirm={confirmAsked}
          onClose={() => setAsking(null)}
        />
      ) : null}
    </Page>
  );
}
