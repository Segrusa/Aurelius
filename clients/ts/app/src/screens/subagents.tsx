// The subagents of a running answer in a side panel, and what one of them was asked and did.

import { Button } from "@astryxdesign/core/Button";
import { ChatToolCalls } from "@astryxdesign/core/Chat";
import { expectBody } from "@aurelius/client";
import type { ActivityView, AgentView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { ActivityOutput } from "./chat-message";
import { FormDialog, Hint } from "./org-kit";
import { elapsedText, subagentDoingText, subagentKey, subagentSteps } from "./run-logic";
import { stepLine } from "./tasks-logic";

/** The server names the subagent behind each call of a run. */
export const SUBAGENT_STEPS_FEATURE = "subagent_steps";

/** How often the panel and an open subagent ask again; nothing tells when they change. */
const POLL_MS = 2000;

const styles = stylex.create({
  side: {
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingTop: 16,
  },
  sideTitle: { marginTop: 0, marginBottom: 8, fontSize: 12, fontWeight: 500, color: "var(--precision-text3)" },
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
  prompt: {
    margin: 0,
    maxHeight: 200,
    overflow: "auto",
    borderRadius: 6,
    backgroundColor: "var(--precision-sunk)",
    padding: 8,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    whiteSpace: "pre-wrap",
  },
  steps: { display: "flex", flexDirection: "column", gap: 2 },
});

/**
 * The helpers of a running answer, while the run lasts. A row opens what the subagent was asked
 * and the steps it took, which follow along while it runs.
 */
export function SubagentsAtWork({ conversationId, live }: { conversationId: string; live: boolean }) {
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
    <SubagentDialog agent={shown ?? opened} done={shown === null} now={now} onClose={() => setOpened(null)} />
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

/** One subagent: its type, how long it runs, what it was asked and every step it took. */
function SubagentDialog({
  agent,
  done,
  now,
  onClose,
}: {
  agent: AgentView;
  done: boolean;
  now: number;
  onClose: () => void;
}) {
  const { features } = useLinkState();
  const prompt = agent.prompt?.trim() ?? "";
  const withSteps = features.includes(SUBAGENT_STEPS_FEATURE) && Boolean(agent.run_id && agent.call_id);
  return (
    <FormDialog title={agent.name} width={720} onClose={onClose} footer={<Button label="Close" onClick={onClose} />}>
      <p {...stylex.props(styles.line)}>
        {[agent.detail || "Subagent", done ? "done" : `running ${elapsedText(now - agent.since_ms)}`].join(" · ")}
      </p>
      {prompt ? (
        <>
          <p {...stylex.props(styles.sideTitle)}>What it was asked</p>
          <pre {...stylex.props(styles.prompt)}>{prompt}</pre>
        </>
      ) : null}
      {withSteps && agent.run_id && agent.call_id ? (
        <SubagentStepList runId={agent.run_id} callId={agent.call_id} live={!done} />
      ) : (
        <p {...stylex.props(styles.line)}>
          {done ? `Last step: ${subagentDoingText(agent)}` : `Now: ${subagentDoingText(agent)}`}
        </p>
      )}
    </FormDialog>
  );
}

/** Every call the subagent made, as the chat shows steps; read again while it runs. */
function SubagentStepList({ runId, callId, live }: { runId: string; callId: string; live: boolean }) {
  const connection = useConnection();
  const [read, setRead] = useState<{ messageId: string | null; steps: ActivityView[] } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    let gone = false;
    const load = () =>
      connection
        .call("run.get_activity", { run_id: runId })
        .then((body) => {
          if (gone) return;
          const activity = expectBody(body, "run_activity").value;
          setRead({ messageId: activity.message_id ?? null, steps: subagentSteps(activity.calls, callId) });
          setProblem(null);
        })
        .catch((e: unknown) => {
          if (!gone) setProblem(e instanceof Error ? e.message : "The steps did not load.");
        });
    void load();
    // Once more after it is done, for its last step; then it no longer changes.
    const tick = live ? setInterval(load, POLL_MS) : null;
    return () => {
      gone = true;
      if (tick) clearInterval(tick);
    };
  }, [connection, runId, callId, live]);
  return (
    <>
      <p {...stylex.props(styles.sideTitle)}>{read ? `Steps (${read.steps.length})` : "Steps"}</p>
      {problem ? <Hint>{problem}</Hint> : null}
      {read && read.steps.length === 0 ? <Hint>No steps yet.</Hint> : null}
      {read ? (
        <div {...stylex.props(styles.steps)}>
          {read.steps.map((step) => (
            <ChatToolCalls
              key={step.call_id}
              calls={[
                {
                  name: step.tool,
                  status: step.ok == null ? "running" : step.ok ? "complete" : "error",
                  target: stepLine(step),
                  ...(step.ok === false && step.output ? { errorMessage: step.output } : {}),
                  ...(read.messageId
                    ? { resultDetail: <ActivityOutput step={step} messageId={read.messageId} /> }
                    : {}),
                },
              ]}
            />
          ))}
        </div>
      ) : null}
    </>
  );
}
