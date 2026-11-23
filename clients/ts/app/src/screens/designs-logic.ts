// The Designs page: every design of the workspace (`design.list`), where it was made, and its
// pages as the viewer takes them.

import type { DesignView } from "@aurelius/protocol";
import type { DesignPage } from "./artifact-logic";

/** Where a design was made, and the screen that opens that conversation (none: no screen). */
export function designPlace(design: DesignView): { label: string; to: string | null } {
  switch (design.kind) {
    case "session":
      return { label: "Session", to: `/session/${design.conversation_id}` };
    case "ticket":
      return design.ticket_number
        ? { label: `Ticket #${design.ticket_number}`, to: `/ticket/${design.ticket_number}` }
        : { label: "Ticket", to: null };
    case "task":
      return { label: "Task", to: design.task_id ? `/tasks/${design.task_id}` : null };
    default:
      return { label: "Project chat", to: null };
  }
}

/** The design's pages as the viewer takes them. */
export function pagesOf(design: DesignView): DesignPage[] {
  return design.pages.map((p) => ({ name: p.file.name, file: p.file, messageId: p.message_id, atMs: p.at_ms }));
}
