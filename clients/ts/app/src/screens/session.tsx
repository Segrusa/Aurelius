// A work session (server feature `work_sessions`): one employee in one project's folder, started from
// New session without a ticket. It is the employee chat with its own head: the session's title and where
// it works. Later messages go in with `chat.send`, as in a chat. Below the words, as in Claude's own
// sessions, it chooses what the next run uses: model, effort and mode. On the right, as a task's run has,
// a panel with the session's properties and what its runs used: files, guidelines and tokens.

import type { AgentMode, MessagesSnapshot, WorkSessionView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { useConnection } from "../connection";
import { useMessages, useWorkSessions } from "../data";
import { useKeyContext } from "../keys/provider";
import { Chip } from "../ui/chip";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { EFFORTS } from "./dialog-logic";
import { useModels } from "./dialogs";
import { EmployeeChat } from "./employee-chat";
import { MODE_LABELS, MODES } from "./employee-logic";
import { lastRunOf, usageTotals } from "./run-logic";
import { FilesWritten, RunAside, RunGuidelines, SideProperties, SideRow, UsageTotalsLine } from "./run-side";
import { SubagentsAtWork } from "./run-tools";
import { runChange, runChipLabel, sessionFolder, sessionPlace, sessionStatus } from "./session-logic";
import { modelName } from "./settings-kit";
import { filesWritten } from "./tasks-logic";

const styles = stylex.create({
  body: { display: "flex", minWidth: 0, minHeight: 0, flexGrow: 1, flexBasis: 0 },
});

export function SessionScreen({ conversationId }: { conversationId: string }) {
  useKeyContext("TeamChat");
  const session = useWorkSessions().data?.find((s) => s.conversation_id === conversationId) ?? null;
  return (
    <div {...stylex.props(styles.body)}>
      <EmployeeChat
        conversationId={conversationId}
        frame={{
          ...(session ? { title: session.title || "Session" } : {}),
          ...(session ? { subtitle: sessionPlace(session) } : {}),
          empty: {
            title: "This session is starting.",
            description: "The first answer appears here. Talk with the employee as in a chat; there is no ticket.",
          },
          footer: (snapshot) => <RunChips snapshot={snapshot} />,
        }}
      />
      <SessionDetails conversationId={conversationId} session={session} />
    </div>
  );
}

/** The session's side panel: what a task's run shows, for this session. */
function SessionDetails({ conversationId, session }: { conversationId: string; session: WorkSessionView | null }) {
  const feed = useMessages(conversationId);
  const list = useMemo(() => feed.data?.messages ?? [], [feed.data]);
  const files = useMemo(() => filesWritten(list), [list]);
  const usage = useMemo(() => usageTotals(list), [list]);
  const run = feed.data?.run;
  const mode = feed.data?.agent_mode;
  const running = session?.running ?? false;
  return (
    <RunAside label="Session properties">
      <SideProperties>
        {session ? (
          <>
            <SideRow name="Employee">{session.employee_name}</SideRow>
            <SideRow name="Status">{sessionStatus(running)}</SideRow>
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
      <SubagentsAtWork conversationId={conversationId} live={running} />
      <FilesWritten files={files} />
      <RunGuidelines runId={lastRunOf(list)} />
      <UsageTotalsLine usage={usage} />
    </RunAside>
  );
}

/** Model, effort and mode of the session's next run; a run under way keeps what it started with. */
function RunChips({ snapshot }: { snapshot: MessagesSnapshot }) {
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
  return (
    <>
      <Menu
        label="Model"
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
