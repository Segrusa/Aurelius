// A work session (server feature `work_sessions`): one employee in one project's folder, started from
// New session without a ticket. It is the employee chat with its own head: the session's title and where
// it works. Later messages go in with `chat.send`, as in a chat. Below the words, as in Claude's own
// sessions, it chooses what the next run uses: model, effort and mode.

import type { AgentMode, MessagesSnapshot } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { useConnection } from "../connection";
import { useWorkSessions } from "../data";
import { useKeyContext } from "../keys/provider";
import { Chip } from "../ui/chip";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { EFFORTS } from "./dialog-logic";
import { useModels } from "./dialogs";
import { EmployeeChat } from "./employee-chat";
import { MODE_LABELS, MODES } from "./employee-logic";
import { runChange, runChipLabel, sessionPlace } from "./session-logic";

export function SessionScreen({ conversationId }: { conversationId: string }) {
  useKeyContext("TeamChat");
  const session = useWorkSessions().data?.find((s) => s.conversation_id === conversationId) ?? null;
  return (
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
