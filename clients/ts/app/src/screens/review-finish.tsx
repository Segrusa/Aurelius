// The Review card as a finish line (ticket #89, design/review-finish.html). One card from Review to
// Done: your decision, merging after you asked the owner (with your request and the owner's steps
// live), merged, the merge stopped (with the conflicting files), and done. Merging stays the owner's
// work through the discussion, as the review card always asked it; the card only shows where it is.

import { Button } from "@astryxdesign/core/Button";
import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { Link } from "@astryxdesign/core/Link";
import { expectBody } from "@aurelius/client";
import type { CodeBranch, CodeCommit, TicketDetail, TicketView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useMessages } from "../data";
import { showChord } from "../keys/keymap";
import { isMac } from "../keys/provider";
import { usePlatform } from "../platform";
import { KeyHint } from "../ui/key-hint";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { doneCheck } from "./board-logic";
import { CODE_VIEW } from "./code-logic";
import { Held, useSend } from "./dialogs";
import { Problem } from "./org-kit";
import { acceptLabel, isQuick } from "./quick-logic";
import {
  autoDoneDue,
  clock,
  count,
  EXPLAIN_ASK,
  type FinishKind,
  type FinishLine,
  type FinishStep,
  finishKind,
  finishLines,
  finishSteps,
  finishTone,
  MERGE_ASK,
  mainBranch,
  mergeAsk,
  mergeCommitOf,
  PUSH_ASK,
  RESOLVE_ASK,
  ticketBranch,
  UNDO_ASK,
} from "./review-finish-logic";
import { factsOf, minutes, stepText } from "./ticket-activity-plain-logic";
import { type TaskLive, type TicketWork, useNow } from "./ticket-page";
import { Clamped, ConfirmDialog, GateKeys } from "./ticket-panel";
import { ticketNumber, useTicketWrite } from "./tickets-kit";

/** How often the git view is read again while the owner merges. */
export const FINISH_MERGING_REFRESH_MS = 5_000;
/** How long a read git view counts as fresh otherwise. */
export const FINISH_CODE_STALE_MS = 30_000;
/** Device storage: Mark done when merged, per host. Unset means on, as the design starts. */
const AUTO_DONE_KEY = "review-finish.auto-done";

/** Everything the finish card shows, derived once per render. */
export interface Finish {
  kind: FinishKind;
  steps: FinishStep[];
  ask: { id: string; atMs: number; author: string | null } | null;
  branch: CodeBranch | null;
  main: CodeBranch | null;
  merge: CodeCommit | null;
  live: TaskLive;
}

/** The project's git view under the Code tab's own key, so both share one read. */
function useProjectCode(projectId: string, fast: boolean) {
  const connection = useConnection();
  const { status, user, features } = useLinkState();
  return useQuery({
    queryKey: ["code", projectId],
    enabled: features.includes(CODE_VIEW) && status === "online" && user !== null,
    staleTime: fast ? 0 : FINISH_CODE_STALE_MS,
    ...(fast ? { refetchInterval: FINISH_MERGING_REFRESH_MS } : {}),
    queryFn: async () => expectBody(await connection.call("code.get", { project_id: projectId }), "code").value,
  });
}

/** The moment of the ticket's finish card; null when the ticket is not in Review or Done. */
export function useFinish(detail: TicketDetail, t: TicketView, work: TicketWork): Finish | null {
  const shown = t.column === "review" || t.column === "done";
  const discussion = useMessages(shown ? t.conversation_id : null);
  const root = detail.root_task ?? null;
  const live = work.live(root?.id ?? "");
  const running = Boolean(root?.live_run) || root?.status === "running" || root?.status === "queued";
  const ask = t.column === "review" ? mergeAsk(discussion.data?.messages ?? []) : null;
  const code = useProjectCode(t.project_id, ask !== null && running);
  // A run that ends changes the branch: read git again then, without waiting for the interval.
  const { refetch } = code;
  useEffect(() => {
    if (!running && ask) void refetch();
  }, [running, ask, refetch]);
  if (!shown) return null;
  const branch = ticketBranch(code.data, t);
  const main = mainBranch(code.data);
  const merge = mergeCommitOf(code.data, t, branch);
  const kind = finishKind({
    column: t.column,
    asked: ask !== null,
    running,
    branch: branch ? { merged: Boolean(branch.merged), ahead: branch.ahead } : null,
    mergedCommit: merge !== null,
  });
  if (!kind) return null;
  const merged = merge !== null || Boolean(t.merge_commit);
  const pushed = Boolean(main?.upstream) && (main?.upstream_ahead ?? 0) === 0;
  return { kind, steps: finishSteps(kind, { merged, pushed }), ask, branch, main, merge, live };
}

const pulse = stylex.keyframes({ "50%": { opacity: 0.35 } });
const spin = stylex.keyframes({ to: { transform: "rotate(360deg)" } });

const TONE = {
  amber: "rgb(232 168 74 / .45)",
  live: "rgb(126 166 248 / .45)",
  ok: "rgb(95 203 149 / .45)",
  bad: "rgb(240 138 138 / .5)",
} as const;

const styles = stylex.create({
  card: {
    display: "flex",
    flexDirection: "column",
    overflow: "hidden",
    borderRadius: 12,
    borderWidth: 1,
    borderStyle: "solid",
    backgroundColor: "var(--precision-card, var(--color-background-surface))",
  },
  head: { display: "flex", alignItems: "center", gap: 10, paddingBlock: 12, paddingInline: 16 },
  title: { fontSize: 14, fontWeight: 600 },
  sub: { fontSize: 12, color: "var(--precision-text3)" },
  end: { marginInlineStart: "auto", display: "flex", alignItems: "center", gap: 6 },
  dot: { width: 8, height: 8, flexShrink: 0, borderRadius: "50%" },
  dotAmber: { backgroundColor: "var(--color-text-warning)" },
  dotLive: {
    backgroundColor: "var(--color-text-info)",
    animationName: { default: pulse, "@media (prefers-reduced-motion: reduce)": "none" },
    animationDuration: "1.4s",
    animationTimingFunction: "ease-in-out",
    animationIterationCount: "infinite",
  },
  dotOk: { backgroundColor: "var(--color-text-success)" },
  dotBad: { backgroundColor: "var(--color-text-error)" },
  track: {
    display: "grid",
    gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
    gap: 6,
    paddingInline: 16,
    paddingBottom: 12,
  },
  step: { display: "flex", minWidth: 0, flexDirection: "column", gap: 5 },
  bar: { height: 3, borderRadius: 2, backgroundColor: "var(--color-border-emphasized)" },
  barDone: { backgroundColor: "var(--color-text-success)" },
  barNow: {
    backgroundImage: "linear-gradient(90deg, var(--color-text-info) 40%, var(--color-border-emphasized) 40%)",
  },
  barAsk: { backgroundColor: "var(--color-text-warning)" },
  barStopped: { backgroundColor: "var(--color-text-error)" },
  label: {
    display: "flex",
    alignItems: "center",
    gap: 5,
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    fontSize: 12,
    color: "var(--precision-text3)",
  },
  labelDone: { color: "var(--color-text-secondary)" },
  labelOn: { fontWeight: 600, color: "var(--color-text-primary)" },
  labelStopped: { fontWeight: 600, color: "var(--color-text-error)" },
  optional: {
    display: { default: "inline", "@media (max-width: 560px)": "none" },
    borderRadius: 999,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingInline: 5,
    fontSize: 10.5,
    fontWeight: 400,
    color: "var(--precision-text3)",
  },
  body: {
    display: "flex",
    flexDirection: "column",
    gap: 10,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingBlock: 12,
    paddingInline: 16,
  },
  report: { color: "var(--color-text-secondary)" },
  facts: { display: "flex", flexWrap: "wrap", gap: 6 },
  fact: {
    display: "inline-flex",
    alignItems: "center",
    gap: 5,
    borderRadius: 6,
    paddingInline: 8,
    paddingBlock: 2,
    fontSize: 12,
    color: "var(--color-text-secondary)",
    backgroundColor: "var(--color-neutral)",
  },
  factOk: { color: "var(--color-text-success)", backgroundColor: "var(--color-success-muted)" },
  factWarn: { color: "var(--color-text-warning)", backgroundColor: "var(--color-warning-muted)" },
  factBad: { color: "var(--color-text-error)", backgroundColor: "var(--color-error-muted)" },
  mono: { fontFamily: "var(--font-family-code)", fontSize: 12 },
  asked: { display: "flex", alignItems: "flex-start", gap: 10 },
  who: { fontSize: 12, color: "var(--precision-text3)" },
  bubble: {
    display: "inline-block",
    marginTop: 2,
    borderRadius: 8,
    paddingInline: 10,
    paddingBlock: 4,
    backgroundColor: "var(--color-neutral)",
  },
  lines: { margin: 0, display: "flex", flexDirection: "column", gap: 2, padding: 0, listStyleType: "none" },
  line: {
    display: "grid",
    gridTemplateColumns: "18px minmax(0, 1fr)",
    alignItems: "center",
    gap: 8,
    paddingBlock: 3,
  },
  lineDone: { color: "var(--color-text-secondary)" },
  icon: {
    display: "grid",
    width: 16,
    height: 16,
    placeItems: "center",
    borderRadius: "50%",
    fontSize: 10,
    fontWeight: 800,
  },
  iconDone: { color: "var(--color-text-success)", backgroundColor: "var(--color-success-muted)" },
  iconBad: { color: "var(--color-text-error)", backgroundColor: "var(--color-error-muted)" },
  iconNow: {
    boxSizing: "border-box",
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: "var(--color-text-info)",
    borderRightColor: "transparent",
    animationName: { default: spin, "@media (prefers-reduced-motion: reduce)": "none" },
    animationDuration: "0.9s",
    animationTimingFunction: "linear",
    animationIterationCount: "infinite",
  },
  code: { fontFamily: "var(--font-family-code)", fontSize: 12, color: "var(--color-text-secondary)" },
  result: {
    display: "grid",
    gridTemplateColumns: { default: "auto minmax(0, 1fr)", "@media (max-width: 560px)": "1fr" },
    gap: "4px 14px",
    margin: 0,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    paddingBlock: 10,
    paddingInline: 12,
  },
  dt: { fontSize: 12, color: "var(--precision-text3)" },
  dd: { margin: 0, minWidth: 0, overflowWrap: "anywhere" },
  files: { margin: 0, padding: 0, listStyleType: "none" },
  file: { fontFamily: "var(--font-family-code)", fontSize: 12, color: "var(--color-text-error)" },
  green: { color: "var(--color-text-success)" },
  foot: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 8,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingBlock: 10,
    paddingInline: 16,
  },
  why: { fontSize: 12, color: "var(--precision-text3)" },
  sep: {
    display: { default: "block", "@media (max-width: 560px)": "none" },
    width: 1,
    height: 20,
    backgroundColor: "var(--color-border)",
  },
  link: { marginInlineStart: "auto", whiteSpace: "nowrap" },
  feedback: { display: "flex", flexDirection: "column", gap: 8 },
  textarea: {
    boxSizing: "border-box",
    width: "100%",
    minHeight: 64,
    resize: "vertical",
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "transparent",
    padding: 8,
    color: "inherit",
    font: "inherit",
  },
  error: { fontSize: 12, color: "var(--color-text-error)" },
  waitLine: { paddingInline: 16, paddingBottom: 10, fontSize: 12, color: "var(--precision-text3)" },
  trackList: { margin: 0, listStyleType: "none" },
  hidden: {
    position: "absolute",
    width: 1,
    height: 1,
    overflow: "hidden",
    clipPath: "inset(50%)",
    whiteSpace: "nowrap",
  },
});

const DOT = { amber: styles.dotAmber, live: styles.dotLive, ok: styles.dotOk, bad: styles.dotBad } as const;

/** The finish line: Review → Merge → Push (optional) → Done. */
function Track({ steps }: { steps: FinishStep[] }) {
  return (
    <ol aria-label="Steps to finish" {...stylex.props(styles.track, styles.trackList)}>
      {steps.map((s) => (
        <li key={s.label} {...stylex.props(styles.step)}>
          <span
            aria-hidden="true"
            {...stylex.props(
              styles.bar,
              s.state === "done" && styles.barDone,
              s.state === "now" && styles.barNow,
              s.state === "ask" && styles.barAsk,
              s.state === "stopped" && styles.barStopped,
            )}
          />
          <span
            {...stylex.props(
              styles.label,
              s.state === "done" && styles.labelDone,
              (s.state === "now" || s.state === "ask") && styles.labelOn,
              s.state === "stopped" && styles.labelStopped,
            )}
          >
            {s.label}
            {s.state === "open" ? null : <span {...stylex.props(styles.hidden)}>{` (${STATE_WORD[s.state]})`}</span>}
            {s.optional ? <span {...stylex.props(styles.optional)}>optional</span> : null}
          </span>
        </li>
      ))}
    </ol>
  );
}

const STATE_WORD: Record<FinishStep["state"], string> = {
  done: "done",
  now: "in progress",
  ask: "your turn",
  stopped: "stopped",
  open: "to do",
};

function Lines({ lines, label }: { lines: FinishLine[]; label?: string }) {
  if (lines.length === 0) return null;
  return (
    <ul aria-label={label} {...stylex.props(styles.lines)}>
      {lines.map((l) => (
        <li key={l.id} {...stylex.props(styles.line, l.state === "done" && styles.lineDone)}>
          <span
            aria-hidden="true"
            {...stylex.props(
              styles.icon,
              l.state === "done" && styles.iconDone,
              l.state === "failed" && styles.iconBad,
              l.state === "now" && styles.iconNow,
            )}
          >
            {l.state === "done" ? "✓" : l.state === "failed" ? "!" : ""}
          </span>
          <span>
            {l.text}
            {l.command ? (
              <>
                {" "}
                <code {...stylex.props(styles.code)}>{l.command}</code>
              </>
            ) : null}
            {l.state === "failed" ? " · failed" : ""}
          </span>
        </li>
      ))}
    </ul>
  );
}

const shortTime = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });

/**
 * The ticket's finish card in Review and on Done. `report` is the owner's report; `quick` tickets
 * keep their own Approve and merge, which the daemon does on accept.
 */
export function FinishCard({ detail, t, finish }: { detail: TicketDetail; t: TicketView; finish: Finish }) {
  const connection = useConnection();
  const write = useTicketWrite();
  const { status, user, hostId } = useLinkState();
  const { storage } = usePlatform();
  const online = status === "online";
  const owner = t.owner_name ?? "The owner";
  const root = detail.root_task ?? null;
  const report = root?.result ?? null;
  const quick = isQuick(t);
  const { kind, branch, main, merge, live, ask } = finish;
  const tone = finishTone(kind);
  const now = useNow(kind === "merging", 1000);
  const check = doneCheck(detail);
  const ready = check.kind === "ready" && online;
  const autoKey = `${AUTO_DONE_KEY}.${hostId ?? ""}`;
  const [autoDone, setAutoDone] = useState(() => storage.get(autoKey) !== "0");
  const [mode, setMode] = useState<"back" | null>(null);
  const [feedback, setFeedback] = useState("");
  const [missing, setMissing] = useState(false);
  const [undoing, setUndoing] = useState(false);
  const { busy, held, error, send, sendAnyway } = useSend((text) => {
    if (text !== "") toast(text);
    setMode(null);
    setFeedback("");
  });
  const tell = (text: string, done = "") =>
    send(async (confirmSecret) => {
      await connection.write("ticket.post", { ticket_id: t.id, text, confirm_secret: confirmSecret });
      return done;
    });
  const accept = () =>
    void write(
      "ticket.accept",
      { ticket_id: t.id },
      { done: quick ? `Merging ${ticketNumber(t)} into main…` : `${ticketNumber(t)} is Done.` },
    );
  // Mark done when merged: once per ticket version, only while the page is open.
  const fired = useRef<number | null>(null);
  useEffect(() => {
    if (!autoDoneDue(kind, autoDone, ready) || fired.current === t.version) return;
    fired.current = t.version;
    void write("ticket.accept", { ticket_id: t.id }, { done: `Merged and marked ${ticketNumber(t)} done.` });
  }, [kind, autoDone, ready, t.id, t.version, write, t]);
  const sendBack = () => {
    if (mode !== "back") return setMode("back");
    if (feedback.trim() === "") return setMissing(true);
    setMissing(false);
    send(async (confirmSecret) => {
      await connection.write(
        "ticket.send_back",
        { ticket_id: t.id, feedback: feedback.trim(), confirm_secret: confirmSecret },
        { retry: false },
      );
      return `${ticketNumber(t)} went back to Doing.`;
    });
  };
  const stop = () => {
    if (!live.liveRun) return;
    connection
      .write("run.stop", { run_id: live.liveRun }, { retry: false })
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  };
  // Nothing of the branch is ahead of main (or there is no branch): there is nothing to merge.
  const nothingToMerge = branch === null || branch.ahead === 0;
  const runLink = t.root_task_id ? (
    <Link href={`/tasks/${t.root_task_id}`} xstyle={styles.link}>
      {kind === "merging" ? "Watch the run →" : "Open run →"}
    </Link>
  ) : null;
  const linesAfterAsk =
    ask && live.latest && live.latest.atMs >= ask.atMs ? finishLines(live.latest.activity) : ([] as FinishLine[]);
  const asker = ask?.author && ask.author === user?.username ? "You" : (ask?.author ?? "You");
  const kbd = <KeyHint onAccent>{showChord("secondary-enter", isMac)}</KeyHint>;
  const conflicts = branch?.conflicts ?? [];
  const head = (() => {
    switch (kind) {
      case "decide": {
        const todos = live.todos;
        const steps =
          todos && todos.length > 0 ? `${owner} finished ${stepText(todos).replace(" done", " steps")}` : null;
        const took = root ? minutes(root.updated_at_ms - root.created_at_ms) : null;
        return {
          title: "Ready for your review",
          sub: [steps ?? `${owner} finished`, took].filter(Boolean).join(" · "),
        };
      }
      case "merging":
        return {
          title: `${owner} is merging into main`,
          sub: ask ? `${asker} asked at ${shortTime(ask.atMs)} · ${clock(now - ask.atMs)} so far` : "",
        };
      case "merged":
        return {
          title: "Merged into main",
          sub: [merge ? `At ${shortTime(merge.at_ms)}` : null, pushedWord(main)].filter(Boolean).join(" · "),
        };
      case "conflict":
        return {
          title:
            conflicts.length > 0
              ? `The merge stopped: ${count(conflicts.length, "file")} conflict`
              : "The merge stopped",
          sub: [
            branch && branch.behind > 0 ? `Main moved ${count(branch.behind, "commit")} since this branch` : null,
            "nothing was changed on main",
          ]
            .filter(Boolean)
            .join(" · "),
        };
      case "done":
        return { title: "Done", sub: t.done_at_ms ? `Marked done at ${shortTime(t.done_at_ms)}` : "" };
    }
  })();
  const testFacts = factsOf(live.all).filter((f) => f.text !== "Saved a commit" && !f.text.startsWith("Changed"));
  return (
    <section aria-label="Finish" aria-live="polite" {...stylex.props(styles.card)} style={{ borderColor: TONE[tone] }}>
      {kind === "decide" || kind === "merged" ? (
        <GateKeys
          onYes={() => {
            if (kind === "merged" && ready) accept();
            else if (kind === "decide" && online && !busy) {
              if (quick || nothingToMerge) {
                if (ready) accept();
              } else tell(MERGE_ASK);
            }
          }}
          onNo={() => setMode("back")}
        />
      ) : null}
      <div {...stylex.props(styles.head)}>
        <span aria-hidden="true" {...stylex.props(styles.dot, DOT[tone])} />
        <div>
          <div {...stylex.props(styles.title)}>{head.title}</div>
          {head.sub ? <div {...stylex.props(styles.sub)}>{head.sub}</div> : null}
        </div>
        {kind === "merging" && live.liveRun ? (
          <div {...stylex.props(styles.end)}>
            <Button variant="ghost" size="sm" label="Stop" onClick={stop} />
          </div>
        ) : null}
      </div>
      <Track steps={finish.steps} />

      {kind === "decide" ? (
        <div {...stylex.props(styles.body)}>
          <div {...stylex.props(styles.report)}>
            {report ? (
              <Clamped text={report} more="Read the whole report" />
            ) : (
              <span>No report text. See the conversation.</span>
            )}
          </div>
          <div {...stylex.props(styles.facts)}>
            {branch ? <span {...stylex.props(styles.fact, styles.mono)}>{branch.name}</span> : null}
            {branch && branch.ahead > 0 ? (
              <span {...stylex.props(styles.fact)}>{`${count(branch.ahead, "commit")} ahead of main`}</span>
            ) : null}
            {testFacts.map((f) => (
              <span
                key={f.text}
                {...stylex.props(styles.fact, f.tone === "good" && styles.factOk, f.tone === "bad" && styles.factBad)}
              >
                {f.tone === "good" ? `✓ ${f.text}` : f.text}
              </span>
            ))}
            {branch && branch.ahead > 0 && branch.conflicts ? (
              branch.conflicts.length === 0 ? (
                <span {...stylex.props(styles.fact)}>Merges cleanly</span>
              ) : (
                <span {...stylex.props(styles.fact, styles.factWarn)}>
                  {`Conflicts with main in ${count(branch.conflicts.length, "file")}`}
                </span>
              )
            ) : null}
          </div>
        </div>
      ) : null}

      {kind === "merging" ? (
        <div {...stylex.props(styles.body)}>
          {ask ? (
            <div {...stylex.props(styles.asked)}>
              <Mark letter={initial(ask.author ?? "Y")} color={markColor(ask.author ?? "")} size={22} radius={6} />
              <div>
                <div {...stylex.props(styles.who)}>{`${asker} · ${shortTime(ask.atMs)}`}</div>
                <span {...stylex.props(styles.bubble)}>Merge into main</span>
              </div>
            </div>
          ) : null}
          <Lines lines={linesAfterAsk} label={`What ${owner} is doing`} />
        </div>
      ) : null}

      {kind === "merged" ? (
        <div {...stylex.props(styles.body)}>
          <dl {...stylex.props(styles.result)}>
            {merge ? (
              <>
                <dt {...stylex.props(styles.dt)}>Commit</dt>
                <dd {...stylex.props(styles.dd)}>
                  <code {...stylex.props(styles.mono)}>{merge.hash.slice(0, 8)}</code> {merge.subject}
                </dd>
              </>
            ) : null}
            {main?.upstream ? (
              <>
                <dt {...stylex.props(styles.dt)}>Main</dt>
                <dd {...stylex.props(styles.dd)}>
                  {(main.upstream_ahead ?? 0) > 0
                    ? `${count(main.upstream_ahead ?? 0, "commit")} ahead of ${main.upstream}`
                    : `Up to date with ${main.upstream}`}
                </dd>
              </>
            ) : null}
            {testFacts.length > 0 ? (
              <>
                <dt {...stylex.props(styles.dt)}>Checks</dt>
                <dd {...stylex.props(styles.dd)}>
                  {testFacts.map((f, i) => (
                    <span key={f.text}>
                      {i > 0 ? " · " : ""}
                      {f.tone === "good" ? <span {...stylex.props(styles.green)}>✓</span> : null} {f.text}
                    </span>
                  ))}
                </dd>
              </>
            ) : null}
            <dt {...stylex.props(styles.dt)}>Cleaned up</dt>
            <dd {...stylex.props(styles.dd)}>
              {branch === null
                ? "Branch removed"
                : branch.worktree_path
                  ? "Branch and worktree still there"
                  : "Branch still there"}
            </dd>
          </dl>
        </div>
      ) : null}

      {kind === "conflict" ? (
        <div {...stylex.props(styles.body)}>
          <Lines lines={linesAfterAsk} />
          <dl {...stylex.props(styles.result)}>
            {conflicts.length > 0 ? (
              <>
                <dt {...stylex.props(styles.dt)}>Conflicts</dt>
                <dd {...stylex.props(styles.dd)}>
                  <ul {...stylex.props(styles.files)}>
                    {conflicts.map((f) => (
                      <li key={f} {...stylex.props(styles.file)}>
                        {f}
                      </li>
                    ))}
                  </ul>
                </dd>
              </>
            ) : null}
            <dt {...stylex.props(styles.dt)}>Branch</dt>
            <dd {...stylex.props(styles.dd)}>
              {branch
                ? `${branch.name} · ${count(branch.ahead, "commit")} not on main${branch.behind > 0 ? ` · ${count(branch.behind, "commit")} behind` : ""}`
                : "Unknown"}
            </dd>
          </dl>
        </div>
      ) : null}

      {kind === "done" ? (
        <div {...stylex.props(styles.body)}>
          <div {...stylex.props(styles.facts)}>
            {merge || t.merge_commit ? (
              <span {...stylex.props(styles.fact, styles.factOk)}>
                {`Merged · ${(merge?.hash ?? t.merge_commit ?? "").slice(0, 8)}`}
              </span>
            ) : (
              <span {...stylex.props(styles.fact)}>Not merged</span>
            )}
            {main?.upstream ? <span {...stylex.props(styles.fact)}>{pushedWord(main)}</span> : null}
            {root && !root.live_run ? <span {...stylex.props(styles.fact)}>{`${owner}'s run ended`}</span> : null}
          </div>
        </div>
      ) : null}

      {mode === "back" ? (
        <div {...stylex.props(styles.body, styles.feedback)}>
          <textarea
            aria-label="What needs to change?"
            placeholder="What needs to change?"
            value={feedback}
            onChange={(e) => setFeedback(e.target.value)}
            {...stylex.props(styles.textarea)}
          />
          {missing ? <span {...stylex.props(styles.error)}>Write what needs to change.</span> : null}
        </div>
      ) : null}
      {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
      {error ? <Problem>{error}</Problem> : null}

      <div {...stylex.props(styles.foot)}>
        {kind === "decide" ? (
          <>
            {quick || nothingToMerge ? (
              <Button
                variant="primary"
                label={quick ? acceptLabel(t) : "Mark done"}
                isDisabled={!ready}
                onClick={accept}
              >
                {quick ? acceptLabel(t) : "Mark done"} {kbd}
              </Button>
            ) : (
              <>
                <Button
                  variant="primary"
                  label="Merge into main"
                  isDisabled={!online || busy}
                  onClick={() => tell(MERGE_ASK)}
                >
                  Merge into main {kbd}
                </Button>
                <CheckboxInput
                  label="Mark done when merged"
                  value={autoDone}
                  onChange={() => {
                    storage.set(autoKey, autoDone ? "0" : "1");
                    setAutoDone(!autoDone);
                  }}
                />
              </>
            )}
            <span aria-hidden="true" {...stylex.props(styles.sep)} />
            <Button
              label="Explain what you did"
              isDisabled={!online || busy}
              onClick={() => tell(EXPLAIN_ASK, `Asked ${owner} to explain.`)}
            />
            <Button label="Send back to Doing" isDisabled={!online || busy} onClick={sendBack} />
            {runLink}
          </>
        ) : kind === "merging" ? (
          <>
            <Button variant="primary" label="Mark done" isDisabled />
            <span {...stylex.props(styles.why)}>
              {autoDone
                ? "Marked done automatically when the merge succeeds."
                : "Mark done waits until the merge has finished."}
            </span>
            {runLink}
          </>
        ) : kind === "merged" ? (
          <>
            <Button variant="primary" label="Mark done" isDisabled={!ready} onClick={accept}>
              Mark done {kbd}
            </Button>
            {main?.upstream && (main.upstream_ahead ?? 0) > 0 ? (
              <Button
                label="Push to origin"
                isDisabled={!online || busy}
                onClick={() => tell(PUSH_ASK, `Asked ${owner} to push main.`)}
              />
            ) : null}
            <span aria-hidden="true" {...stylex.props(styles.sep)} />
            <Button
              variant="ghost"
              label="Undo the merge"
              isDisabled={!online || busy}
              onClick={() => setUndoing(true)}
            />
            {runLink}
          </>
        ) : kind === "conflict" ? (
          <>
            <Button
              variant="primary"
              label={`Ask ${owner} to resolve and merge`}
              isDisabled={!online || busy}
              onClick={() => tell(RESOLVE_ASK)}
            />
            <Button label="Send back to Doing" isDisabled={!online || busy} onClick={sendBack} />
            {runLink}
          </>
        ) : (
          runLink
        )}
      </div>
      {check.kind === "working" && kind !== "merging" && kind !== "done" ? (
        <span {...stylex.props(styles.waitLine)}>Still at work: Done waits until that has finished.</span>
      ) : null}
      <ConfirmDialog
        open={undoing}
        onClose={() => setUndoing(false)}
        title={`Undo the merge of ${ticketNumber(t)}?`}
        body={`${owner} reverts the merge commit on main and keeps the branch. Nothing is pushed.`}
        confirm="Undo the merge"
        danger
        onConfirm={() => tell(UNDO_ASK, `Asked ${owner} to undo the merge.`)}
      />
    </section>
  );
}

/** "not pushed", "pushed", or nothing when main has no remote. */
function pushedWord(main: CodeBranch | null): string | null {
  if (!main?.upstream) return null;
  return (main.upstream_ahead ?? 0) > 0 ? "Not pushed" : "Pushed";
}

/** The main task's state in the properties while the card has its own word for it. */
export function finishRunText(finish: Finish | null): { text: string; tone: "green" | "blue" | "red" } | null {
  switch (finish?.kind) {
    case "merging":
      return { text: "Running · merging into main", tone: "blue" };
    case "merged":
      return { text: "Finished · merged", tone: "green" };
    case "conflict":
      return { text: "Stopped · merge conflict", tone: "red" };
    case "done":
      return finish.merge ? { text: "Finished · merged", tone: "green" } : null;
    default:
      return null;
  }
}
