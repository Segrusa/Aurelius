// The rules of what a prompt box carries along with its words: photos, files and folders.

/** The server feature that lets `chat.send` and `work_session.start` take files of any type. */
export const CHAT_FILES_FEATURE = "chat_files";

/** What one attachment is: a photo is sent as an image the model sees, a file is copied to the
 * work folder, a folder stays where it is and is named in the message. */
export type AttachmentKind = "image" | "file" | "folder";

export interface Attachment {
  kind: AttachmentKind;
  /** A path the server can read; for a folder the path on this machine. */
  path: string;
  name: string;
  /** An object URL to show a photo before it is sent; none for files, folders and picked photos. */
  preview?: string | null;
}

const IMAGE_NAME = /\.(png|jpe?g|gif|webp)$/i;

/** A photo the model can look at: by type when there is one, else by its name. */
export function isImage(file: { name: string; type?: string }): boolean {
  if (file.type) return /^image\/(png|jpeg|gif|webp)$/.test(file.type);
  return IMAGE_NAME.test(file.name);
}

/** The last part of a path, on any system. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter((p) => p !== "");
  return parts.at(-1) ?? path;
}

/**
 * The message with its folders named under it. A folder is not copied: the employee reads it
 * where it is, with its own tools.
 */
export function withFolders(text: string, folders: string[]): string {
  if (folders.length === 0) return text;
  const lines = folders.map((f) => `- ${f}`).join("\n");
  const head = folders.length === 1 ? "Folder to use:" : "Folders to use:";
  return text.trim() === "" ? `${head}\n${lines}` : `${text.trimEnd()}\n\n${head}\n${lines}`;
}

/** The paths of one kind, in the order they were added. */
export function pathsOf(items: Attachment[], kind: AttachmentKind): string[] {
  return items.filter((i) => i.kind === kind).map((i) => i.path);
}

/** Adds what is new; the same path twice stays once. */
export function mergeAttachments(now: Attachment[], more: Attachment[], max: number): Attachment[] {
  const next = [...now];
  for (const item of more) if (!next.some((x) => x.kind === item.kind && x.path === item.path)) next.push(item);
  return next.slice(0, max);
}
