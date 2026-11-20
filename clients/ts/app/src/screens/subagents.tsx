// The subagents of a running answer in a side panel, and one of them as a chat log that cannot be
// answered: what it was asked, the steps it took and what it reported.

import { Button } from "@astryxdesign/core/Button";
import { ChatToolCalls } from "@astryxdesign/core/Chat";
import { expectBody } from "@aurelius/client";
import type { ActivityView, AgentView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { Markdown } from "../ui/markdown";
import { initial, Mark, markColor } from "../ui/marks";
import { stepsLabel } from "./answer-steps-logic";
import { fold } from "./chat-logic";
import { ActivityOutput } from "./chat-message";
import { FormDialog, Hint } from "./org-kit";
import { elapsedText, stepBatches, subagentDoingText, subagentKey, subagentReport, subagentSteps } from "./run-logic";
import { stepLine } from "./tasks-logic";

/** The server names the subagent behind each call of a run. */
export const SUBAGENT_STEPS_FEATURE = "subagent_steps";

/** How often the panel and an open subagent ask again; nothing tells when they change. */
const POLL_MS = 2000;

const styles = stylex.create({
  // A block of the properties card: space above it and a bold heading, as the other blocks.
  side: { marginTop: 16 },
  sideTitle: { marginTop: 0, marginBottom: 6, fontSize: 13, fontWeight: 600, color: "var(--color-text-primary)" },
  agent: {
    display: "flex",
    width: "100%",
    cursor: "pointer",
    flexDirection: "column",
    borderWidth: 0,
    borderRadius: 5,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    paddingBlock: 4,
    paddingInline: 4,
    marginInline: -4,
    textAlign: "start",
    color: "inherit",
    font: "inherit",
    fontSize: 12,
  },
  agentHead: { display: "flex", justifyContent: "space-between", gap: 8 },
  agentName: { overflow: "hidden", whiteSpace: "nowrap", textOverflow: "ellipsis", fontWeight: 500 },
  muted: { flexShrink: 0, color: "var(--precision-text3)" },
  agentDoing: {
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: "var(--precision-text3)",
  },
  line: { margin: 0, fontSize: 12, color: "var(--color-text-secondary)" },
  // The log scrolls on its own, so it can follow the bottom while the subagent works.
  log: {
    display: "flex",
    maxHeight: "min(64vh, 620px)",
    flexDirection: "column",
    gap: 18,
    overflowY: "auto",
    paddingBlock: 4,
    paddingInlineEnd: 4,
  },
  entry: { display: "grid", gridTemplateColumns: "28px minmax(0,1fr)", columnGap: 12 },
  entryBody: { display: "flex", minWidth: 0, flexDirection: "column", gap: 6 },
  entryHead: { display: "flex", alignItems: "baseline", gap: 8, margin: 0, fontSize: 13 },
  entryName: { fontWeight: 600 },
  entryNote: { fontSize: 12, color: "var(--precision-text3)" },
  batch: { display: "flex", flexDirection: "column", gap: 4 },
  quiet: {
    display: "inline-flex",
    alignItems: "center",
    alignSelf: "flex-start",
    gap: 6,
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    fontSize: 13,
    textAlign: "left",
    color: { default: "var(--color-text-secondary)", ":hover": "var(--color-text-primary)" },
  },
  failed: { color: "var(--color-text-error)" },
  working: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    margin: 0,
    paddingInlineStart: 40,
    fontSize: 12,
    color: "var(--color-text-accent)",
  },
  workingDot: { width: 6, height: 6, borderRadius: 3, backgroundColor: "var(--color-text-accent)" },
  foot: { display: "flex", flexGrow: 1, alignItems: "center", justifyContent: "space-between", gap: 12 },
  readOnly: { fontSize: 12, color: "var(--precision-text3)" },
});

/**
 * The helpers of a running answer, while the run lasts. A row opens what the subagent was asked
 * and the steps it took, which follow along while it runs.
 */
export function SubagentsAtWork({
  conversationId,
  live,
  askedBy = "the employee",
}: {
  conversationId: string;
  live: boolean;
  /** The employee whose run started the subagents: the one who asked them. */
  askedBy?: string;
}) {
  const connection = useConnection();
  const [agents, setAgents] = useState<AgentView[]>([]);
  const [opened, setOpened] = useState<AgentView | null>(null);
  useEffect(() => {
    if (!live) {
      setAgents([]);
      return;
    }
    const read = () =>
      connection
        .call("subagent.list", { conversation_id: conversationId })
        .then((body) => setAgents(expectBody(body, "subagents").value))
        .catch(() => setAgents([]));
    void read();
    const tick = setInterval(read, POLL_MS);
    return () => clearInterval(tick);
  }, [connection, conversationId, live]);
  const now = Date.now();
  // The open one as the latest list tells it; once it is done, the last that was seen.
  const shown = opened ? (agents.find((a) => subagentKey(a) === subagentKey(opened)) ?? null) : null;
  const dialog = opened ? (
    <SubagentDialog
      agent={shown ?? opened}
      askedBy={askedBy}
      done={shown === null}
      now={now}
      onClose={() => setOpened(null)}
    />
  ) : null;
  if (agents.length === 0) return dialog;
  return (
    <div {...stylex.props(styles.side)}>
      <p {...stylex.props(styles.sideTitle)}>Subagents at work</p>
      {agents.map((a) => (
        <button
          key={subagentKey(a)}
          type="button"
          title="Show what this subagent was asked and what it did"
          onClick={() => setOpened(a)}
          {...stylex.props(styles.agent)}
        >
          <span {...stylex.props(styles.agentHead)}>
            <span {...stylex.props(styles.agentName)}>{a.name}</span>
            <span {...stylex.props(styles.muted)}>{elapsedText(now - a.since_ms)}</span>
          </span>
          <span {...stylex.props(styles.agentDoing)}>{a.doing ?? a.detail}</span>
        </button>
      ))}
      {dialog}
    </div>
  );
}

/** Steps per folded line in a subagent's log: a long run reads as a handful of lines, not hundreds. */
const STEP_BATCH = 10;

/**
 * One subagent as a chat log that cannot be answered: what the employee asked it, its steps in folded
 * lines, and its report once it is done. While it runs the log follows along at the bottom.
 */
function SubagentDialog({
  agent,
  askedBy,
  done,
  now,
  onClose,
}: {
  agent: AgentView;
  askedBy: string;
  done: boolean;
  now: number;
  onClose: () => void;
}) {
  return (
    <FormDialog
      title={agent.name}
      width={760}
      onClose={onClose}
      footer={
        <div {...stylex.props(styles.foot)}>
          <span {...stylex.props(styles.readOnly)}>
            Read only: a subagent reports to {askedBy}; you cannot write to it.
          </span>
          <Button label="Close" onClick={onClose} />
        </div>
      }
    >
      <p {...stylex.props(styles.line)}>
        {[agent.detail || "Subagent", done ? "done" : `running ${elapsedText(now - agent.since_ms)}`].join(" · ")}
      </p>
      <SubagentLog agent={agent} askedBy={askedBy} done={done} />
    </FormDialog>
  );
}

/** The log itself; it reads the subagent's steps again while it runs. */
function SubagentLog({ agent, askedBy, done }: { agent: AgentView; askedBy: string; done: boolean }) {
  const connection = useConnection();
  const { features } = useLinkState();
  const prompt = agent.prompt?.trim() ?? "";
  const runId = agent.run_id ?? null;
  const callId = agent.call_id ?? null;
  const withSteps = features.includes(SUBAGENT_STEPS_FEATURE) && runId !== null && callId !== null;
  const [read, setRead] = useState<{ messageId: string | null; steps: ActivityView[]; report: string | null } | null>(
    null,
  );
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    if (!withSteps || !runId) return;
    let gone = false;
    const load = () =>
      connection
        .call("run.get_activity", { run_id: runId })
        .then((body) => {
          if (gone) return;
          const activity = expectBody(body, "run_activity").value;
          setRead({
            messageId: activity.message_id ?? null,
            steps: subagentSteps(activity.calls, callId),
            report: subagentReport(activity.calls, callId),
          });
          setProblem(null);
        })
        .catch((e: unknown) => {
          if (!gone) setProblem(e instanceof Error ? e.message : "The steps did not load.");
        });
    void load();
    // Once more after it is done, for its last step and its report; then it no longer changes.
    const tick = done ? null : setInterval(load, POLL_MS);
    return () => {
      gone = true;
      if (tick) clearInterval(tick);
    };
  }, [connection, withSteps, runId, callId, done]);

  // Follow the bottom while it runs, unless the reader scrolled up to read.
  const scroller = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  const count = read?.steps.length ?? 0;
  // biome-ignore lint/correctness/useExhaustiveDependencies: only new steps or a report move it.
  useEffect(() => {
    const el = scroller.current;
    if (el && atBottom.current) el.scrollTop = el.scrollHeight;
  }, [count, read?.report]);

  const batches = stepBatches(read?.steps ?? [], STEP_BATCH);
  return (
    <div
      ref={scroller}
      role="log"
      aria-label={`What ${agent.name} did`}
      onScroll={(e) => {
        const el = e.currentTarget;
        atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
      }}
      {...stylex.props(styles.log)}
    >
      {prompt ? (
        <LogEntry who={askedBy} note="asked">
          <FoldedText text={prompt} />
        </LogEntry>
      ) : null}
      {withSteps ? (
        <LogEntry who={agent.name} note={read ? stepCount(read.steps.length) : undefined}>
          {problem ? <Hint>{problem}</Hint> : null}
          {read && read.steps.length === 0 && !done ? <p {...stylex.props(styles.line)}>Starting…</p> : null}
          {read
            ? batches.map((batch, i) => (
                <StepBatch
                  key={batch[0]?.call_id ?? i}
                  steps={batch}
                  messageId={read.messageId}
                  open={!done && i === batches.length - 1}
                />
              ))
            : null}
        </LogEntry>
      ) : (
        <LogEntry who={agent.name}>
          <p {...stylex.props(styles.line)}>
            {done ? `Last step: ${subagentDoingText(agent)}` : `Now: ${subagentDoingText(agent)}`}
          </p>
        </LogEntry>
      )}
      {!done ? (
        <p role="status" {...stylex.props(styles.working)}>
          <span {...stylex.props(styles.workingDot)} />
          Working · {agent.doing ?? "starting…"}
        </p>
      ) : null}
      {read?.report ? (
        <LogEntry who={agent.name} note="report">
          <FoldedText text={read.report} />
        </LogEntry>
      ) : null}
    </div>
  );
}

const stepCount = (n: number) => (n === 1 ? "1 step" : `${n} steps`);

/** One message in the log: who, a quiet note, and what they said or did. */
function LogEntry({ who, note, children }: { who: string; note?: string | undefined; children: ReactNode }) {
  return (
    <div {...stylex.props(styles.entry)}>
      <Mark letter={initial(who)} color={markColor(who)} size={28} radius={7} />
      <div {...stylex.props(styles.entryBody)}>
        <p {...stylex.props(styles.entryHead)}>
          <span {...stylex.props(styles.entryName)}>{who}</span>
          {note ? <span {...stylex.props(styles.entryNote)}>{note}</span> : null}
        </p>
        {children}
      </div>
    </div>
  );
}

/** Long text folds, as in the chat. */
function FoldedText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const folded = fold(text);
  return (
    <>
      <Markdown text={folded && !open ? folded.shown : text} />
      {folded ? (
        <button type="button" onClick={() => setOpen(!open)} {...stylex.props(styles.quiet)}>
          {open ? "▴ Show less" : `▾ Show more (${folded.hidden} lines)`}
        </button>
      ) : null}
    </>
  );
}

/** A batch of steps as one quiet line, as the chat folds steps; open, each step shows its output. */
function StepBatch({
  steps,
  messageId,
  open: openAtFirst,
}: {
  steps: ActivityView[];
  /** Without it (an older server) a step shows no output. */
  messageId: string | null;
  open: boolean;
}) {
  const [open, setOpen] = useState(openAtFirst);
  const failed = steps.some((s) => s.ok === false);
  return (
    <div {...stylex.props(styles.batch)}>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        {...stylex.props(styles.quiet, failed && styles.failed)}
      >
        <span aria-hidden="true">{open ? "⌄" : "›"}</span>
        {stepsLabel(steps)}
      </button>
      {open
        ? steps.map((step) => (
            <ChatToolCalls
              key={step.call_id}
              calls={[
                {
                  name: step.tool,
                  status: step.ok == null ? "running" : step.ok ? "complete" : "error",
                  target: stepLine(step),
                  ...(step.ok === false && step.output ? { errorMessage: step.output } : {}),
                  ...(messageId ? { resultDetail: <ActivityOutput step={step} messageId={messageId} /> } : {}),
                },
              ]}
            />
          ))
        : null}
    </div>
  );
}
