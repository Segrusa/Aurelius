// The review-first Code tab (ticket #84, design/code-review.html). Left the queue: what waits for the board
// member's review (tickets in Review and idle work sessions with work main lacks), what employees work on now,
// and one Tidy up card. Right the review of one branch: its checks, the owner's report with its commits and the
// talk about it, the commits, the changed files, and at the bottom Finish review and Merge into main.
// What it decides is in code-review-logic.ts; git runs on the server.

import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { RadioList, RadioListItem } from "@astryxdesign/core/RadioList";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Tab, TabList } from "@astryxdesign/core/TabList";
import { TextArea } from "@astryxdesign/core/TextArea";
import { Tooltip } from "@astryxdesign/core/Tooltip";
import type { CodeView, MessageView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useMemo, useState } from "react";
import { useConnection } from "../connection";
import { useEmployees, useMessages, useTicket, useTickets, useWorkSessions } from "../data";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { ago, shortHash } from "./code-logic";
import {
  behindText,
  type Check,
  type CheckTone,
  canSubmit,
  commitAsk,
  currentItem,
  filterQueue,
  mergeBlocked,
  mergeCheck,
  QUEUE_FILTERS,
  type QueueFilter,
  type ReviewItem,
  reviewCheck,
  reviewPost,
  reviewQueue,
  sizeCheck,
  type Tidy,
  tidyText,
  tidyUp,
  traceAsk,
  type Verdict,
  waited,
} from "./code-review-logic";
import { Held, useSend } from "./dialogs";
import { Confirm, FormDialog } from "./org-kit";
import { Problem } from "./settings-kit";
import { ticketPlace } from "./ticket-session-logic";

const MONO = "var(--font-family-code)";
const NARROW = "@media (max-width: 760px)";
/** The reviewer "Ask Trace to review" writes to. */
const REVIEWER = "Trace";

const styles = stylex.create({
  // minWidth 0: a long commit line or branch name must not make the page wider than the window.
  work: { display: "flex", minHeight: 0, minWidth: 0, flexGrow: 1, flexShrink: 1, flexBasis: "0%" },
  // The queue
  queue: {
    display: { default: "flex", [NARROW]: "flex" },
    flexDirection: "column",
    gap: 4,
    width: { default: 330, [NARROW]: "100%" },
    flexShrink: 0,
    overflowY: "auto",
    paddingTop: 12,
    paddingBottom: 24,
    paddingInline: 10,
    borderRightWidth: { default: 1, [NARROW]: 0 },
    borderRightStyle: "solid",
    borderRightColor: "var(--color-border)",
  },
  queueHidden: { display: { default: "flex", [NARROW]: "none" } },
  seg: { paddingInline: 4, paddingBottom: 6 },
  qHead: { display: "flex", alignItems: "baseline", gap: 8, paddingTop: 10, paddingBottom: 4, paddingInline: 6 },
  qHeadFirst: { paddingTop: 2 },
  qTitle: {
    margin: 0,
    fontSize: 12,
    fontWeight: 600,
    color: "var(--color-text-secondary)",
    textTransform: "uppercase",
    letterSpacing: "0.04em",
  },
  count: { fontSize: 12, color: "var(--precision-text3)" },
  hint: { marginInlineStart: "auto", fontSize: 11.5, color: "var(--precision-text3)" },
  qi: {
    display: "flex",
    flexDirection: "column",
    gap: 4,
    width: "100%",
    paddingBlock: 9,
    paddingInline: 10,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "transparent",
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    color: "inherit",
    font: "inherit",
    textAlign: "start",
    cursor: "pointer",
  },
  qiOn: {
    backgroundColor: { default: "var(--color-background-card)", ":hover": "var(--color-background-card)" },
    borderColor: "var(--color-border-emphasized)",
  },
  qiStill: { cursor: "default", backgroundColor: { default: "transparent", ":hover": "transparent" } },
  t1: { display: "flex", alignItems: "center", gap: 8, minWidth: 0 },
  t2: { display: "flex", alignItems: "center", gap: 8, minWidth: 0, fontSize: 12, color: "var(--precision-text3)" },
  t3: { display: "flex", flexWrap: "wrap", gap: 6 },
  title: { minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: 600 },
  num: { color: "var(--precision-text3)", fontWeight: 500 },
  mono: {
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontFamily: MONO,
    fontSize: 12,
  },
  grow: { flexGrow: 1 },
  size: { fontFamily: MONO, fontSize: 11.5, whiteSpace: "nowrap" },
  add: { color: "var(--color-text-success)" },
  del: { color: "var(--color-text-error)" },
  kind: {
    flexShrink: 0,
    paddingInline: 4,
    borderRadius: 4,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    fontSize: 10.5,
    fontWeight: 700,
    textTransform: "uppercase",
    letterSpacing: "0.04em",
    color: "var(--precision-text3)",
  },
  chip: {
    display: "inline-flex",
    alignItems: "center",
    gap: 5,
    paddingInline: 7,
    paddingBlock: 1,
    borderRadius: 999,
    fontSize: 11,
    fontWeight: 600,
    whiteSpace: "nowrap",
  },
  chip_ok: { backgroundColor: "var(--color-success-muted)", color: "var(--color-text-success)" },
  chip_warn: { backgroundColor: "var(--color-warning-muted)", color: "var(--color-text-warning)" },
  chip_bad: { backgroundColor: "var(--color-error-muted)", color: "var(--color-text-error)" },
  chip_info: { backgroundColor: "var(--color-info-muted)", color: "var(--color-text-info)" },
  chip_n: { backgroundColor: "var(--color-background-muted)", color: "var(--color-text-secondary)" },
  dot: { width: 7, height: 7, borderRadius: "50%", backgroundColor: "currentColor", flexShrink: 0 },
  empty: { paddingBlock: 8, paddingInline: 10, fontSize: 12.5, color: "var(--precision-text3)" },
  tidy: {
    display: "flex",
    flexDirection: "column",
    gap: 6,
    marginTop: 6,
    paddingBlock: 10,
    paddingInline: 12,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "dashed",
    borderColor: "var(--color-border-emphasized)",
    fontSize: 12.5,
    color: "var(--color-text-secondary)",
  },
  tidyList: { margin: 0, paddingInlineStart: 16, fontSize: 12, color: "var(--precision-text3)" },
  row: { display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" },
  link: {
    padding: 0,
    borderWidth: 0,
    backgroundColor: "transparent",
    color: "var(--color-text-accent)",
    font: "inherit",
    fontWeight: 500,
    cursor: "pointer",
    textDecoration: { default: "none", ":hover": "underline" },
  },
  // The review
  review: { display: { default: "flex", [NARROW]: "none" }, flexDirection: "column", flexGrow: 1, minWidth: 0 },
  reviewShown: { display: "flex" },
  scroll: { flexGrow: 1, overflowY: "auto" },
  rv: {
    display: "flex",
    flexDirection: "column",
    gap: 14,
    maxWidth: 1100,
    marginInline: "auto",
    paddingTop: { default: 16, [NARROW]: 12 },
    paddingBottom: 24,
    paddingInline: { default: 24, [NARROW]: 14 },
  },
  back: { display: { default: "none", [NARROW]: "inline-flex" }, alignSelf: "flex-start" },
  l1: { display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" },
  h2: { margin: 0, fontSize: 17, fontWeight: 600 },
  l2: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    flexWrap: "wrap",
    marginTop: 6,
    fontSize: 12.5,
    color: "var(--color-text-secondary)",
  },
  sep: { color: "var(--precision-text3)" },
  checks: {
    display: "grid",
    gridTemplateColumns: {
      default: "repeat(3, minmax(0, 1fr))",
      "@media (max-width: 1180px)": "repeat(2, minmax(0, 1fr))",
    },
    gap: { default: 10, [NARROW]: 8 },
  },
  check: {
    display: "flex",
    flexDirection: "column",
    gap: 3,
    minWidth: 0,
    paddingBlock: 10,
    paddingInline: 12,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
  },
  checkName: {
    fontSize: 11.5,
    fontWeight: 600,
    color: "var(--precision-text3)",
    textTransform: "uppercase",
    letterSpacing: "0.04em",
  },
  checkValue: { display: "flex", alignItems: "center", gap: 6, fontWeight: 600 },
  checkValue_ok: { color: "var(--color-text-success)" },
  checkValue_warn: { color: "var(--color-text-warning)" },
  checkValue_bad: { color: "var(--color-text-error)" },
  checkValue_n: { color: "var(--color-text-primary)" },
  checkSub: { fontSize: 12, color: "var(--precision-text3)" },
  checkAsk: { alignSelf: "flex-start", marginTop: 3 },
  card: {
    borderRadius: 12,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-card)",
    overflow: "hidden",
  },
  // Conversation
  tl: { display: "flex", flexDirection: "column", gap: 12 },
  cwrap: { display: "flex", alignItems: "flex-start", gap: 10 },
  avatar: { display: { default: "inline-flex", [NARROW]: "none" } },
  ccard: {
    flexGrow: 1,
    minWidth: 0,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-card)",
    overflow: "hidden",
  },
  ch: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    paddingBlock: 7,
    paddingInline: 12,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    backgroundColor: "var(--color-background-surface)",
    fontSize: 12.5,
    color: "var(--color-text-secondary)",
  },
  strong: { color: "var(--color-text-primary)", fontWeight: 600 },
  cb: { paddingBlock: 10, paddingInline: 12, minWidth: 0 },
  faint: { color: "var(--precision-text3)" },
  small: { fontSize: 12 },
  evS: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    flexWrap: "wrap",
    fontSize: 12.5,
    color: "var(--color-text-secondary)",
  },
  ic: {
    display: "grid",
    placeItems: "center",
    width: 24,
    height: 24,
    flexShrink: 0,
    marginInlineStart: 4,
    borderRadius: "50%",
    backgroundColor: "var(--color-background-muted)",
    color: "var(--precision-text3)",
  },
  icWarn: { backgroundColor: "var(--color-warning-muted)", color: "var(--color-text-warning)" },
  evc: {
    display: "flex",
    flexDirection: "column",
    gap: 2,
    marginTop: -4,
    marginBottom: 0,
    marginInlineStart: 38,
    padding: 0,
    listStyle: "none",
    fontSize: 12.5,
    color: "var(--color-text-secondary)",
  },
  evcLine: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  hash: { fontFamily: MONO, fontSize: 12, color: "var(--precision-text3)" },
  composer: { display: "flex", flexDirection: "column" },
  composerBox: { paddingBlock: 8, paddingInline: 10 },
  cfoot: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    flexWrap: "wrap",
    paddingBlock: 7,
    paddingInlineStart: 12,
    paddingInlineEnd: 10,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    backgroundColor: "var(--color-background-surface)",
    fontSize: 12,
  },
  // Commits
  commits: { margin: 0, padding: 0, listStyle: "none" },
  commit: {
    display: "grid",
    gridTemplateColumns: "76px minmax(0, 1fr) auto",
    alignItems: "baseline",
    gap: 10,
    paddingBlock: 8,
    paddingInline: 14,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
  },
  commitFirst: { borderTopWidth: 0 },
  // Files
  files: { margin: 0, padding: 0, listStyle: "none" },
  file: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    paddingBlock: 7,
    paddingInline: 12,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
  },
  // The action bar
  actbar: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    flexWrap: "wrap",
    flexShrink: 0,
    paddingBlock: 10,
    paddingInline: { default: 24, [NARROW]: 14 },
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
  },
  state: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    flexWrap: "wrap",
    fontSize: 12.5,
    color: "var(--color-text-secondary)",
    width: { default: "auto", [NARROW]: "100%" },
  },
  actGrow: { flexGrow: 1, display: { default: "block", [NARROW]: "none" } },
  center: { display: "flex", justifyContent: "center", paddingBlock: 48 },
});

type Tone = CheckTone | "info";

function Chip({ tone, children, live }: { tone: Tone; children: ReactNode; live?: boolean }) {
  return (
    <span {...stylex.props(styles.chip, styles[`chip_${tone}`])}>
      {live ? <span aria-hidden="true" {...stylex.props(styles.dot)} /> : null}
      {children}
    </span>
  );
}

function Size({ add, del }: { add: number; del: number }) {
  return (
    <span {...stylex.props(styles.size)}>
      <span {...stylex.props(styles.add)}>+{add}</span> <span {...stylex.props(styles.del)}>−{del}</span>
    </span>
  );
}

function Owner({ id, name, size = 20 }: { id: string | null; name: string | null; size?: number }) {
  if (!name) return null;
  return <Mark letter={initial(name)} color={markColor(id ?? name)} size={size} radius={size > 24 ? size / 2 : 5} />;
}

function Label({ item }: { item: ReviewItem }) {
  return item.number !== null ? (
    <>
      <span {...stylex.props(styles.num)}>#{item.number}</span> {item.title}
    </>
  ) : (
    item.title
  );
}

const CHECK_ICON: Record<CheckTone, string> = { ok: "✓", warn: "!", bad: "✕", n: "" };

/** A button whose reason for being off shows on hover (a disabled button gets no hover itself). */
function WithReason({ why, children }: { why: string | null; children: ReactNode }) {
  if (!why) return <>{children}</>;
  return (
    <Tooltip content={why} delay={300}>
      <span>{children}</span>
    </Tooltip>
  );
}

/** The board member's verdicts on this device, per branch head: a new commit asks for a new review. */
function useVerdicts() {
  const [verdicts, setVerdicts] = useState<Record<string, Verdict>>({});
  const keyOf = (item: ReviewItem) => `${item.branch.name}@${item.branch.head}`;
  return {
    of: (item: ReviewItem): Verdict | null => {
      const v = verdicts[keyOf(item)];
      return v === "approved" || v === "changes" ? v : null;
    },
    set: (item: ReviewItem, v: Verdict) => setVerdicts((prev) => ({ ...prev, [keyOf(item)]: v })),
  };
}

function Queue({
  ready,
  working,
  current,
  filter,
  onFilter,
  onPick,
  tidy,
  main,
  now,
  verdictOf,
  busy,
  onCleanUp,
  onShowAll,
  hidden,
}: {
  ready: ReviewItem[];
  working: ReviewItem[];
  current: string | null;
  filter: QueueFilter;
  onFilter: (f: QueueFilter) => void;
  onPick: (key: string) => void;
  tidy: Tidy;
  main: string;
  now: number;
  verdictOf: (item: ReviewItem) => Verdict | null;
  busy: boolean;
  onCleanUp: () => void;
  onShowAll: () => void;
  hidden: boolean;
}) {
  const tidyLine = tidyText(tidy, main);
  const named = [...tidy.branches];
  return (
    <aside aria-label="Branches to review" {...stylex.props(styles.queue, hidden && styles.queueHidden)}>
      <div {...stylex.props(styles.seg)}>
        <SegmentedControl label="Show" size="sm" value={filter} onChange={(v) => onFilter(v as QueueFilter)}>
          {QUEUE_FILTERS.map((f) => (
            <SegmentedControlItem key={f.value} value={f.value} label={f.label} />
          ))}
        </SegmentedControl>
      </div>
      <div {...stylex.props(styles.qHead, styles.qHeadFirst)}>
        <h2 {...stylex.props(styles.qTitle)}>Ready for review</h2>
        <span {...stylex.props(styles.count)}>{ready.length}</span>
        <span {...stylex.props(styles.hint)}>oldest first</span>
      </div>
      {ready.length === 0 ? <div {...stylex.props(styles.empty)}>Nothing waits for your review.</div> : null}
      {ready.map((item) => {
        const merge = mergeCheck(item, main);
        const rev = reviewCheck(item, verdictOf(item));
        return (
          <button
            key={item.key}
            type="button"
            aria-current={item.key === current}
            onClick={() => onPick(item.key)}
            {...stylex.props(styles.qi, item.key === current && styles.qiOn)}
          >
            <span {...stylex.props(styles.t1)}>
              <span title={item.ownerName ?? undefined}>
                <Owner id={item.ownerId} name={item.ownerName} />
              </span>
              <span {...stylex.props(styles.title)}>
                <Label item={item} />
              </span>
              {item.kind === "session" ? (
                <>
                  <span {...stylex.props(styles.grow)} />
                  <span {...stylex.props(styles.kind)}>Session</span>
                </>
              ) : null}
            </span>
            <span {...stylex.props(styles.t2)}>
              <span {...stylex.props(styles.mono)}>{item.branch.name}</span>
              <span {...stylex.props(styles.grow)} />
              <Size add={item.branch.insertions ?? 0} del={item.branch.deletions ?? 0} />
            </span>
            <span {...stylex.props(styles.t3)}>
              {rev.tone !== "n" ? <Chip tone={rev.tone}>{rev.value}</Chip> : null}
              <Chip tone={merge.tone}>{merge.value}</Chip>
              <Chip tone="n">
                {item.kind === "session" ? "idle" : "waits"} {waited(item.since, now)}
              </Chip>
            </span>
          </button>
        );
      })}
      <div {...stylex.props(styles.qHead)}>
        <h2 {...stylex.props(styles.qTitle)}>Being worked on</h2>
        <span {...stylex.props(styles.count)}>{working.length}</span>
      </div>
      {working.length === 0 ? <div {...stylex.props(styles.empty)}>Nobody works on a branch right now.</div> : null}
      {working.map((item) => (
        <div key={item.key} {...stylex.props(styles.qi, styles.qiStill)}>
          <span {...stylex.props(styles.t1)}>
            <span title={item.ownerName ?? undefined}>
              <Owner id={item.ownerId} name={item.ownerName} />
            </span>
            <span {...stylex.props(styles.title)}>
              <Label item={item} />
            </span>
            <span {...stylex.props(styles.grow)} />
            {item.kind === "session" ? <span {...stylex.props(styles.kind)}>Session</span> : null}
            <Chip tone="ok" live>
              Working
            </Chip>
          </span>
          <span {...stylex.props(styles.t2)}>
            <span {...stylex.props(styles.mono)}>{item.branch.name}</span>
          </span>
          <span {...stylex.props(styles.t2)}>{workingLine(item)}</span>
        </div>
      ))}
      <div {...stylex.props(styles.qHead)}>
        <h2 {...stylex.props(styles.qTitle)}>Tidy up</h2>
      </div>
      <div {...stylex.props(styles.tidy)}>
        <span>{tidyLine ?? "Nothing to tidy up."}</span>
        {named.length > 0 ? (
          <ul {...stylex.props(styles.tidyList)}>
            {named.slice(0, 2).map((b) => (
              <li key={b} {...stylex.props(styles.mono)}>
                {b}
              </li>
            ))}
            {named.length > 2 ? <li>and {named.length - 2} more</li> : null}
          </ul>
        ) : null}
        <span {...stylex.props(styles.row)}>
          {tidyLine ? <Button size="sm" label="Clean up merged" isDisabled={busy} onClick={onCleanUp} /> : null}
          <button type="button" onClick={onShowAll} {...stylex.props(styles.link)}>
            All branches and graph →
          </button>
        </span>
      </div>
    </aside>
  );
}

/** "1 commit · 4 files changed, not committed" under a branch being worked on. */
function workingLine(item: ReviewItem): string {
  const parts: string[] = [];
  if (item.branch.ahead > 0) parts.push(`${item.branch.ahead} ${item.branch.ahead === 1 ? "commit" : "commits"}`);
  if (item.uncommitted > 0)
    parts.push(`${item.uncommitted} ${item.uncommitted === 1 ? "file" : "files"} changed, not committed`);
  return parts.length > 0 ? parts.join(" · ") : "nothing committed yet";
}

function CheckCard({ check, children }: { check: Check; children?: ReactNode }) {
  const icon = CHECK_ICON[check.tone];
  return (
    <div {...stylex.props(styles.check)}>
      <span {...stylex.props(styles.checkName)}>{check.name}</span>
      <span {...stylex.props(styles.checkValue, styles[`checkValue_${check.tone}`])}>
        {icon ? <span aria-hidden="true">{icon}</span> : null}
        {check.value}
      </span>
      <span {...stylex.props(styles.checkSub)}>{check.sub}</span>
      {children}
    </div>
  );
}

/** The owner's words about the work: a ticket's report, or a session's last answer. */
function useReport(item: ReviewItem) {
  const ticket = useTicket(item.kind === "ticket" ? (item.ticket?.id ?? null) : null);
  const session = useMessages(item.kind === "session" ? item.session?.conversation_id : null);
  const discussion = useMessages(item.kind === "ticket" ? item.ticket?.conversation_id : null);
  if (item.kind === "ticket") {
    const root = ticket.data?.root_task ?? null;
    const at = root?.updated_at_ms ?? item.since;
    const after = (discussion.data?.messages ?? []).filter((m) => m.created_at_ms >= at && m.text.trim() !== "");
    return { text: root?.result ?? null, at, comments: after, loading: ticket.isPending };
  }
  const messages = session.data?.messages ?? [];
  let last = -1;
  messages.forEach((m, i) => {
    if (m.role === "assistant" && m.text.trim() !== "") last = i;
  });
  const answer = last >= 0 ? (messages[last] as MessageView) : null;
  const comments = messages.slice(last + 1).filter((m) => m.role === "user" && m.text.trim() !== "");
  return { text: answer?.text ?? null, at: answer?.created_at_ms ?? item.since, comments, loading: session.isPending };
}

function authorOf(m: MessageView): string {
  if (m.role === "user") return m.author_user?.username ?? m.author ?? "You";
  return m.author ?? "Employee";
}

function Conversation({
  item,
  now,
  onOpen,
  onAskTrace,
  traceBusy,
}: {
  item: ReviewItem;
  now: number;
  onOpen: () => void;
  onAskTrace: (() => void) | null;
  traceBusy: boolean;
}) {
  const connection = useConnection();
  const report = useReport(item);
  const [text, setText] = useState("");
  const { busy, held, error, send, sendAnyway } = useSend((done) => {
    toast(done);
    setText("");
  });
  const isSession = item.kind === "session";
  const owner = item.ownerName ?? "the owner";
  const commits = item.branch.commits ?? [];
  const post = () => {
    const words = text.trim();
    if (!words || busy) return;
    send(async (confirmSecret) => {
      if (item.ticket) {
        await connection.write(
          "ticket.post",
          { ticket_id: item.ticket.id, text: words, confirm_secret: confirmSecret },
          { retry: false },
        );
      } else if (item.session) {
        await connection.write(
          "chat.send",
          { conversation_id: item.session.conversation_id, text: words, confirm_secret: confirmSecret },
          { retry: false },
        );
      }
      return `Posted; ${owner} is woken.`;
    });
  };
  const author = (who: string | null) => (
    <span {...stylex.props(styles.avatar)}>
      <Mark letter={initial(who ?? "?")} color={markColor(who ?? "?")} size={28} radius={14} />
    </span>
  );
  return (
    <div {...stylex.props(styles.tl)}>
      <div {...stylex.props(styles.cwrap)}>
        <span {...stylex.props(styles.avatar)}>
          <Owner id={item.ownerId} name={item.ownerName} size={28} />
        </span>
        <div {...stylex.props(styles.ccard)}>
          <div {...stylex.props(styles.ch)}>
            <span {...stylex.props(styles.strong)}>{item.ownerName ?? "Nobody"}</span>
            {isSession ? "in the session" : "reported"} · {ago(report.at, now)}
            <span {...stylex.props(styles.grow)} />
            <Chip tone="n">{isSession ? "Last answer" : "Report"}</Chip>
          </div>
          <div {...stylex.props(styles.cb)}>
            {report.text ? (
              <Markdown text={report.text} safe />
            ) : (
              <span {...stylex.props(styles.faint)}>
                {report.loading ? "Loading…" : isSession ? "No answer yet." : "No report yet."}
              </span>
            )}
            <div>
              <button type="button" onClick={onOpen} {...stylex.props(styles.link, styles.small)}>
                {isSession ? "Open the session" : `Open ticket #${item.number}`}
              </button>
            </div>
          </div>
        </div>
      </div>
      {commits.length > 0 ? (
        <>
          <div {...stylex.props(styles.evS)}>
            <span aria-hidden="true" {...stylex.props(styles.ic)}>
              ●
            </span>
            <span>
              <span {...stylex.props(styles.strong)}>{item.ownerName ?? "Someone"}</span> added {item.branch.ahead}{" "}
              {item.branch.ahead === 1 ? "commit" : "commits"} · last {ago(item.branch.at_ms, now)}
            </span>
          </div>
          <ul {...stylex.props(styles.evc)}>
            {[...commits].reverse().map((c) => (
              <li key={c.hash} {...stylex.props(styles.evcLine)}>
                <span {...stylex.props(styles.hash)}>{shortHash(c.hash)}</span> {c.subject}
              </li>
            ))}
          </ul>
        </>
      ) : null}
      {item.uncommitted > 0 ? (
        <div {...stylex.props(styles.evS)}>
          <span aria-hidden="true" {...stylex.props(styles.ic, styles.icWarn)}>
            !
          </span>
          <span>
            {item.uncommitted} {item.uncommitted === 1 ? "file" : "files"} changed after the last commit and{" "}
            {item.uncommitted === 1 ? "is" : "are"} <span {...stylex.props(styles.strong)}>not committed</span> yet.
          </span>
        </div>
      ) : null}
      {isSession && onAskTrace ? (
        <div {...stylex.props(styles.evS)}>
          <span aria-hidden="true" {...stylex.props(styles.ic)}>
            ◎
          </span>
          <span>No review yet. A session has no review step of its own.</span>
          <Button size="sm" label="Ask Trace to review" isDisabled={traceBusy} onClick={onAskTrace} />
        </div>
      ) : null}
      {report.comments.map((m) => (
        <div key={m.id} {...stylex.props(styles.cwrap)}>
          {author(authorOf(m))}
          <div {...stylex.props(styles.ccard)}>
            <div {...stylex.props(styles.ch)}>
              <span {...stylex.props(styles.strong)}>{authorOf(m)}</span> commented · {ago(m.created_at_ms, now)}
            </div>
            <div {...stylex.props(styles.cb)}>
              <Markdown text={m.text} safe />
            </div>
          </div>
        </div>
      ))}
      <div {...stylex.props(styles.cwrap)}>
        {author("You")}
        <div {...stylex.props(styles.ccard, styles.composer)}>
          <div {...stylex.props(styles.composerBox)}>
            <TextArea
              label="Comment"
              isLabelHidden
              placeholder={`Add a comment for ${owner}`}
              rows={3}
              value={text}
              onChange={setText}
            />
          </div>
          {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
          {error ? <Problem error={error} /> : null}
          <div {...stylex.props(styles.cfoot)}>
            <span {...stylex.props(styles.faint)}>
              {isSession
                ? `Goes to the session and wakes ${owner}.`
                : `Goes to the discussion of ticket #${item.number} and wakes ${owner}.`}
            </span>
            <span {...stylex.props(styles.grow)} />
            <Button size="sm" variant="primary" label="Comment" isDisabled={busy || !text.trim()} onClick={post} />
          </div>
        </div>
      </div>
    </div>
  );
}

function Commits({ item, view, now }: { item: ReviewItem; view: CodeView; now: number }) {
  const known = useMemo(() => new Map((view.commits ?? []).map((c) => [c.hash, c])), [view.commits]);
  const commits = item.branch.commits ?? [];
  if (commits.length === 0) return <EmptyState isCompact title="No commits ahead of main yet." />;
  return (
    <div {...stylex.props(styles.card)}>
      <ul {...stylex.props(styles.commits)}>
        {commits.map((c, i) => {
          const full = known.get(c.hash);
          return (
            <li key={c.hash} {...stylex.props(styles.commit, i === 0 && styles.commitFirst)}>
              <span {...stylex.props(styles.hash)}>{shortHash(c.hash)}</span>
              <span {...stylex.props(styles.mono)} style={{ fontFamily: "inherit", fontSize: 13 }}>
                {c.subject}
              </span>
              <span {...stylex.props(styles.faint, styles.small)}>
                {full ? `${full.author} · ${ago(full.at_ms, now)}` : ""}
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function Files({ item, view }: { item: ReviewItem; view: CodeView }) {
  const uncommitted = (view.worktrees ?? []).find((w) => w.branch === item.branch.name)?.changed_files ?? [];
  return (
    <div {...stylex.props(styles.tl)}>
      <div {...stylex.props(styles.evS)}>
        <span>
          {item.branch.files_changed ?? 0} {item.branch.files_changed === 1 ? "file" : "files"} changed against{" "}
          {view.main_branch ?? "main"} <Size add={item.branch.insertions ?? 0} del={item.branch.deletions ?? 0} />. The
          line-by-line diff is not available from this server yet.
        </span>
      </div>
      {uncommitted.length > 0 ? (
        <div {...stylex.props(styles.card)}>
          <div {...stylex.props(styles.ch)}>
            <span {...stylex.props(styles.strong)}>Not committed</span>
            <span {...stylex.props(styles.grow)} />
            <Chip tone="warn">{item.uncommitted}</Chip>
          </div>
          <ul {...stylex.props(styles.files)}>
            {uncommitted.map((f) => (
              <li key={f} {...stylex.props(styles.file)}>
                <span title={f} {...stylex.props(styles.mono)}>
                  {f}
                </span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

type ReviewTab = "conversation" | "commits" | "files";
type Asking = "finish" | "merge" | "commit" | null;

function FinishReview({
  item,
  onClose,
  onDone,
}: {
  item: ReviewItem;
  onClose: () => void;
  onDone: (v: Verdict) => void;
}) {
  const connection = useConnection();
  const [verdict, setVerdict] = useState<Verdict>("comment");
  const [body, setBody] = useState("");
  const isSession = item.kind === "session";
  const owner = item.ownerName ?? "The owner";
  const { busy, held, error, send, sendAnyway } = useSend((text) => {
    toast(text);
    onDone(verdict);
    onClose();
  });
  const go = () => {
    if (busy || !canSubmit(verdict, body)) return;
    const text = reviewPost(verdict, body, item.branch.name);
    send(async (confirmSecret) => {
      if (item.ticket) {
        if (verdict === "changes")
          await connection.write(
            "ticket.send_back",
            { ticket_id: item.ticket.id, feedback: text, confirm_secret: confirmSecret },
            { retry: false },
          );
        else if (verdict === "comment" || body.trim() !== "")
          await connection.write(
            "ticket.post",
            { ticket_id: item.ticket.id, text, confirm_secret: confirmSecret },
            { retry: false },
          );
      } else if (item.session && (verdict !== "approved" || body.trim() !== "")) {
        await connection.write(
          "chat.send",
          { conversation_id: item.session.conversation_id, text, confirm_secret: confirmSecret },
          { retry: false },
        );
      }
      if (verdict === "approved") return "Approved. Merge into main when you are ready.";
      return `Sent to ${owner}${isSession ? " in the session" : ` on ticket #${item.number}`}.`;
    });
  };
  return (
    <FormDialog
      title="Finish your review"
      width={520}
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="primary" label="Submit review" isDisabled={busy || !canSubmit(verdict, body)} onClick={go} />
        </>
      }
    >
      <TextArea
        hasAutoFocus
        label="Summary"
        isLabelHidden
        placeholder={verdict === "approved" ? "Leave a summary (optional)" : "Leave a summary"}
        rows={4}
        value={body}
        onChange={setBody}
      />
      <RadioList label="Verdict" isLabelHidden value={verdict} onChange={(v) => setVerdict(v as Verdict)}>
        <RadioListItem
          value="comment"
          label="Comment"
          description={`${owner} is woken and answers. ${isSession ? "Nothing else changes." : "The ticket stays in Review."}`}
        />
        <RadioListItem
          value="approved"
          label="Approve"
          description="Your approval shows in Review. You merge when you are ready."
        />
        <RadioListItem
          value="changes"
          label="Request changes"
          description={`${owner} is woken to change it.${isSession ? "" : " The ticket goes back to Doing."}`}
        />
      </RadioList>
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? <Problem error={error} /> : null}
    </FormDialog>
  );
}

function AskCommit({ item, onClose }: { item: ReviewItem; onClose: () => void }) {
  const connection = useConnection();
  const owner = item.ownerName ?? "the owner";
  const [text, setText] = useState(commitAsk(item.branch.name));
  const { busy, held, error, send, sendAnyway } = useSend((done) => {
    toast(done);
    onClose();
  });
  const go = () => {
    if (busy || !text.trim()) return;
    send(async (confirmSecret) => {
      if (item.session)
        await connection.write(
          "chat.send",
          { conversation_id: item.session.conversation_id, text: text.trim(), confirm_secret: confirmSecret },
          { retry: false },
        );
      else if (item.ticket)
        await connection.write(
          "ticket.post",
          { ticket_id: item.ticket.id, text: text.trim(), confirm_secret: confirmSecret },
          { retry: false },
        );
      return `Sent to ${owner}${item.session ? " in the session" : ` on ticket #${item.number}`}.`;
    });
  };
  return (
    <FormDialog
      title={`Ask ${owner} to commit?`}
      width={520}
      onClose={onClose}
      footer={
        <>
          <Button label="Cancel" onClick={onClose} />
          <Button variant="primary" label="Send" isDisabled={busy || !text.trim()} onClick={go} />
        </>
      }
    >
      <span>
        This sends a message {item.session ? "in the session" : "in the ticket's discussion"} and wakes {owner}. Merge
        into main becomes available once the worktree is clean.
      </span>
      <TextArea label={`Message for ${owner}`} isLabelHidden rows={3} value={text} onChange={setText} />
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? <Problem error={error} /> : null}
    </FormDialog>
  );
}

/** The review side of the Code tab: queue and review. `onMerge` runs `code.merge`, `onCleanUp` asks for Clean up merged. */
export function CodeReview({
  view,
  projectId,
  now,
  busy,
  onMerge,
  onCleanUp,
  onShowAll,
}: {
  view: CodeView;
  projectId: string;
  now: number;
  busy: boolean;
  onMerge: (branch: string) => void;
  onCleanUp: () => void;
  onShowAll: () => void;
}) {
  const navigate = useNavigate();
  const connection = useConnection();
  const tickets = useTickets();
  const sessions = useWorkSessions(projectId);
  const employees = useEmployees();
  const verdicts = useVerdicts();
  const [filter, setFilter] = useState<QueueFilter>("all");
  const [wanted, setWanted] = useState<string | null>(null);
  const [tab, setTab] = useState<ReviewTab>("conversation");
  const [reviewing, setReviewing] = useState(false);
  const [asking, setAsking] = useState<Asking>(null);
  const [traceBusy, setTraceBusy] = useState(false);
  const main = view.main_branch ?? "main";

  const queue = useMemo(
    () =>
      reviewQueue(
        view,
        (tickets.data ?? []).filter((t) => t.project_id === projectId),
        sessions.data ?? [],
      ),
    [view, tickets.data, sessions.data, projectId],
  );
  const ready = filterQueue(queue.ready, filter);
  const working = filterQueue(queue.working, filter);
  const item = currentItem(ready, wanted);
  const tidy = useMemo(() => tidyUp(view, now), [view, now]);
  const trace = (employees.data ?? []).find((e) => e.name === REVIEWER) ?? null;

  const pick = (key: string) => {
    setWanted(key);
    setTab("conversation");
    setReviewing(true);
  };

  const askTrace = (it: ReviewItem) => {
    if (!trace || traceBusy) return;
    setTraceBusy(true);
    connection
      .write(
        "employee.send_message",
        { employee_id: trace.id, conversation_id: trace.conversation_id, text: traceAsk(it, view.repo_path, main) },
        { retry: false },
      )
      .then(() => toast(`${REVIEWER} reviews ${it.branch.name}. The findings come in ${REVIEWER}'s chat.`))
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."))
      .finally(() => setTraceBusy(false));
  };

  const open = (it: ReviewItem) => {
    if (it.ticket) void navigate(ticketPlace({ id: it.ticket.id, number: it.ticket.number }));
    else if (it.session)
      void navigate({ to: "/session/$conversationId", params: { conversationId: it.session.conversation_id } });
  };

  let review: ReactNode;
  if (!item) {
    review = (
      <div {...stylex.props(styles.center)}>
        <EmptyState
          isCompact
          title="Nothing waits for your review"
          description="A ticket shows here when it is in Review with work main lacks; a session when it is idle with work on a branch of its own."
        />
      </div>
    );
  } else {
    const isSession = item.kind === "session";
    const verdict = verdicts.of(item);
    const blocked = mergeBlocked(item);
    const checks: Check[] = [
      reviewCheck(item, verdict),
      mergeCheck(item, main),
      sizeCheck(item.branch, item.uncommitted),
    ];
    const status = isSession ? "Idle" : item.ticket?.column === "doing" ? "Doing" : "Review";
    review = (
      <>
        <div {...stylex.props(styles.scroll)}>
          <div {...stylex.props(styles.rv)}>
            <span {...stylex.props(styles.back)}>
              <Button size="sm" variant="ghost" label="‹ To review" onClick={() => setReviewing(false)} />
            </span>
            <div>
              <div {...stylex.props(styles.l1)}>
                {isSession ? <span {...stylex.props(styles.kind)}>Session</span> : null}
                <h2 {...stylex.props(styles.h2)}>
                  <Label item={item} />
                </h2>
                <Chip tone={isSession ? "n" : status === "Doing" ? "warn" : "info"}>{status}</Chip>
              </div>
              <div {...stylex.props(styles.l2)}>
                <Owner id={item.ownerId} name={item.ownerName} />
                {item.ownerName ?? "Nobody"}
                <span {...stylex.props(styles.sep)}>·</span>
                <span {...stylex.props(styles.mono)}>{item.branch.name}</span>
                <span {...stylex.props(styles.sep)}>→</span>
                <span {...stylex.props(styles.mono)}>{main}</span>
                <span {...stylex.props(styles.sep)}>·</span>
                {behindText(item.branch, main)}
                <span {...stylex.props(styles.sep)}>·</span>
                {isSession
                  ? `${item.where === view.repo_path ? "own branch in the project folder" : "worktree"} · idle ${waited(item.since, now)}`
                  : `waits ${waited(item.since, now)}`}
              </div>
            </div>
            <div {...stylex.props(styles.checks)}>
              {checks.map((c) => (
                <CheckCard key={c.name} check={c}>
                  {isSession && c.name === "Review" && c.tone === "n" && trace ? (
                    <span {...stylex.props(styles.checkAsk)}>
                      <Button
                        size="sm"
                        label="Ask Trace to review"
                        isDisabled={traceBusy}
                        onClick={() => askTrace(item)}
                      />
                    </span>
                  ) : null}
                </CheckCard>
              ))}
            </div>
            <TabList role="tablist" aria-label="Review" hasDivider value={tab} onChange={(v) => setTab(v as ReviewTab)}>
              <Tab value="conversation" label="Conversation" panelId="code-review-panel" />
              <Tab
                value="commits"
                label="Commits"
                panelId="code-review-panel"
                endContent={<span {...stylex.props(styles.count)}>{item.branch.ahead}</span>}
              />
              <Tab
                value="files"
                label="Files changed"
                panelId="code-review-panel"
                endContent={<span {...stylex.props(styles.count)}>{item.branch.files_changed ?? 0}</span>}
              />
            </TabList>
            <div id="code-review-panel" role="tabpanel">
              {tab === "conversation" ? (
                <Conversation
                  key={item.key}
                  item={item}
                  now={now}
                  onOpen={() => open(item)}
                  onAskTrace={trace ? () => askTrace(item) : null}
                  traceBusy={traceBusy}
                />
              ) : null}
              {tab === "commits" ? <Commits item={item} view={view} now={now} /> : null}
              {tab === "files" ? <Files item={item} view={view} /> : null}
            </div>
          </div>
        </div>
        <div {...stylex.props(styles.actbar)}>
          <span {...stylex.props(styles.state)}>
            {item.uncommitted > 0 ? (
              <Chip tone="warn">
                ! {item.uncommitted} {item.uncommitted === 1 ? "file" : "files"} not committed
              </Chip>
            ) : null}
            <span>
              <span {...stylex.props(styles.strong)}>{item.branch.ahead}</span>{" "}
              {item.branch.ahead === 1 ? "commit" : "commits"} ·{" "}
              <span {...stylex.props(styles.strong)}>{item.branch.files_changed ?? 0}</span> files
            </span>
          </span>
          <span {...stylex.props(styles.actGrow)} />
          <Button label="Finish review" onClick={() => setAsking("finish")} />
          {item.uncommitted > 0 ? (
            <Button
              variant="primary"
              label={`Ask ${item.ownerName ?? "the owner"} to commit`}
              onClick={() => setAsking("commit")}
            />
          ) : (
            <WithReason why={blocked}>
              <Button
                variant="primary"
                label={`Merge into ${main}`}
                isDisabled={busy || blocked !== null}
                onClick={() => setAsking("merge")}
              />
            </WithReason>
          )}
        </div>
      </>
    );
  }

  return (
    <div {...stylex.props(styles.work)}>
      <Queue
        ready={ready}
        working={working}
        current={item?.key ?? null}
        filter={filter}
        onFilter={setFilter}
        onPick={pick}
        tidy={tidy}
        main={main}
        now={now}
        verdictOf={verdicts.of}
        busy={busy}
        onCleanUp={onCleanUp}
        onShowAll={onShowAll}
        hidden={reviewing && item !== null}
      />
      <section aria-label="Review" {...stylex.props(styles.review, reviewing && item !== null && styles.reviewShown)}>
        {review}
      </section>
      {asking === "finish" && item ? (
        <FinishReview item={item} onClose={() => setAsking(null)} onDone={(v) => verdicts.set(item, v)} />
      ) : null}
      {asking === "commit" && item ? <AskCommit item={item} onClose={() => setAsking(null)} /> : null}
      {asking === "merge" && item ? (
        <Confirm
          title={`Merge ${item.branch.name} into ${main}?`}
          body={mergeBody(item, main)}
          action={`Merge into ${main}`}
          busy={busy}
          onConfirm={() => {
            setAsking(null);
            onMerge(item.branch.name);
          }}
          onClose={() => setAsking(null)}
        />
      ) : null}
    </div>
  );
}

function mergeBody(item: ReviewItem, main: string): ReactNode {
  const b = item.branch;
  return (
    <>
      {b.ahead} {b.ahead === 1 ? "commit" : "commits"}, {b.files_changed ?? 0} files, +{b.insertions ?? 0} −
      {b.deletions ?? 0}.{" "}
      {b.behind > 0
        ? `${main} moved on by ${b.behind} ${b.behind === 1 ? "commit" : "commits"}; a conflict stops the merge and leaves ${main} as it was.`
        : `The branch is up to date with ${main}.`}{" "}
      Nothing is pushed.{" "}
      {item.kind === "session"
        ? `The session stays open; ${item.ownerName ?? "the owner"} can keep working on the branch.`
        : `Ticket #${item.number} stays in Review; set it to Done on the board.`}{" "}
      The branch and its worktree stay until you clean up.
    </>
  );
}
