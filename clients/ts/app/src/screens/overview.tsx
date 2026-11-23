// The Overview of a workspace (ticket #82, design workspace-overview.html): one column with the
// workspace's facts, what needs the board member, what runs, the board in five lanes, the newest designs
// and what got done. All workspaces is the list of every workspace of the company, where each board member
// pins the ones they want on their own rail.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Link } from "@astryxdesign/core/Link";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import type { ProjectSummary, TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useMemo, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import {
  useActivityOverview,
  useCompany,
  useEmployees,
  useGuidelines,
  useProjects,
  useTickets,
  useWorkSessions,
} from "../data";
import { useKeys } from "../keys/provider";
import { Icon, PlusIcon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import { dispatcherOf } from "./board-logic";
import { DesignCard, useDesigns } from "./designs";
import { designLink } from "./designs-logic";
import { groupAt } from "./history-logic";
import { failureOf, hhmm } from "./org-kit";
import {
  boardLanes,
  DESIGNS_SHOWN,
  elapsed,
  groupDigits,
  type LaneName,
  linkedGuidelines,
  NEED_ACTIONS,
  NEED_LABELS,
  type NeedCard,
  needCards,
  type RunningRow,
  recentlyDone,
  runningRows,
  styleguideName,
} from "./overview-logic";
import { useRailPins } from "./projects";
import { useTicketNeeds } from "./tickets-kit";
import { byName, openProjects } from "./workspace-logic";

// The running dot breathes, as in the sidebar.
const pulse = stylex.keyframes({
  "0%": { opacity: 1, transform: "scale(1)" },
  "50%": { opacity: 0.35, transform: "scale(0.75)" },
  "100%": { opacity: 1, transform: "scale(1)" },
});

const GAP = 24;
const ellipsis = { whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" } as const;

const styles = stylex.create({
  scroll: { minHeight: 0, flex: 1, overflowY: "auto", backgroundColor: "var(--color-background-surface)" },
  content: {
    display: "flex",
    flexDirection: "column",
    gap: GAP,
    width: "100%",
    maxWidth: 1180,
    marginInline: "auto",
    boxSizing: "border-box",
    paddingBlockStart: { default: 28, "@media (max-width: 600px)": 20 },
    paddingBlockEnd: 48,
    paddingInline: { default: 32, "@media (max-width: 600px)": 16 },
  },
  hero: { display: "flex", flexWrap: "wrap", alignItems: "flex-start", gap: 16 },
  grow: { minWidth: 0, flex: 1 },
  heroTitle: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 10,
    marginBlock: "2px 4px",
    marginInline: 0,
    fontSize: 22,
    fontWeight: 650,
    letterSpacing: "-0.01em",
  },
  heroText: { margin: 0, maxWidth: 640, lineHeight: 1.45, color: "var(--color-text-secondary)" },
  muted: { color: "var(--precision-text3)" },
  allTile: {
    display: "grid",
    placeItems: "center",
    width: 48,
    height: 48,
    flexShrink: 0,
    borderRadius: 12,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-card)",
    color: "var(--color-text-secondary)",
  },
  actions: { display: "flex", flexShrink: 0, gap: 8 },
  btn: {
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    height: 30,
    paddingInline: 12,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    backgroundColor: { default: "var(--color-background-card)", ":hover": "var(--color-overlay-hover)" },
    color: "var(--color-text-primary)",
    font: "inherit",
    fontWeight: 500,
    whiteSpace: "nowrap",
    cursor: "pointer",
  },
  primary: {
    borderColor: "transparent",
    backgroundColor: { default: "var(--color-accent)", ":hover": "var(--color-accent)" },
    color: "var(--color-on-accent)",
    filter: { default: null, ":hover": "brightness(1.08)" },
  },
  small: { height: 24, paddingInline: 10, fontSize: 12, flexShrink: 0 },
  square: { width: 30, paddingInline: 0, justifyContent: "center" },
  pill: {
    display: "inline-flex",
    alignItems: "center",
    gap: 5,
    paddingInline: 8,
    paddingBlock: 2,
    borderRadius: 10,
    fontSize: 11,
    fontWeight: 600,
    backgroundColor: "var(--color-accent-muted)",
    color: "var(--color-text-success)",
  },
  pillPaused: { backgroundColor: "var(--color-warning-muted)", color: "var(--color-text-warning)" },
  pillNeed: { backgroundColor: "var(--color-error-muted)", color: "var(--color-text-error)" },
  dot: { display: "block", width: 6, height: 6, flexShrink: 0, borderRadius: "50%" },
  dotRun: {
    backgroundColor: "var(--color-text-success)",
    animationName: { default: pulse, "@media (prefers-reduced-motion: reduce)": "none" },
    animationDuration: "1.6s",
    animationIterationCount: "infinite",
    animationTimingFunction: "ease-in-out",
  },
  sectionTitle: {
    display: "flex",
    alignItems: "baseline",
    gap: 8,
    marginBlock: "0 10px",
    marginInline: 0,
    fontSize: 11,
    fontWeight: 600,
    letterSpacing: "0.04em",
    textTransform: "uppercase",
    color: "var(--color-text-secondary)",
  },
  count: { fontWeight: 500, color: "var(--precision-text3)" },
  titleLink: {
    marginInlineStart: "auto",
    padding: 0,
    borderWidth: 0,
    backgroundColor: "transparent",
    font: "inherit",
    fontWeight: 500,
    letterSpacing: 0,
    textTransform: "none",
    cursor: "pointer",
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  card: {
    overflow: "hidden",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 8,
    backgroundColor: "var(--color-background-card)",
    boxShadow: "var(--precision-card-shadow)",
  },
  facts: {
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(4, minmax(0, 1fr))",
      "@media (max-width: 900px)": "repeat(2, minmax(0, 1fr))",
    },
  },
  fact: {
    display: "flex",
    flexDirection: "column",
    gap: 3,
    minWidth: 0,
    paddingBlock: 10,
    paddingInline: 14,
    borderInlineStartWidth: { default: 1, ":first-child": 0 },
    borderInlineStartStyle: "solid",
    borderInlineStartColor: "var(--color-border)",
  },
  factK: { fontSize: 11.5, color: "var(--precision-text3)" },
  factV: { display: "flex", alignItems: "center", gap: 6, minWidth: 0, ...ellipsis },
  mono: { fontFamily: "var(--font-family-code)", fontSize: 11.5, ...ellipsis },
  accent: { color: "var(--color-text-accent)", fontWeight: 500 },
  calm: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    paddingBlock: 14,
    paddingInline: 16,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "dashed",
    borderColor: "var(--color-border-emphasized)",
    color: "var(--color-text-secondary)",
  },
  check: { display: "flex", flexShrink: 0, color: "var(--color-text-success)" },
  needs: { display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))", gap: 8 },
  ask: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    minWidth: 0,
    paddingBlock: 8,
    paddingInlineStart: 12,
    paddingInlineEnd: 10,
    borderWidth: 1,
    borderInlineStartWidth: 3,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border)", ":hover": "var(--color-border-emphasized)" },
    borderInlineStartColor: { default: "var(--color-text-error)", ":hover": "var(--color-text-error)" },
    borderRadius: 6,
    backgroundColor: "var(--color-background-card)",
    color: "var(--color-text-primary)",
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  askReview: { borderInlineStartColor: { default: "var(--color-text-info)", ":hover": "var(--color-text-info)" } },
  askPlan: { borderInlineStartColor: { default: "var(--color-text-warning)", ":hover": "var(--color-text-warning)" } },
  askBody: { display: "flex", flexDirection: "column", gap: 1, minWidth: 0, flex: 1 },
  askKind: {
    display: "flex",
    alignItems: "center",
    gap: 5,
    fontSize: 11,
    color: "var(--precision-text3)",
    ...ellipsis,
  },
  askTitle: { fontSize: 12.5, fontWeight: 600, ...ellipsis },
  askSub: { fontSize: 12, color: "var(--color-text-secondary)", ...ellipsis },
  askAction: {
    display: "inline-flex",
    alignItems: "center",
    flexShrink: 0,
    height: 24,
    paddingInline: 10,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    backgroundColor: "var(--color-background-card)",
    fontSize: 12,
    fontWeight: 500,
  },
  row: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    width: "100%",
    paddingBlock: 10,
    paddingInline: 14,
    borderWidth: 0,
    borderBlockStartWidth: { default: 1, ":first-child": 0 },
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: "var(--color-text-primary)",
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  rowMain: { minWidth: 0, flex: 1 },
  rowT: { fontWeight: 500, ...ellipsis },
  rowS: { marginBlockStart: 1, fontSize: 12, color: "var(--color-text-secondary)", ...ellipsis },
  num: {
    fontFamily: "var(--font-family-code)",
    fontSize: 11.5,
    fontVariantNumeric: "tabular-nums",
    color: "var(--precision-text3)",
  },
  side: { fontSize: 12, whiteSpace: "nowrap", fontVariantNumeric: "tabular-nums", color: "var(--precision-text3)" },
  emptyNote: { margin: 0, padding: 14, color: "var(--precision-text3)" },
  board: {
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(5, minmax(0, 1fr))",
      "@media (max-width: 1100px)": "repeat(5, minmax(120px, 1fr))",
    },
    overflowX: { default: "hidden", "@media (max-width: 1100px)": "auto" },
  },
  lane: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    minWidth: 0,
    paddingBlock: 12,
    paddingInline: 14,
    borderWidth: 0,
    borderInlineStartWidth: { default: 1, ":first-child": 0 },
    borderInlineStartStyle: "solid",
    borderInlineStartColor: "var(--color-border)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: "var(--color-text-primary)",
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  laneHead: {
    display: "flex",
    alignItems: "baseline",
    gap: 6,
    fontSize: 12,
    fontWeight: 600,
    color: "var(--color-text-secondary)",
  },
  laneCount: {
    marginInlineStart: "auto",
    fontSize: 18,
    fontWeight: 650,
    fontVariantNumeric: "tabular-nums",
    color: "var(--color-text-primary)",
  },
  laneBar: { display: "block", height: 4, borderRadius: 2 },
  laneTicket: { fontSize: 12, color: "var(--color-text-secondary)", ...ellipsis },
  laneNum: { marginInlineEnd: 4 },
  day: {
    paddingBlock: "8px 4px",
    paddingInline: 14,
    borderBlockStartWidth: { default: 1, ":first-child": 0 },
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
    backgroundColor: "var(--color-background-muted)",
    fontSize: 11,
    fontWeight: 600,
    color: "var(--precision-text3)",
  },
  designs: { display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(200px, 1fr))", gap: 12 },
  designsEmpty: {
    padding: 14,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "dashed",
    borderColor: "var(--color-border-emphasized)",
    color: "var(--precision-text3)",
  },
  start: {
    padding: 22,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-card)",
  },
  startTitle: { marginBlock: "0 4px", marginInline: 0, fontSize: 15 },
  startText: { marginBlock: "0 16px", marginInline: 0, color: "var(--color-text-secondary)" },
  steps: {
    display: "grid",
    gridTemplateColumns: { default: "repeat(3, minmax(0, 1fr))", "@media (max-width: 900px)": "minmax(0, 1fr)" },
    gap: 12,
  },
  step: {
    display: "flex",
    flexDirection: "column",
    gap: 6,
    padding: 14,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: "var(--color-text-primary)",
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  stepStill: { cursor: "default", backgroundColor: "transparent" },
  stepN: {
    display: "grid",
    placeItems: "center",
    width: 22,
    height: 22,
    borderRadius: "50%",
    backgroundColor: "var(--color-accent-muted)",
    color: "var(--color-text-success)",
    fontSize: 12,
    fontWeight: 600,
  },
  stepDone: { backgroundColor: "var(--color-accent)", color: "var(--color-on-accent)" },
  stepName: { fontWeight: 600 },
  stepText: { fontSize: 12, lineHeight: 1.4, color: "var(--color-text-secondary)" },
  find: {
    width: "100%",
    maxWidth: 360,
    boxSizing: "border-box",
    height: 32,
    marginBlockEnd: 12,
    paddingInline: 10,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border)", ":focus": "var(--color-border-emphasized)" },
    backgroundColor: "var(--color-background-card)",
    color: "var(--color-text-primary)",
    font: "inherit",
    outline: "none",
  },
  wsList: {
    display: "flex",
    flexDirection: "column",
    margin: 0,
    padding: 0,
    listStyle: "none",
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-card)",
    overflow: "hidden",
  },
  wsRow: {
    display: "flex",
    alignItems: "center",
    gap: 12,
    paddingInlineEnd: 12,
    borderBlockStartWidth: { default: 1, ":first-child": 0 },
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
  },
  wsOpen: {
    display: "flex",
    alignItems: "center",
    gap: 12,
    minWidth: 0,
    flex: 1,
    paddingBlock: 12,
    paddingInlineStart: 14,
    borderWidth: 0,
    backgroundColor: "transparent",
    color: "var(--color-text-primary)",
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  wsName: { minWidth: 0, flex: 1, fontSize: 15, fontWeight: 600 },
  wsSmall: { display: "block", fontSize: 12, fontWeight: 400, color: "var(--precision-text3)", ...ellipsis },
  wsState: { flexShrink: 0, fontSize: 12, color: "var(--precision-text3)", whiteSpace: "nowrap" },
  wsStateNeed: { color: "var(--color-text-warning)" },
  wsStateRun: { color: "var(--color-text-accent)" },
  pinOn: { borderColor: "var(--color-text-accent)", color: "var(--color-text-accent)" },
});

/** The colour of a lane's bar, as in the design. */
const LANE_COLORS: Record<LaneName, string> = {
  Backlog: "var(--precision-text3)",
  Todo: "var(--color-text-info)",
  Doing: "var(--color-text-success)",
  Review: "var(--color-text-warning)",
  Done: "var(--color-border-emphasized)",
};

const ASK_STYLES = { question: null, plan: styles.askPlan, review: styles.askReview } as const;

function CheckMark() {
  return (
    <span {...stylex.props(styles.check)}>
      <Icon name="check" size={14} weight={2.2} />
    </span>
  );
}

function SectionTitle({
  title,
  count,
  link,
}: {
  title: string;
  count?: number;
  link?: { label: string; onClick: () => void } | null;
}) {
  return (
    <h2 {...stylex.props(styles.sectionTitle)}>
      {title}
      {count === undefined ? null : <span {...stylex.props(styles.count)}>{count}</span>}
      {link ? (
        <button type="button" onClick={link.onClick} {...stylex.props(styles.titleLink)}>
          {link.label}
        </button>
      ) : null}
    </h2>
  );
}

function Avatar({ name, id, size }: { name: string; id: string; size: number }) {
  return <Mark letter={initial(name)} color={markColor(id)} size={size} radius={size / 2} />;
}

/** Everything the Overview reads, once for both of its views. */
function useOverviewData() {
  const tickets = useTickets();
  const sessions = useWorkSessions();
  const activity = useActivityOverview();
  const all = tickets.data ?? [];
  const need = useTicketNeeds(all);
  const since = useMemo(() => {
    const at = new Map((activity.data?.tickets ?? []).map((t) => [t.ticket_id, t]));
    return {
      ms: (id: string) => at.get(id)?.since_ms ?? null,
      waitsForSlot: (id: string) => at.get(id)?.state === "waiting_slot",
    };
  }, [activity.data]);
  const needs = useMemo(() => needCards(all, sessions.data ?? [], need.needs, need.mine), [all, sessions.data, need]);
  const running = useMemo(() => runningRows(all, sessions.data ?? [], since.ms), [all, sessions.data, since]);
  return { tickets: all, sessions: sessions.data ?? [], needs, running, since };
}

function NeedsSection({ cards, projects }: { cards: NeedCard[]; projects: ProjectSummary[] | null }) {
  const navigate = useNavigate();
  const now = Date.now();
  return (
    <section aria-label="Needs you">
      <SectionTitle title="Needs you" count={cards.length} />
      {cards.length === 0 ? (
        <p {...stylex.props(styles.calm)}>
          <CheckMark />
          Nothing needs you. Your employees will ask here when they do.
        </p>
      ) : (
        <div {...stylex.props(styles.needs)}>
          {cards.map((c) => {
            const project = projects?.find((p) => p.id === c.projectId) ?? null;
            return (
              <button
                key={c.key}
                type="button"
                onClick={() => void navigate({ href: c.to })}
                {...stylex.props(styles.ask, ASK_STYLES[c.kind])}
              >
                <span {...stylex.props(styles.askBody)}>
                  <span {...stylex.props(styles.askKind)}>
                    {project ? (
                      <>
                        <Mark letter={initial(project.name)} color={markColor(project.id)} size={14} radius={4} />
                        {project.name} ·{" "}
                      </>
                    ) : null}
                    {NEED_LABELS[c.kind]} · {c.who} · {elapsed(c.atMs, now)}
                  </span>
                  <span {...stylex.props(styles.askTitle)}>{c.title}</span>
                  <span {...stylex.props(styles.askSub)}>{c.text}</span>
                </span>
                <span {...stylex.props(styles.askAction)}>{NEED_ACTIONS[c.kind]}</span>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}

function RunningSection({ rows, waitsForSlot }: { rows: RunningRow[]; waitsForSlot: (id: string) => boolean }) {
  const navigate = useNavigate();
  const now = Date.now();
  return (
    <section aria-label="Running now">
      <SectionTitle title="Running now" count={rows.length} />
      <div {...stylex.props(styles.card)}>
        {rows.length === 0 ? (
          <p {...stylex.props(styles.emptyNote)}>Nothing is running. Start a session or move a ticket to Doing.</p>
        ) : (
          rows.map((r) => {
            const slot = r.queued || waitsForSlot(r.key.slice(2));
            return (
              <button
                key={r.key}
                type="button"
                onClick={() => void navigate({ href: r.to })}
                {...stylex.props(styles.row)}
              >
                <Avatar name={r.who} id={r.whoId} size={28} />
                <span {...stylex.props(styles.rowMain)}>
                  <span {...stylex.props(styles.rowT)} style={{ display: "block" }}>
                    {r.number === null ? null : <span {...stylex.props(styles.num)}>#{r.number} </span>}
                    {r.title}
                  </span>
                  <span {...stylex.props(styles.rowS)} style={{ display: "block" }}>
                    {r.who} · {slot ? "Waiting for a slot" : "Working"}
                  </span>
                </span>
                <span {...stylex.props(styles.side)}>{elapsed(r.sinceMs, now)}</span>
              </button>
            );
          })
        )}
      </div>
    </section>
  );
}

function BoardSection({ tickets }: { tickets: TicketView[] }) {
  const navigate = useNavigate();
  const lanes = useMemo(() => boardLanes(tickets), [tickets]);
  const open = () => void navigate({ to: "/board" });
  return (
    <section aria-label="Board">
      <SectionTitle title="Board" link={{ label: "Open board →", onClick: open }} />
      <div {...stylex.props(styles.card, styles.board)}>
        {lanes.map((l) => (
          <button
            key={l.name}
            type="button"
            aria-label={`${l.name}: ${l.count}`}
            onClick={open}
            {...stylex.props(styles.lane)}
          >
            <span {...stylex.props(styles.laneHead)}>
              {l.name}
              <b {...stylex.props(styles.laneCount)}>{l.count}</b>
            </span>
            <span {...stylex.props(styles.laneBar)} style={{ background: LANE_COLORS[l.name] }} />
            {l.top.length === 0 ? (
              <span {...stylex.props(styles.laneTicket, styles.muted)}>Empty</span>
            ) : (
              l.top.map((t) => (
                <span key={t.id} {...stylex.props(styles.laneTicket)}>
                  <span {...stylex.props(styles.num, styles.laneNum)}>#{t.number}</span>
                  {t.title}
                </span>
              ))
            )}
          </button>
        ))}
      </div>
    </section>
  );
}

/** The newest designs, the same cards as the Designs page; All designs opens that page. */
function DesignsSection({ projectId, projects }: { projectId: string | null; projects: ProjectSummary[] | null }) {
  const { features } = useLinkState();
  const navigate = useNavigate();
  const supported = features.includes("designs");
  const designs = useDesigns(projectId, supported);
  if (!supported) return null;
  const list = designs.data ?? [];
  const name = (id: string) => projects?.find((p) => p.id === id)?.name ?? null;
  return (
    <section aria-label="Designs">
      <SectionTitle
        title="Designs"
        count={list.length}
        link={list.length ? { label: "All designs →", onClick: () => void navigate({ to: "/designs" }) } : null}
      />
      {designs.isPending ? null : list.length === 0 ? (
        <p {...stylex.props(styles.designsEmpty)}>
          No designs yet. When an employee makes an HTML design, it shows here.
        </p>
      ) : (
        <div {...stylex.props(styles.designs)}>
          {list.slice(0, DESIGNS_SHOWN).map((d) => (
            <DesignCard
              key={d.conversation_id}
              design={d}
              project={projects ? name(d.project_id) : null}
              openPage={null}
              onOpen={(_page, fileId) => void navigate({ href: designLink(d, fileId) ?? "/designs" })}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function DoneSection({ tickets }: { tickets: TicketView[] }) {
  const navigate = useNavigate();
  const { features } = useLinkState();
  const days = useMemo(
    () => groupAt(recentlyDone(tickets), (t) => t.done_at_ms ?? t.updated_at_ms, new Date(), "en-GB"),
    [tickets],
  );
  return (
    <section aria-label="Recently done">
      <SectionTitle
        title="Recently done"
        link={
          features.includes("ticket_history")
            ? { label: "History →", onClick: () => void navigate({ to: "/history", search: {} }) }
            : null
        }
      />
      <div {...stylex.props(styles.card)}>
        {days.length === 0 ? <p {...stylex.props(styles.emptyNote)}>Finished tickets show up here.</p> : null}
        {days.map((d) => (
          <div key={d.key}>
            <div {...stylex.props(styles.day)}>{d.label}</div>
            {d.entries.map((t) => (
              <button
                key={t.id}
                type="button"
                onClick={() => void navigate({ to: "/ticket/$number", params: { number: String(t.number) } })}
                {...stylex.props(styles.row)}
              >
                <CheckMark />
                <span {...stylex.props(styles.rowMain)}>
                  <span {...stylex.props(styles.rowT)} style={{ display: "block" }}>
                    <span {...stylex.props(styles.num)}>#{t.number}</span> {t.title}
                  </span>
                </span>
                <span {...stylex.props(styles.side)}>
                  {t.owner_name ?? "—"} · {hhmm(t.done_at_ms ?? t.updated_at_ms)}
                </span>
              </button>
            ))}
          </div>
        ))}
      </div>
    </section>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.fact)}>
      <span {...stylex.props(styles.factK)}>{label}</span>
      <span {...stylex.props(styles.factV)}>{children}</span>
    </div>
  );
}

/** The workspace's styleguide, guidelines, dispatcher and folder in one row; Settings changes them. */
function FactsSection({ project }: { project: ProjectSummary }) {
  const navigate = useNavigate();
  const guidelines = useGuidelines();
  const employees = useEmployees();
  const company = useCompany();
  const linked = linkedGuidelines(guidelines.data ?? [], project.id);
  const dispatcher = dispatcherOf(
    project.dispatcher_employee_id,
    company.data?.company.dispatcher_employee_id,
    employees.data ?? [],
  );
  return (
    <section aria-label="Workspace">
      <SectionTitle
        title="Workspace"
        link={{
          label: "Settings →",
          onClick: () => void navigate({ to: "/projects/$id", params: { id: project.id } }),
        }}
      />
      <div {...stylex.props(styles.card, styles.facts)}>
        <Fact label="Styleguide">
          {project.styleguide_url ? (
            <Link href={project.styleguide_url} target="_blank" xstyle={styles.accent}>
              {`${styleguideName(project.styleguide_url)} ↗`}
            </Link>
          ) : (
            <span {...stylex.props(styles.muted)}>None</span>
          )}
        </Fact>
        <Fact label="Guidelines">
          {linked.count > 0 ? (
            `${linked.count} linked · ${groupDigits(linked.chars)} chars per run`
          ) : (
            <span {...stylex.props(styles.muted)}>Only each employee's own</span>
          )}
        </Fact>
        <Fact label="Dispatcher">
          {dispatcher.kind === "project" || dispatcher.kind === "company" ? (
            <>
              <Avatar name={dispatcher.name} id={dispatcher.id} size={18} />
              {dispatcher.name}
            </>
          ) : dispatcher.kind === "inactive" ? (
            <span {...stylex.props(styles.muted)}>
              {dispatcher.name} · {dispatcher.state}
            </span>
          ) : (
            <span {...stylex.props(styles.muted)}>None</span>
          )}
        </Fact>
        <Fact label="Folder">
          {project.folder ? (
            <span title={project.folder} {...stylex.props(styles.mono)}>
              {project.folder}
            </span>
          ) : (
            <span {...stylex.props(styles.muted)}>No folder</span>
          )}
        </Fact>
      </div>
    </section>
  );
}

/** A workspace without any work yet: three steps to get going. */
function StartCard({ project }: { project: ProjectSummary }) {
  const navigate = useNavigate();
  const settings = () => void navigate({ to: "/projects/$id", params: { id: project.id } });
  return (
    <section aria-label="Get started" {...stylex.props(styles.start)}>
      <h3 {...stylex.props(styles.startTitle)}>{project.name} is ready</h3>
      <p {...stylex.props(styles.startText)}>Three things make work here go well. Each takes a minute.</p>
      <div {...stylex.props(styles.steps)}>
        <button type="button" onClick={settings} {...stylex.props(styles.step)}>
          <span {...stylex.props(styles.stepN, Boolean(project.folder) && styles.stepDone)}>
            {project.folder ? "✓" : "1"}
          </span>
          <b {...stylex.props(styles.stepName)}>Project folder</b>
          <span {...stylex.props(styles.stepText)}>
            {project.folder
              ? `${project.folder}. Employees work in this folder.`
              : "Choose the folder employees work in."}
          </span>
        </button>
        {/* The styleguide is set outside the desktop app (ticket #81), so this step only says what it is. */}
        <div {...stylex.props(styles.step, styles.stepStill)}>
          <span {...stylex.props(styles.stepN, Boolean(project.styleguide_url) && styles.stepDone)}>
            {project.styleguide_url ? "✓" : "2"}
          </span>
          <b {...stylex.props(styles.stepName)}>Add a styleguide</b>
          <span {...stylex.props(styles.stepText)}>
            Link a Design System from claude.ai, so every UI follows the same tokens.
          </span>
        </div>
        <button type="button" onClick={() => void navigate({ to: "/new" })} {...stylex.props(styles.step)}>
          <span {...stylex.props(styles.stepN)}>3</span>
          <b {...stylex.props(styles.stepName)}>Start the first session</b>
          <span {...stylex.props(styles.stepText)}>Describe what should happen. The dispatcher picks who does it.</span>
        </button>
      </div>
    </section>
  );
}

function WorkspaceOverview({ project }: { project: ProjectSummary }) {
  const connection = useConnection();
  const client = useQueryClient();
  const data = useOverviewData();
  const mine = <T extends { project_id: string }>(list: T[]) => list.filter((x) => x.project_id === project.id);
  const tickets = mine(data.tickets);
  const sessions = mine(data.sessions);
  const paused = project.status === "paused";
  const fresh = tickets.length === 0 && sessions.length === 0;
  const resume = () =>
    void connection
      .write("project.set_status", { project_id: project.id, status: "active" }, { retry: false })
      .then(() => client.invalidateQueries({ queryKey: ["projects"] }))
      .then(() => toast("Workspace resumed."))
      .catch((e: unknown) => toast(failureOf(e)));
  return (
    <>
      {paused ? (
        <Banner
          status="warning"
          title="This workspace is paused. No new automatic work starts."
          endContent={<Button size="sm" label="Resume" onClick={resume} />}
        />
      ) : null}
      <header {...stylex.props(styles.hero)}>
        <Mark letter={initial(project.name)} color={markColor(project.id)} size={48} radius={12} />
        <div {...stylex.props(styles.grow)}>
          <h1 {...stylex.props(styles.heroTitle)}>
            {project.name}
            {paused ? (
              <span {...stylex.props(styles.pill, styles.pillPaused)}>Paused</span>
            ) : (
              <span {...stylex.props(styles.pill)}>
                <span {...stylex.props(styles.dot, styles.dotRun)} />
                Active
              </span>
            )}
          </h1>
          {project.description ? (
            <p {...stylex.props(styles.heroText)}>{project.description}</p>
          ) : (
            <p {...stylex.props(styles.heroText, styles.muted)}>
              No description yet.{" "}
              <Link href={`/projects/${project.id}`} isStandalone={false}>
                Add one
              </Link>
            </p>
          )}
        </div>
      </header>
      <FactsSection project={project} />
      {fresh ? <StartCard project={project} /> : null}
      <NeedsSection cards={data.needs.filter((c) => c.projectId === project.id)} projects={null} />
      <RunningSection
        rows={data.running.filter((r) => r.projectId === project.id)}
        waitsForSlot={data.since.waitsForSlot}
      />
      <BoardSection tickets={tickets} />
      <DesignsSection projectId={project.id} projects={null} />
      <DoneSection tickets={tickets} />
    </>
  );
}

/** What a workspace row says about its state: paused first, then what waits, then what runs. */
function rowState(
  project: ProjectSummary,
  needs: number,
  running: number,
): { label: string; tone: "need" | "run" | null } {
  if (project.status === "paused") return { label: "Paused", tone: null };
  if (needs > 0) return { label: `${needs} need${needs === 1 ? "s" : ""} you`, tone: "need" };
  if (running > 0) return { label: `${running} running`, tone: "run" };
  return { label: "Quiet", tone: null };
}

/**
 * All workspaces: every open workspace of the company, by name. Each board member pins the ones they
 * want on their own rail here; the others stay one click away in this list and the palette.
 */
function AllOverview({ projects }: { projects: ProjectSummary[] }) {
  const navigate = useNavigate();
  const keys = useKeys();
  const { choose } = useWorkspace();
  const rail = useRailPins();
  const data = useOverviewData();
  const [query, setQuery] = useState("");
  const sorted = useMemo(() => byName(projects), [projects]);
  const words = query.trim().toLowerCase();
  const shown = words ? sorted.filter((p) => `${p.name} ${p.description ?? ""}`.toLowerCase().includes(words)) : sorted;
  const pinned = projects.filter((p) => rail.pins.includes(p.id)).length;
  return (
    <>
      <header {...stylex.props(styles.hero)}>
        <span {...stylex.props(styles.allTile)}>
          <Icon name="grid" size={18} />
        </span>
        <div {...stylex.props(styles.grow)}>
          <h1 {...stylex.props(styles.heroTitle)}>All workspaces</h1>
          <p {...stylex.props(styles.heroText)}>
            {projects.length} {projects.length === 1 ? "workspace" : "workspaces"} · {pinned} pinned to your rail
          </p>
        </div>
        <div {...stylex.props(styles.actions)}>
          {/* All workspaces has no panel, whose head held this button. */}
          <Tooltip content="All workspaces settings" delay={400} hasHoverIndication={false}>
            <button
              type="button"
              aria-label="All workspaces settings"
              onClick={() => void navigate({ to: "/projects" })}
              {...stylex.props(styles.btn, styles.square)}
            >
              <Icon name="settings" size={15} />
            </button>
          </Tooltip>
          <button type="button" onClick={() => keys.run("NewProject")} {...stylex.props(styles.btn, styles.primary)}>
            <PlusIcon size={14} />
            New workspace
          </button>
        </div>
      </header>
      <section aria-label="Workspaces">
        {projects.length > 0 ? (
          <input
            type="search"
            aria-label="Find a workspace"
            placeholder="Find a workspace"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            {...stylex.props(styles.find)}
          />
        ) : null}
        <ul {...stylex.props(styles.wsList)}>
          {shown.map((p) => {
            const on = rail.pins.includes(p.id);
            const state = rowState(
              p,
              data.needs.filter((c) => c.projectId === p.id).length,
              data.running.filter((r) => r.projectId === p.id).length,
            );
            return (
              <li key={p.id} {...stylex.props(styles.wsRow)}>
                <button
                  type="button"
                  aria-label={`Open ${p.name}`}
                  onClick={() => {
                    choose(p.id);
                    void navigate({ to: "/overview" });
                  }}
                  {...stylex.props(styles.wsOpen)}
                >
                  <Mark letter={initial(p.name)} color={markColor(p.id)} size={32} radius={8} />
                  <span {...stylex.props(styles.wsName)}>
                    {p.name}
                    <small {...stylex.props(styles.wsSmall)}>{p.description || p.folder || ""}</small>
                  </span>
                  <span
                    {...stylex.props(
                      styles.wsState,
                      state.tone === "need" && styles.wsStateNeed,
                      state.tone === "run" && styles.wsStateRun,
                    )}
                  >
                    {state.label}
                  </span>
                </button>
                <button
                  type="button"
                  aria-pressed={on}
                  aria-label={`${on ? "Unpin" : "Pin"} ${p.name} ${on ? "from" : "to"} your rail`}
                  onClick={() => rail.toggle(p.id)}
                  {...stylex.props(styles.btn, on && styles.pinOn)}
                >
                  {on ? "Pinned" : "Pin to rail"}
                </button>
              </li>
            );
          })}
        </ul>
        {projects.length === 0 ? <p {...stylex.props(styles.muted)}>No workspaces yet.</p> : null}
        {projects.length > 0 && shown.length === 0 ? (
          <p {...stylex.props(styles.muted)}>No workspace matches “{query.trim()}”.</p>
        ) : null}
      </section>
    </>
  );
}

/** The Overview page: a workspace's, or All workspaces' when none is chosen. */
export function OverviewScreen() {
  const { id, project } = useWorkspace();
  const projects = useProjects();
  const open = useMemo(() => openProjects(projects.data ?? []), [projects.data]);
  return (
    <div {...stylex.props(styles.scroll)}>
      <div {...stylex.props(styles.content)}>
        {id !== null && project ? <WorkspaceOverview key={project.id} project={project} /> : null}
        {id === null ? <AllOverview projects={open} /> : null}
      </div>
    </div>
  );
}
