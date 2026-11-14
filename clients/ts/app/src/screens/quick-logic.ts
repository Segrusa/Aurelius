// Quick tasks (feature `quick_task`, ticket #53), pure: a kind of ticket that starts at once in Doing with its
// employee, works on its own branch and is merged into the project's main when a board member approves it.

import type { MethodParams } from "@aurelius/protocol";

/** The server feature; without it nothing of quick tasks shows (ST-07 AC3). */
export const QUICK_FEATURE = "quick_task";

/** What a quick task does, said where it is made. */
export const QUICK_NOTE = "Starts at once in Doing, on its own branch. In Review, Approve and merge puts it in main.";

/** Where the employee last chosen for a quick task is remembered, like the other desktop prefs. */
export const LAST_QUICK = "last_quick_to";

export function quickEnabled(features: readonly string[]): boolean {
  return features.includes(QUICK_FEATURE);
}

/** A view says "quick" only for a quick task; a normal ticket has no kind. */
export function isQuick(t: { kind?: string | null }): boolean {
  return t.kind === "quick";
}

/** The tag on a card and a row, or none. */
export function quickTag(t: { kind?: string | null }, enabled: boolean): string | null {
  return enabled && isQuick(t) ? "Quick" : null;
}

/** The board's and My tickets' choice: every ticket, or only quick tasks. */
export type KindFilter = "all" | "quick";

export function kindMatches(filter: KindFilter, t: { kind?: string | null }): boolean {
  return filter === "all" || isQuick(t);
}

/** What the filter shows on a server with the feature; without it there is no choice. */
export function kindFilterOptions(enabled: boolean): { key: KindFilter; label: string }[] {
  return enabled
    ? [
        { key: "all", label: "All tickets" },
        { key: "quick", label: "Quick tasks" },
      ]
    : [];
}

/** ST-05 AC1: Done of a quick task merges it. */
export function acceptLabel(t: { kind?: string | null }): string {
  return isQuick(t) ? "Approve and merge" : "Mark done";
}

export interface Confirmation {
  title: string;
  body: string;
  confirm: string;
}

/** Done from a drag or the Move menu. */
export function doneConfirmation(t: { number: number; kind?: string | null }): Confirmation {
  return isQuick(t)
    ? {
        title: `Approve and merge #${t.number}?`,
        body: "Its branch is merged into the project's main; the card goes to Done when the merge worked. Nothing is pushed.",
        confirm: "Approve and merge",
      }
    : {
        title: `Mark #${t.number} as Done?`,
        body: "This closes the ticket. Nothing is merged, pushed or deployed.",
        confirm: "Mark done",
      };
}

/** BR4: back to Backlog of a quick task removes its branch and worktree, so it is always confirmed. */
export function cancelNeedsConfirm(t: { kind?: string | null }, otherwise: boolean): boolean {
  return isQuick(t) || otherwise;
}

export function cancelConfirmation(t: { number: number; kind?: string | null }): Confirmation {
  return isQuick(t)
    ? {
        title: `Cancel quick task #${t.number}?`,
        body: "Its branch and worktree are removed; work that was not merged is lost.",
        confirm: "Cancel quick task",
      }
    : { title: `Back to Backlog: #${t.number}?`, body: "This stops the work.", confirm: "Back to Backlog" };
}

const SHORT_COMMIT = 7;

export function shortCommit(sha: string): string {
  return sha.slice(0, SHORT_COMMIT);
}

/** The Done line of the ticket details; `when` is the formatted time it closed, if known. */
export function doneLine(t: { kind?: string | null; merge_commit?: string | null }, when: string | null): string {
  const done = `Done${when ? ` · ${when}` : ""}.`;
  if (!isQuick(t)) return `${done} Nothing was merged, pushed or deployed by closing it.`;
  return t.merge_commit
    ? `${done} Merged into main as ${shortCommit(t.merge_commit)}. Nothing was pushed.`
    : `${done} The branch had no commits, so nothing was merged.`;
}

/** The last employee given a quick task, while still active; else none, so the board member chooses. */
export function quickAssignee(employees: { id: string; status: string }[], last: string | null): string | null {
  return last !== null && employees.some((e) => e.id === last && e.status === "active") ? last : null;
}

type CreateParams = MethodParams["ticket.create"];

/** What makes a quick task out of a create; the server starts it, in Auto, so no `start` and no mode go along. */
export interface QuickForm {
  projectId: string;
  title: string;
  description: string;
  employeeId: string;
  files: string[];
  model: string | null;
  effort?: string | null;
}

/** Why a quick task cannot be created yet, or null. The server checks again and says why in its own words. */
export function quickProblem(form: { description: string; employeeId: string | null }): string | null {
  if (form.description.trim() === "") return "Describe what needs doing.";
  return form.employeeId ? null : "Choose who does it.";
}

/** `ticket.create` of a quick task. `kind` only ever goes along here: an older server refuses a field it does not know. */
export function quickCreateParams(form: QuickForm, confirmSecret: boolean): CreateParams {
  return {
    project_id: form.projectId,
    kind: "quick",
    title: form.title.trim(),
    description: form.description.trim(),
    owner_employee_id: form.employeeId,
    run: { ...(form.model ? { model: form.model } : {}), ...(form.effort ? { effort: form.effort } : {}) },
    ...(form.files.length > 0 ? { files: form.files } : {}),
    confirm_secret: confirmSecret,
  };
}
