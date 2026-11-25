// Side questions (feature `side_question`), like Claude Code's /btw: the questions asked in the
// open popover and their answers. Nothing is kept: closing the popover forgets them.

import type { SideAnswer } from "@aurelius/protocol";

export type SideStatus = "asking" | "answering" | "done" | "failed";

/** One question in the popover and its answer so far. */
export interface SideAsk {
  /** This client's own key, known before the server answers. */
  key: number;
  /** The server's id, once `conversation.side_ask` answered. */
  sideId: string | null;
  question: string;
  text: string;
  status: SideStatus;
  error: string | null;
}

/** A new question, waiting for its id. */
export function asked(list: SideAsk[], key: number, question: string): SideAsk[] {
  return [...list, { key, sideId: null, question, text: "", status: "asking", error: null }];
}

/** The server took the question; `early` is an answer that came before its id did. */
export function started(list: SideAsk[], key: number, sideId: string, early?: SideAnswer): SideAsk[] {
  const next = list.map((a) => (a.key === key ? { ...a, sideId } : a));
  return early ? answered(next, early) : next;
}

/** The server refused the question (too long, no login, offline). */
export function refused(list: SideAsk[], key: number, error: string): SideAsk[] {
  return list.map((a) => (a.key === key ? { ...a, status: "failed", error } : a));
}

/** An answer so far (each holds the whole text); an answer to no question here is left. */
export function answered(list: SideAsk[], answer: SideAnswer): SideAsk[] {
  return list.map((a) => {
    if (a.sideId !== answer.side_id) return a;
    const status: SideStatus = answer.error ? "failed" : answer.done ? "done" : "answering";
    // A failure keeps what was written before it.
    const text = answer.text || a.text;
    return { ...a, text, status, error: answer.error ?? null };
  });
}

/** Whether this answer belongs to no question here yet (it came before the id). */
export function unclaimed(list: SideAsk[], answer: SideAnswer): boolean {
  return !list.some((a) => a.sideId === answer.side_id);
}

/** The ids still being answered: stopped when the popover closes. */
export function stillAnswering(list: SideAsk[]): string[] {
  return list.flatMap((a) =>
    a.sideId !== null && (a.status === "asking" || a.status === "answering") ? [a.sideId] : [],
  );
}

/** Whether a new question may go: one at a time, and not blank. */
export function canAsk(list: SideAsk[], draft: string): boolean {
  return draft.trim().length > 0 && !list.some((a) => a.status === "asking" || a.status === "answering");
}
