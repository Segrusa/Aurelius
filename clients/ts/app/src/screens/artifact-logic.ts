// Design artifacts: an HTML file a run kept with its message, opened as a live design instead of
// a download. A design's pages are the kept HTML files of a conversation, each at its newest.

import type { ActivityView, FileRef, MessageView } from "@aurelius/protocol";

/** A kept file the viewer opens; every other file keeps its old behaviour. */
export function isArtifact(file: Pick<FileRef, "media_type">): boolean {
  return file.media_type.split(";")[0]?.trim().toLowerCase() === "text/html";
}

/** One page of a design: the newest kept HTML file with this name in the conversation. */
export interface DesignPage {
  name: string;
  /** What the page is shown as (`designTitle`). */
  title: string;
  file: FileRef;
  messageId: string;
  atMs: number;
}

/**
 * The pages of the designs in `messages`: one per file name, the newest kept HTML file of each
 * (a deleted message no longer offers its file), in the order the pages first came. `opened`, a
 * file whose message is not in the list (yet), is a page of its own.
 */
export function designPages(messages: readonly MessageView[], opened?: FileRef, atMs = 0): DesignPage[] {
  const pages = new Map<string, DesignPage>();
  for (const m of messages.filter((x) => !x.deleted_at_ms).toSorted((a, b) => a.ordinal - b.ordinal))
    for (const file of (m.files ?? []).filter(isArtifact)) pages.set(file.name, pageOf(file, m.id, m.created_at_ms));
  if (opened && !pages.has(opened.name)) pages.set(opened.name, pageOf(opened, "", atMs));
  return [...pages.values()];
}

/** A kept file as a page of its design. */
export function pageOf(file: FileRef, messageId: string, atMs: number): DesignPage {
  return { name: file.name, title: designTitle(file), file, messageId, atMs };
}

/**
 * The pages of the designs over several conversations (a ticket's discussion and its tasks): one
 * per file name, the newest by time, in the order the pages first came. A list not read yet is
 * left out.
 */
export function designPagesAcross(lists: readonly (readonly MessageView[] | undefined)[]): DesignPage[] {
  const pages = new Map<string, DesignPage>();
  for (const list of lists)
    for (const page of designPages(list ?? [])) {
      const had = pages.get(page.name);
      if (!had || page.atMs >= had.atMs) pages.set(page.name, page);
    }
  return [...pages.values()];
}

/**
 * What a design page is called: its own `<title>` (read by the server when it kept the file),
 * else its file name made readable: "workspace-settings.html" is "Workspace settings".
 */
export function designTitle(file: { name: string; title?: string | null }): string {
  const own = file.title?.trim();
  if (own) return own;
  const words = file.name
    .replace(/\.html?$/i, "")
    .replace(/[-_]+/g, " ")
    .trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : file.name;
}

/** The `<title>` in an HTML page's head, decoded and on one line; null without one. */
export function htmlTitle(html: string): string | null {
  const head = html.slice(0, Math.max(0, html.search(/<\/head/i)) || html.length);
  const found = /<title(?:\s[^>]*)?>([\s\S]*?)<\/title/i.exec(head);
  if (!found?.[1]) return null;
  const text = found[1]
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#0?39;/g, "'")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ")
    .trim();
  return text || null;
}

export const VIEWPORTS = [
  { value: "desktop", label: "Desktop", width: 1280 },
  { value: "tablet", label: "Tablet", width: 768 },
  { value: "mobile", label: "Mobile", width: 390 },
] as const;

export type Viewport = (typeof VIEWPORTS)[number]["value"];

export function viewportWidth(viewport: Viewport): number {
  return VIEWPORTS.find((v) => v.value === viewport)?.width ?? 1280;
}

interface Edges {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/**
 * What of the page can lie over the artifact view: menus and lists, toasts (Astryx gives each a
 * `data-toast-id`) and dialogs. The view lies above the whole page, so it would hide them, and the
 * artifact could draw a lookalike in their place.
 */
export const OVERLAY_SELECTOR =
  '[role="listbox"], [role="menu"], [data-toast-id], dialog[open], [role="dialog"], [role="alertdialog"]';
/** Which overlays are dialogs: only those that opened after the view count (see overlaysOver). */
export const DIALOG_SELECTOR = 'dialog, [role="dialog"], [role="alertdialog"]';

/** One element that matched OVERLAY_SELECTOR, as the viewer measured it. */
export interface Overlay {
  rect: Edges & { width: number; height: number };
  visible: boolean;
  isDialog: boolean;
  /** Open before the view was shown: the viewer's own dialog, or one it was opened from. */
  openBefore: boolean;
  /** Holds the stage: the viewer's own dialog, which lies under the view. */
  holdsStage: boolean;
}

/**
 * The rectangles that make the view step aside: every visible menu, list and toast, and every
 * visible dialog that opened after the view (a confirm, another dialog). Closed menus and toasts
 * stay in the document, hidden, and do not count.
 */
export function overlaysOver(overlays: readonly Overlay[]): Edges[] {
  return overlays
    .filter((o) => o.visible && o.rect.width > 0 && o.rect.height > 0)
    .filter((o) => !(o.isDialog && (o.openBefore || o.holdsStage)))
    .map((o) => o.rect);
}

/**
 * Where the desktop lays the artifact view: the frame box (the chosen width, centred in the stage)
 * cut to the stage and the window, in CSS pixels. Null hides the view: when nothing of it is in
 * sight, or an overlay of the page lies over it, which the view would cover (it lies above the
 * page). `menus` are the rectangles of those overlays (overlaysOver).
 */
export function stageBounds(
  frame: Edges,
  stage: Edges,
  window: { width: number; height: number },
  menus: readonly Edges[] = [],
): { x: number; y: number; width: number; height: number } | null {
  const left = Math.max(frame.left, stage.left, 0);
  const top = Math.max(frame.top, stage.top, 0);
  const right = Math.min(frame.right, stage.right, window.width);
  const bottom = Math.min(frame.bottom, stage.bottom, window.height);
  if (right <= left || bottom <= top) return null;
  const covered = menus.some((m) => m.left < right && m.right > left && m.top < bottom && m.bottom > top);
  return covered ? null : { x: left, y: top, width: right - left, height: bottom - top };
}

/** HTML that arrived as base64 from `file.get`, read as UTF-8 like a browser would read the file. */
export function htmlOf(base64: string): string {
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
  return new TextDecoder("utf-8").decode(bytes);
}

// A design written live: an HTML file an answer writes with `Write`, shown while it is written
// (the draft the server streams) and after, also when the answer does not keep the file.

/** A file the app shows as a design, by its path. */
export function isDesignPath(path: string): boolean {
  return /\.html?$/i.test(path);
}

/** The last part of a path, Windows or Unix. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** One design an answer writes: its newest `Write`, and the edits on it after. */
export interface WrittenDesign {
  path: string;
  name: string;
  /** The `Write` call. */
  callId: string;
  /** Still being written: the draft, or the call is not done. */
  writing: boolean;
  /** The content so far while the call is not whole yet; null: fetch the call. */
  draft: string | null;
  /** The answer's later `Edit`/`MultiEdit` calls on the same file that worked, in order. */
  edits: string[];
}

interface DraftLike {
  call_id: string;
  path: string;
  content: string;
}

const pathOf = (input: unknown): string | null => {
  const path = (input as { file_path?: unknown } | null)?.file_path;
  return typeof path === "string" ? path : null;
};

/**
 * The designs of an answer, in the order it first wrote them: from its own calls (a subagent's
 * work is its own) and the draft of the call it is writing now.
 */
export function designsOf(activity: readonly ActivityView[], draft: DraftLike | null | undefined): WrittenDesign[] {
  const designs = new Map<string, WrittenDesign>();
  for (const call of activity) {
    if (call.parent_call_id) continue;
    const path = pathOf(call.input);
    if (path === null || !isDesignPath(path)) continue;
    if (call.tool === "Write") {
      designs.set(path, {
        path,
        name: baseName(path),
        callId: call.call_id,
        writing: call.ok == null,
        draft: null,
        edits: [],
      });
    } else if ((call.tool === "Edit" || call.tool === "MultiEdit") && call.ok === true) {
      designs.get(path)?.edits.push(call.call_id);
    }
  }
  if (draft && isDesignPath(draft.path) && !activity.some((c) => c.call_id === draft.call_id)) {
    designs.set(draft.path, {
      path: draft.path,
      name: baseName(draft.path),
      callId: draft.call_id,
      writing: true,
      draft: draft.content,
      edits: [],
    });
  }
  return [...designs.values()];
}

/**
 * The design to open by itself, as Claude Desktop opens an artifact it starts writing: the first
 * one that streams a draft now and was not opened by itself before (`seen` holds `Write` call
 * ids). Only a draft counts, so an older answer never opens anything when a chat loads, and a
 * design the user closed stays closed until the answer writes it anew.
 */
export function designToOpen(designs: readonly WrittenDesign[], seen: ReadonlySet<string>): WrittenDesign | null {
  return designs.find((d) => d.draft !== null && !seen.has(d.callId)) ?? null;
}

interface EditStep {
  old_string?: unknown;
  new_string?: unknown;
  replace_all?: unknown;
}

function applyOne(html: string, step: EditStep): string | null {
  if (typeof step.old_string !== "string" || typeof step.new_string !== "string" || step.old_string === "") return null;
  if (!html.includes(step.old_string)) return null;
  return step.replace_all === true
    ? html.split(step.old_string).join(step.new_string)
    : html.replace(step.old_string, () => step.new_string as string);
}

/**
 * The file after an `Edit` or `MultiEdit` call's whole input, as the tool does it; null when it
 * does not fit (the file changed in another way meanwhile), so the viewer keeps what it had.
 */
export function applyEdit(html: string, input: unknown): string | null {
  const edits = (input as { edits?: unknown } | null)?.edits;
  const steps: EditStep[] = Array.isArray(edits) ? (edits as EditStep[]) : [input as EditStep];
  let out: string | null = html;
  for (const step of steps) {
    out = out === null ? null : applyOne(out, step ?? {});
  }
  return out;
}

/** The design's content from its whole `Write` input. */
export function writtenContent(input: unknown): string | null {
  const content = (input as { content?: unknown } | null)?.content;
  return typeof content === "string" ? content : null;
}

/** The handlers of a resizable region's drag (the design system's `ResizableProps`). */
interface DragHandlers {
  _onResizeStart: () => void;
  _onResizeEnd: () => void;
  _onResizeCancel?: () => void;
}

/**
 * A resizable region's props that also tell `onDrag` when a drag starts (true) and ends or is
 * cancelled (false), so the artifact's view can step aside while its panel is dragged.
 */
export function withDrag<P extends DragHandlers>(props: P, onDrag: (dragging: boolean) => void): P {
  return {
    ...props,
    _onResizeStart: () => {
      onDrag(true);
      props._onResizeStart();
    },
    _onResizeEnd: () => {
      onDrag(false);
      props._onResizeEnd();
    },
    _onResizeCancel: () => {
      onDrag(false);
      props._onResizeCancel?.();
    },
  };
}

/**
 * What the viewer of a design being written does with newer HTML (`html`) while it shows `served`:
 * load the first draft in a view ("serve"), grow it in that same view ("update": no new view, so
 * no flash and the scroll stays), or nothing. A finished design that holds scripts is loaded once
 * more, because scripts put in by an update do not run. The first view waits until the draft has
 * left its head (draftHasContent): before that it is a blank white page for as long as the styles
 * take to write.
 */
export function liveStep(
  served: string | undefined,
  html: string | undefined,
  writing: boolean,
): "serve" | "update" | "none" {
  if (html === undefined || html === served) return "none";
  if (served === undefined) return writing && !draftHasContent(html) ? "none" : "serve";
  if (!writing && /<script[\s>]/i.test(html)) return "serve";
  return "update";
}

/** A draft's size for the board member, e.g. "12 KB". */
export const sizeText = (chars: number) => (chars >= 1024 ? `${Math.round(chars / 1024)} KB` : `${chars} B`);

/** A draft without what never shows: comments, styles, scripts, the title and the head's tags. */
function draftBody(html: string): string {
  return (
    html
      // Unclosed ones run to the end of the draft: they are still being written.
      .replace(/<!--[\s\S]*?(?:-->|$)/g, "")
      .replace(/<(style|script|title|template|noscript)\b[\s\S]*?(?:<\/\1\s*>|$)/gi, "")
      .replace(/<!doctype[^>]*>?/gi, "")
      .replace(/<\/?(?:html|head|meta|link|base)\b[^>]*>?/gi, "")
      // A tag cut off by the end of the draft: what it is shows in the next one.
      .replace(/<[^>]*$/, "")
  );
}

/**
 * Whether a draft holds anything of the page itself yet: its body, an element or text outside the
 * head. A design writes its head first (often many KB of styles), which shows as a blank page.
 */
export function draftHasContent(html: string): boolean {
  return draftBody(html).trim() !== "";
}

/** What a design being written is at, before its page shows: shown to the board member while they wait. */
export type DraftPhase = "starting" | "head" | "styles" | "scripts" | "page";

export function draftPhase(html: string | null): DraftPhase {
  if (html === null || html.trim() === "") return "starting";
  if (draftHasContent(html)) return "page";
  const open = (tag: string) => {
    const opened = html.toLowerCase().lastIndexOf(`<${tag}`);
    return opened >= 0 && !html.slice(opened).toLowerCase().includes(`</${tag}`);
  };
  if (open("style")) return "styles";
  if (open("script")) return "scripts";
  return "head";
}

/** The page a design session opens with: the newest kept HTML file of the conversation. */
export function newestDesignFile(messages: readonly MessageView[]): FileRef | null {
  let newest: { ordinal: number; file: FileRef } | null = null;
  for (const m of messages) {
    if (m.deleted_at_ms || (newest && m.ordinal < newest.ordinal)) continue;
    const pages = (m.files ?? []).filter(isArtifact);
    const last = pages[pages.length - 1];
    if (last) newest = { ordinal: m.ordinal, file: last };
  }
  return newest?.file ?? null;
}

/** The kept HTML file `id` of the conversation, to open it as a design; null when it is not there. */
export function designFile(messages: readonly MessageView[], id: string): FileRef | null {
  for (const m of messages) {
    if (m.deleted_at_ms) continue;
    const found = (m.files ?? []).find((f) => f.id === id && isArtifact(f));
    if (found) return found;
  }
  return null;
}

/**
 * The file a finished answer kept of the design it wrote at `path` (same file name), or null
 * while it is still being written or when the answer did not keep it.
 */
export function keptDesign(message: Pick<MessageView, "status" | "files">, path: string): FileRef | null {
  if (message.status !== "complete") return null;
  const name = baseName(path);
  return (message.files ?? []).find((f) => isArtifact(f) && f.name === name) ?? null;
}

/**
 * What a ticket made from a page of a design starts with: the page's title (`designTitle`) in the
 * ticket's title, the path the employee named in the description. The page goes along as a file.
 */
export function designTicket(name: string, path: string): { title: string; description: string } {
  return {
    title: `Build ${name}`,
    description: `Build this page as designed in ${path}. The design is attached: follow its layout, colours and texts.`,
  };
}
