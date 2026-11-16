// An answer as Claude shows one: the text it wrote, with the steps it took in between at the place they
// happened ("Ran 5 commands, read employee-chat.tsx"). Each step's `offset` is the UTF-8 byte length of the
// answer text when the step started (crates/daemon/src/dispatcher/chat.rs).

import type { ActivityView } from "@aurelius/protocol";

export type AnswerPart = { kind: "text"; text: string } | { kind: "steps"; steps: ActivityView[]; key: string };

/** The UTF-16 index in `text` that sits `bytes` UTF-8 bytes in; past the end gives the length. */
function indexAtByte(text: string, bytes: number): number {
  let seen = 0;
  let i = 0;
  while (i < text.length && seen < bytes) {
    const code = text.codePointAt(i) ?? 0;
    seen += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    i += code > 0xffff ? 2 : 1;
  }
  return i;
}

/** The text cut at each step, with the steps that ran between two pieces of text grouped together. */
export function answerParts(text: string, activity: ActivityView[]): AnswerPart[] {
  const parts: AnswerPart[] = [];
  const pushText = (piece: string) => {
    const trimmed = piece.trim();
    if (trimmed !== "") parts.push({ kind: "text", text: trimmed });
  };
  let at = 0;
  for (const step of activity) {
    // Steps keep the order they ran in: an offset that goes back (a rewritten text) stays where it was.
    const cut = Math.max(at, indexAtByte(text, step.offset));
    pushText(text.slice(at, cut));
    at = cut;
    const last = parts[parts.length - 1];
    if (last?.kind === "steps") last.steps.push(step);
    else parts.push({ kind: "steps", steps: [step], key: step.call_id });
  }
  pushText(text.slice(at));
  return parts;
}

type Kind = "command" | "read" | "edit" | "write" | "search" | "web" | "other";

const KINDS: Record<string, Kind> = {
  Bash: "command",
  BashOutput: "command",
  Read: "read",
  NotebookRead: "read",
  Edit: "edit",
  MultiEdit: "edit",
  NotebookEdit: "edit",
  Write: "write",
  Grep: "search",
  Glob: "search",
  LS: "search",
  WebSearch: "web",
  WebFetch: "web",
};

const kindOf = (tool: string): Kind => KINDS[tool.replace(/^claude\./, "")] ?? "other";

const fileName = (step: ActivityView): string | null => {
  const path = (step.input as { file_path?: unknown; notebook_path?: unknown } | null)?.file_path;
  return typeof path === "string" && path !== "" ? (path.split(/[\\/]/).pop() ?? path) : null;
};

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** One kind of step as a phrase, in the past or (while one of them runs) the present tense. */
function phrase(kind: Kind, steps: ActivityView[], running: boolean): string {
  const files = [...new Set(steps.map(fileName).filter((f): f is string => f !== null))];
  const what = files.length === 1 ? (files[0] ?? "") : plural(files.length || steps.length, "file", "files");
  switch (kind) {
    case "command":
      return `${running ? "running" : "ran"} ${plural(steps.length, "command", "commands")}`;
    case "read":
      return `${running ? "reading" : "read"} ${what}`;
    case "edit":
      return `${running ? "editing" : "edited"} ${what}`;
    case "write":
      return `${running ? "writing" : "wrote"} ${what}`;
    case "search":
      return `${running ? "searching" : "searched"} ${plural(steps.length, "time", "times")}`;
    case "web":
      return `${running ? "searching" : "searched"} the web`;
    default:
      return `${running ? "using" : "used"} ${plural(steps.length, "tool", "tools")}`;
  }
}

/** "Ran 2 commands, read employee-chat.tsx", in the order the kinds first came up. */
export function stepsLabel(steps: ActivityView[]): string {
  const byKind = new Map<Kind, ActivityView[]>();
  for (const step of steps) {
    const kind = kindOf(step.tool);
    byKind.set(kind, [...(byKind.get(kind) ?? []), step]);
  }
  const runningKind = [...steps].reverse().find((s) => s.ok == null);
  const running = runningKind ? kindOf(runningKind.tool) : null;
  const text = [...byKind].map(([kind, list]) => phrase(kind, list, kind === running)).join(", ");
  const failed = steps.filter((s) => s.ok === false).length;
  const label = text.charAt(0).toUpperCase() + text.slice(1);
  return `${label}${running ? "…" : ""}${failed > 0 ? ` · ${failed} failed` : ""}`;
}
