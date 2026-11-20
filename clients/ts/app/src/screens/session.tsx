// A work session (server feature `work_sessions`): one employee in one project's folder, started from
// New session without a ticket. It is the employee chat with its own head: the session's title and where
// it works. Later messages go in with `chat.send`, as in a chat. Below the words, as in Claude's own
// sessions, it chooses what the next run uses: model, effort and mode. Beside it, as a task's run has, a
// panel (shown or hidden from the head) with the session's properties and what its runs used.

import { Button } from "@astryxdesign/core/Button";
import type { AgentMode, MessagesSnapshot, WorkSessionView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useConnection } from "../connection";
import { useEmployees, useMessages, useWorkSessions } from "../data";
import { useAction, useKeyContext } from "../keys/provider";
import { Chip } from "../ui/chip";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { EFFORTS } from "./dialog-logic";
import { useModels } from "./dialogs";
import { EmployeeChat } from "./employee-chat";
import { MODE_LABELS, MODES } from "./employee-logic";
import { lastRunOf, usageTotals } from "./run-logic";
import {
  FilesWritten,
  type PropertiesDock,
  PropertiesPanel,
  PropertiesToggle,
  RunGuidelines,
  SideProperties,
  SideRow,
  usePropertiesDock,
} from "./run-side";
import { chooserText, runChange, runChipLabel, sessionFolder, sessionStatus } from "./session-logic";
import { modelName } from "./settings-kit";
import { SubagentsAtWork } from "./subagents";
import { filesWritten } from "./tasks-logic";

/** Where the composer is in a session another board member started. */
export const OTHERS_SESSION = "Another board member started this session. Only they write in it; you can read it.";

const styles = stylex.create({
  body: { display: "flex", minWidth: 0, minHeight: 0, flexGrow: 1, flexBasis: 0 },
  chooser: {
    display: "flex",
    alignItems: "center",
    gap: 12,
    marginInline: "auto",
    width: "100%",
    maxWidth: 820,
    paddingInline: 24,
    paddingBottom: 16,
    color: "var(--color-text-secondary)",
  },
  chooserText: { flexGrow: 1, margin: 0 },
});

export function SessionScreen({ conversationId }: { conversationId: string }) {
  useKeyContext("TeamChat");
  const mine = useWorkSessions();
  const session = mine.data?.find((s) => s.conversation_id === conversationId) ?? null;
  // Opened from the History, a session another board member started: read, not written (the server
  // refuses it too). Only once the list is current, so a session just started never looks like that.
  const othersSession = mine.isSuccess && !mine.isFetching && session === null;
  const dock = usePropertiesDock();
  useSeenOnRead(conversationId);
  return (
    <div {...stylex.props(styles.body)}>
      <EmployeeChat
        conversationId={conversationId}
        frame={{
          ...(session ? { title: session.title || "Session" } : {}),
          empty: {
            title: "This session is starting.",
            description: "The first answer appears here. Talk with the employee as in a chat; there is no ticket.",
          },
          footer: (snapshot) => <RunChips snapshot={snapshot} />,
          ...(session?.dispatch ? { instead: <Chooser session={session} /> } : {}),
          ...(othersSession ? { instead: <p {...stylex.props(styles.chooser)}>{OTHERS_SESSION}</p> } : {}),
          actions: <PropertiesToggle dock={dock} label="Session properties" />,
          aside: <SessionDetails dock={dock} conversationId={conversationId} session={session} />,
        }}
      />
    </div>
  );
}

/**
 * Reading the session marks it seen on the server; the session list, read alongside, may still say it
 * finished unseen (the orange dot). So after each read of its messages, the list is read once more.
 */
function useSeenOnRead(conversationId: string) {
  const client = useQueryClient();
  const readAt = useMessages(conversationId).dataUpdatedAt;
  useEffect(() => {
    if (readAt > 0) void client.invalidateQueries({ queryKey: ["messages", "work_sessions"] });
  }, [readAt, client]);
}

/** The session's properties panel: what a task's run shows, for this session. */
function SessionDetails({
  dock,
  conversationId,
  session,
}: {
  dock: PropertiesDock;
  conversationId: string;
  session: WorkSessionView | null;
}) {
  const feed = useMessages(conversationId);
  const list = useMemo(() => feed.data?.messages ?? [], [feed.data]);
  const files = useMemo(() => filesWritten(list), [list]);
  const usage = useMemo(() => usageTotals(list), [list]);
  const run = feed.data?.run;
  const mode = feed.data?.agent_mode;
  const running = session?.running ?? false;
  return (
    <PropertiesPanel dock={dock} label="Session properties">
      <SideProperties usage={usage}>
        {session ? (
          <>
            <SideRow name="Employee">{session.dispatch ? "Not chosen yet" : session.employee_name}</SideRow>
            <SideRow name="Status">{sessionStatus(running, session.dispatch)}</SideRow>
            <SideRow name="Project">{session.project_name}</SideRow>
            {session.branch ? <SideRow name="Branch">{session.branch}</SideRow> : null}
            <SideRow name="Folder" title={session.worktree ?? undefined}>
              {sessionFolder(session)}
            </SideRow>
          </>
        ) : null}
        <SideRow name="Model">{runChipLabel(run?.model, modelName)}</SideRow>
        <SideRow name="Effort">{runChipLabel(run?.effort)}</SideRow>
        {mode ? <SideRow name="Mode">{MODE_LABELS[mode]}</SideRow> : null}
        {session ? (
          <SideRow name="Started">
            {new Date(session.created_at_ms).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" })}
          </SideRow>
        ) : null}
      </SideProperties>
      <SubagentsAtWork
        conversationId={conversationId}
        live={running}
        {...(session ? { askedBy: session.employee_name } : {})}
      />
      <FilesWritten files={files} />
      <RunGuidelines runId={lastRunOf(list)} />
    </PropertiesPanel>
  );
}

/**
 * Where the composer is while nobody works in the session yet (`session_dispatch`): the dispatcher
 * chooses, or chose nobody; the board member can choose who takes it.
 */
function Chooser({ session }: { session: WorkSessionView }) {
  const connection = useConnection();
  const client = useQueryClient();
  const employees = useEmployees();
  const active = (employees.data ?? []).filter((e) => e.status === "active");
  const choose = async (employeeId: string) => {
    try {
      await connection.write(
        "work_session.assign",
        { conversation_id: session.conversation_id, employee_id: employeeId },
        { retry: false },
      );
      await client.invalidateQueries({ queryKey: ["messages"] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  return (
    <div {...stylex.props(styles.chooser)}>
      <p {...stylex.props(styles.chooserText)}>{chooserText(session)}</p>
      <Menu
        label="Who takes this session"
        options={active.map((e) => ({ key: e.id, label: e.name, description: e.title ?? e.role }))}
        onPick={(id) => void choose(id)}
        trigger={
          <Button label="Choose who takes it" variant={session.dispatch === "nobody" ? "primary" : "secondary"} />
        }
      />
    </div>
  );
}

/** Model, effort and mode of the next run (a session's or a task's); a run under way keeps what it started with. */
export function RunChips({ snapshot }: { snapshot: MessagesSnapshot }) {
  const connection = useConnection();
  const client = useQueryClient();
  const models = useModels();
  const id = snapshot.conversation_id;
  const { run, agent_mode: mode } = snapshot;
  const write = async (call: Promise<unknown>) => {
    try {
      await call;
      await client.invalidateQueries({ queryKey: ["messages", id] });
    } catch (e) {
      toast(e instanceof Error ? e.message : "This did not work.");
    }
  };
  const modelOf = (key: string) => models.find((m) => m.key === key)?.label ?? key;
  const [modelOpen, setModelOpen] = useState(false);
  useAction("CycleModel", () => setModelOpen(true));
  return (
    <>
      <Menu
        label="Model"
        open={modelOpen}
        onOpenChange={setModelOpen}
        value={run.model ?? "default"}
        options={models.map((m) => (m.key === "default" ? { ...m, label: "Employee's default" } : m))}
        onPick={(model) =>
          void write(connection.write("conversation.set_run", runChange(id, run, { model }), { retry: false }))
        }
        trigger={
          <Chip name="Model" quiet>
            {runChipLabel(run.model, modelOf)}
          </Chip>
        }
      />
      <Menu
        label="Effort"
        value={run.effort ?? "default"}
        options={EFFORTS.map((e) => ({ key: e, label: e === "default" ? "Employee's default" : e }))}
        onPick={(effort) =>
          void write(connection.write("conversation.set_run", runChange(id, run, { effort }), { retry: false }))
        }
        trigger={
          <Chip name="Effort" quiet>
            {runChipLabel(run.effort)}
          </Chip>
        }
      />
      <Menu
        label="Mode"
        value={mode}
        options={MODES.map((m) => ({ key: m, label: MODE_LABELS[m] }))}
        onPick={(next: AgentMode) =>
          void write(
            connection.write("conversation.set_agent_mode", { conversation_id: id, mode: next }, { retry: false }),
          )
        }
        trigger={
          <Chip name="Mode" quiet>
            {MODE_LABELS[mode]}
          </Chip>
        }
      />
    </>
  );
}
