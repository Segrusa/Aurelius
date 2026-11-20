// What the Inbox lists, read once for its page and for the bell in every screen's head.

import { useMemo } from "react";
import { useLinkState } from "../connection";
import { useApprovals, useGuidelineProposals, useNotifications, useOpenTasks, useProposals, useTickets } from "../data";
import { useWorkspace } from "../workspace";
import { taskCounts } from "./board-logic";
import { buildInbox, type InboxTone } from "./inbox-logic";
import { useTicketNeeds } from "./tickets-kit";
import { guidelineProposalsIn, narrow } from "./workspace-logic";

/** The colour of a group's dot and a row's glyph. */
export const INBOX_TONES: Record<InboxTone, string> = {
  amber: "var(--color-text-warning)",
  violet: "var(--precision-violet)",
  blue: "var(--color-text-info)",
  green: "var(--color-text-accent)",
  muted: "var(--precision-text3)",
  red: "var(--color-text-error)",
};

/** The Inbox of this workspace, in its groups: what needs a decision, a review, and the updates. */
export function useInbox() {
  const workspace = useWorkspace().id;
  const approvals = useApprovals(workspace);
  const proposals = useProposals();
  const guidelineProposals = useGuidelineProposals();
  const tasks = useOpenTasks();
  const notifications = useNotifications(workspace);
  const { features } = useLinkState();
  const responsible = features.includes("ticket_responsible");
  const tickets = useTickets(responsible);
  const need = useTicketNeeds(tickets.data ?? []);
  const groups = useMemo(
    () =>
      buildInbox({
        approvals: approvals.data ?? [],
        // Who to hire is the company's, not a project's.
        proposals: workspace === null ? (proposals.data ?? []) : [],
        guidelineProposals: guidelineProposalsIn(guidelineProposals.data ?? [], workspace),
        // Freeze §5: a ticket's main task only when its ticket needs this board member.
        tasks: narrow(tasks.data ?? [], workspace).filter((t) =>
          taskCounts(
            t,
            responsible,
            tickets.data?.find((x) => x.id === t.ticket_id),
            need.mine,
          ),
        ),
        notifications: notifications.data ?? [],
      }),
    [
      workspace,
      approvals.data,
      proposals.data,
      guidelineProposals.data,
      tasks.data,
      notifications.data,
      responsible,
      tickets.data,
      need,
    ],
  );
  const loading = approvals.isPending || notifications.isPending || tasks.isPending;
  const error = approvals.error ?? notifications.error ?? tasks.error ?? null;
  return { groups, loading, error };
}
