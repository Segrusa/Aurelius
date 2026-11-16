// A work session (server feature `work_sessions`): one employee in one project's folder, started from
// New session without a ticket. It is the employee chat with its own head: the session's title and where
// it works. Later messages go in with `chat.send`, as in a chat.

import { useWorkSessions } from "../data";
import { useKeyContext } from "../keys/provider";
import { EmployeeChat } from "./employee-chat";
import { sessionPlace } from "./session-logic";

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
      }}
    />
  );
}
