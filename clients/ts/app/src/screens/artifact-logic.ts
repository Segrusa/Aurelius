// Design artifacts: an HTML file a run kept with its message, opened as a live design instead of
// a download. Its versions are the kept HTML files with the same name in the same conversation.

import type { FileRef, MessageView } from "@aurelius/protocol";

/** A kept file the viewer opens; every other file keeps its old behaviour. */
export function isArtifact(file: Pick<FileRef, "media_type">): boolean {
  return file.media_type.split(";")[0]?.trim().toLowerCase() === "text/html";
}

export interface ArtifactVersion {
  /** 1 for the first message that kept this name, counting up in message order. */
  version: number;
  file: FileRef;
  messageId: string;
  atMs: number;
}

/**
 * All versions of the artifact called `name`, newest first. A deleted message no longer offers
 * its file. Two files with the same name in one message are two versions, in the message's order.
 */
export function artifactVersions(messages: readonly MessageView[], name: string): ArtifactVersion[] {
  const kept = messages
    .filter((m) => !m.deleted_at_ms)
    .toSorted((a, b) => a.ordinal - b.ordinal)
    .flatMap((m) =>
      (m.files ?? [])
        .filter((f) => f.name === name && isArtifact(f))
        .map((file) => ({ file, messageId: m.id, atMs: m.created_at_ms })),
    );
  return kept.map((v, i) => ({ ...v, version: i + 1 })).reverse();
}

/**
 * The versions the viewer offers for a file that was opened: those of the conversation, or the
 * file alone when its message is not in the list (yet).
 */
export function versionsFor(messages: readonly MessageView[], file: FileRef, atMs: number): ArtifactVersion[] {
  const versions = artifactVersions(messages, file.name);
  return versions.some((v) => v.file.id === file.id) ? versions : [{ version: 1, file, messageId: "", atMs }];
}

/** "v2 of 3" for the card of a file; null when there is only one. */
export function versionOf(versions: readonly ArtifactVersion[], fileId: string): string | null {
  const own = versions.find((v) => v.file.id === fileId);
  return own && versions.length > 1 ? `v${own.version} of ${versions.length}` : null;
}

const sameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

/** "v3 · 14:05", with the date when it was not today. */
export function versionLabel(v: ArtifactVersion, nowMs: number): string {
  const at = new Date(v.atMs);
  const time = at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });
  const when = sameDay(at, new Date(nowMs))
    ? time
    : `${at.toLocaleDateString(undefined, { month: "short", day: "numeric" })}, ${time}`;
  return `v${v.version} · ${when}`;
}

/** The widths the viewer shows an artifact at; an export is made at the same width. */
export const VIEWPORTS = [
  { value: "desktop", label: "Desktop", width: 1280 },
  { value: "tablet", label: "Tablet", width: 768 },
  { value: "mobile", label: "Mobile", width: 390 },
] as const;

export type Viewport = (typeof VIEWPORTS)[number]["value"];

export function viewportWidth(viewport: Viewport): number {
  return VIEWPORTS.find((v) => v.value === viewport)?.width ?? 1280;
}

/** The iframe sandbox: scripts run, in an origin of their own; nothing else is allowed. */
export const ARTIFACT_SANDBOX = "allow-scripts";

/** HTML that arrived as base64 from `file.get`, read as UTF-8 like a browser would read the file. */
export function htmlOf(base64: string): string {
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
  return new TextDecoder("utf-8").decode(bytes);
}
