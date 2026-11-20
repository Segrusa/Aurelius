// Files for a form: chosen, pasted or dropped. Each ends up as a path the server can read
// (`Platform.pickFiles` and `Platform.keepFiles`); the same limits as the server's.

import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { type ClipboardEvent, type DragEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { type Attachment, baseName, isImage, mergeAttachments, pathsOf } from "./attach-logic";
import { toast } from "./toast";

const styles = stylex.create({
  row: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 6 },
  file: {
    cursor: "pointer",
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border)", ":hover": "var(--color-border-emphasized)" },
    backgroundColor: "transparent",
    paddingInline: 8,
    paddingBlock: 4,
    fontSize: 12,
    color: "var(--color-text-primary)",
  },
});

export const FILES_MAX = 8;
export const FILE_MAX_BYTES = 25 * 1024 * 1024;

export interface Attached {
  path: string;
  name: string;
}

const failure = (e: unknown) => (e instanceof Error ? e.message : "This did not work.");

/** What is too big or too many, as words; the rest goes on. */
export function acceptable(files: File[]): { fit: File[]; problems: string[] } {
  const problems: string[] = [];
  const fit = files.filter((f) => {
    if (f.size <= FILE_MAX_BYTES) return true;
    problems.push(`${f.name} is ${Math.ceil(f.size / 1024 / 1024)} MB; files can be at most 25 MB.`);
    return false;
  });
  return { fit, problems };
}

export function filesFrom(data: DataTransfer | null): File[] {
  return data ? [...data.files] : [];
}

export function useAttachments(kept = 0) {
  const platform = usePlatform();
  const [files, setFiles] = useState<Attached[]>([]);
  const merge = useCallback(
    (more: Attached[]) => {
      setFiles((now) => {
        const next = [...now, ...more.filter((f) => !now.some((x) => x.path === f.path))];
        if (next.length + kept > FILES_MAX) toast(`At most ${FILES_MAX} files.`);
        return next.slice(0, Math.max(0, FILES_MAX - kept));
      });
    },
    [kept],
  );
  const pick = useCallback(() => {
    platform.pickFiles().then(merge, (e: unknown) => toast(failure(e)));
  }, [merge, platform]);
  const take = useCallback(
    (given: File[]) => {
      const { fit, problems } = acceptable(given);
      for (const problem of problems) toast(problem);
      if (fit.length > 0) platform.keepFiles(fit).then(merge, (e: unknown) => toast(failure(e)));
    },
    [merge, platform],
  );
  const onPaste = useCallback(
    (event: ClipboardEvent) => {
      const given = filesFrom(event.clipboardData);
      if (given.length === 0) return;
      // A copied image or file; plain text stays the field's.
      event.preventDefault();
      take(given);
    },
    [take],
  );
  const onDrop = useCallback(
    (event: DragEvent) => {
      const given = filesFrom(event.dataTransfer);
      if (given.length === 0) return;
      event.preventDefault();
      take(given);
    },
    [take],
  );
  const onDragOver = useCallback((event: DragEvent) => {
    if (event.dataTransfer.types.includes("Files")) event.preventDefault();
  }, []);
  useAction("AttachFiles", pick);
  return {
    files,
    pick,
    remove: (path: string) => setFiles((now) => now.filter((f) => f.path !== path)),
    clear: () => setFiles([]),
    paths: files.map((f) => f.path),
    /** Spread on the part of a form that takes pasted and dropped files. */
    target: { onPaste, onDrop, onDragOver },
  };
}

export function AttachRow({ attach }: { attach: ReturnType<typeof useAttachments> }) {
  return (
    <div {...stylex.props(styles.row)}>
      <Button label="Add files…" onClick={attach.pick} />
      {attach.files.map((f) => (
        <button
          key={f.path}
          type="button"
          aria-label={`Remove ${f.name}`}
          onClick={() => attach.remove(f.path)}
          {...stylex.props(styles.file)}
        >
          {f.name} ×
        </button>
      ))}
    </div>
  );
}

const tray = stylex.create({
  row: { display: "flex", flexWrap: "wrap", alignItems: "flex-start", gap: 8, paddingInline: 4, paddingBlockEnd: 6 },
  thumb: {
    position: "relative",
    width: 64,
    height: 64,
    flexShrink: 0,
    overflow: "hidden",
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-muted)",
  },
  img: { display: "block", width: "100%", height: "100%", objectFit: "cover" },
  // A photo picked from disk has no object URL to show: its name stands in.
  noPreview: {
    display: "flex",
    width: "100%",
    height: "100%",
    alignItems: "center",
    justifyContent: "center",
    padding: 4,
    overflow: "hidden",
    fontSize: 10,
    textAlign: "center",
    wordBreak: "break-all",
    color: "var(--precision-text3)",
  },
  chip: {
    display: "inline-flex",
    maxWidth: 240,
    alignItems: "center",
    gap: 6,
    height: 30,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-muted)",
    paddingInlineStart: 8,
    paddingInlineEnd: 2,
    fontSize: 12,
    color: "var(--color-text-primary)",
  },
  chipName: { minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  glyph: { flexShrink: 0, color: "var(--precision-text3)" },
  remove: {
    display: "inline-flex",
    width: 22,
    height: 22,
    flexShrink: 0,
    cursor: "pointer",
    alignItems: "center",
    justifyContent: "center",
    borderWidth: 0,
    borderRadius: 999,
    backgroundColor: { default: "transparent", ":hover": "var(--color-overlay-hover)" },
    padding: 0,
    fontSize: 14,
    lineHeight: 1,
    color: "var(--precision-text3)",
  },
  removeOnThumb: {
    position: "absolute",
    top: 3,
    right: 3,
    width: 20,
    height: 20,
    backgroundColor: { default: "rgb(0 0 0 / 0.55)", ":hover": "rgb(0 0 0 / 0.75)" },
    color: "white",
  },
});

/**
 * What a prompt box carries along: photos (pasted, dropped or picked, with a preview where the
 * photo is at hand), files and folders. `files` and `folders` say what the place it sends to takes;
 * photos it always takes.
 */
export function usePromptAttachments({
  files: takesFiles,
  folders: takesFolders,
  shortcut = true,
}: {
  files: boolean;
  folders: boolean;
  /** Add files (the keyboard) opens this box's picker; off where the box has no +. */
  shortcut?: boolean;
}) {
  const platform = usePlatform();
  const [items, setItems] = useState<Attachment[]>([]);
  // Object URLs of the previews, revoked when they go and when the box goes.
  const urls = useRef(new Set<string>());
  useEffect(() => {
    const kept = urls.current;
    return () => {
      for (const url of kept) URL.revokeObjectURL(url);
      kept.clear();
    };
  }, []);
  const forget = useCallback((gone: Attachment[]) => {
    for (const item of gone) {
      if (item.preview && urls.current.delete(item.preview)) URL.revokeObjectURL(item.preview);
    }
  }, []);
  const add = useCallback(
    (more: Attachment[]) =>
      setItems((now) => {
        const next = mergeAttachments(now, more, FILES_MAX);
        if (now.length + more.length > FILES_MAX) toast(`At most ${FILES_MAX} attachments.`);
        forget(more.filter((m) => !next.includes(m)));
        return next;
      }),
    [forget],
  );
  /** Pasted, dropped or picked in the page: photos get a preview, other files only where they are taken. */
  const take = useCallback(
    (given: File[]) => {
      const photos = given.filter((f) => isImage(f));
      const others = given.filter((f) => !isImage(f));
      if (others.length > 0 && !takesFiles) toast("Only photos can be added here.");
      const wanted = takesFiles ? [...photos, ...others] : photos;
      const { fit, problems } = acceptable(wanted);
      for (const problem of problems) toast(problem);
      if (fit.length === 0) return;
      platform.keepFiles(fit).then(
        (kept) =>
          add(
            kept.map((k, i) => {
              const file = fit[i] as File;
              const photo = isImage(file);
              const preview = photo ? URL.createObjectURL(file) : null;
              if (preview) urls.current.add(preview);
              return { kind: photo ? "image" : "file", path: k.path, name: k.name || file.name, preview };
            }),
          ),
        (e: unknown) => toast(failure(e)),
      );
    },
    [add, platform, takesFiles],
  );
  const photoInput = useRef<HTMLInputElement | null>(null);
  const pickPhotos = useCallback(() => photoInput.current?.click(), []);
  const pickFiles = useCallback(() => {
    platform.pickFiles().then(
      (picked) =>
        add(picked.map((p) => ({ kind: isImage({ name: p.name }) ? "image" : "file", path: p.path, name: p.name }))),
      (e: unknown) => toast(failure(e)),
    );
  }, [add, platform]);
  const choose = platform.pickFolder;
  const pickFolder = useMemo(
    () =>
      choose && takesFolders
        ? () => {
            choose("Add a folder").then(
              (path) => {
                if (path) add([{ kind: "folder", path, name: baseName(path) }]);
              },
              (e: unknown) => toast(failure(e)),
            );
          }
        : null,
    [add, choose, takesFolders],
  );
  const onPaste = useCallback(
    (event: ClipboardEvent) => {
      const given = filesFrom(event.clipboardData);
      if (given.length === 0) return;
      // A copied photo or file; plain text stays the field's.
      event.preventDefault();
      take(given);
    },
    [take],
  );
  const onDrop = useCallback(
    (event: DragEvent) => {
      const entries = [...event.dataTransfer.items].map((i) => i.webkitGetAsEntry?.() ?? null);
      if (entries.some((e) => e?.isDirectory)) {
        event.preventDefault();
        toast(pickFolder ? "Add a folder with + and then Folder…" : "A folder cannot be added here.");
        return;
      }
      const given = filesFrom(event.dataTransfer);
      if (given.length === 0) return;
      event.preventDefault();
      take(given);
    },
    [pickFolder, take],
  );
  const onDragOver = useCallback((event: DragEvent) => {
    if (event.dataTransfer.types.includes("Files")) event.preventDefault();
  }, []);
  useAction("AttachFiles", takesFiles ? pickFiles : pickPhotos, shortcut);
  return {
    items,
    takesFiles,
    pickPhotos,
    pickFiles: takesFiles ? pickFiles : null,
    pickFolder,
    images: pathsOf(items, "image"),
    files: pathsOf(items, "file"),
    folders: pathsOf(items, "folder"),
    remove: (item: Attachment) => {
      forget([item]);
      setItems((now) => now.filter((x) => x !== item));
    },
    clear: () => {
      setItems((now) => {
        forget(now);
        return [];
      });
    },
    /** Spread on the part of the page that takes pasted and dropped photos and files. */
    target: { onPaste, onDrop, onDragOver },
    /** The hidden photo picker; render it once, anywhere in the box. */
    photoPicker: (
      <input
        ref={photoInput}
        type="file"
        accept="image/png,image/jpeg,image/gif,image/webp"
        multiple
        hidden
        onChange={(e) => {
          const chosen = [...(e.target.files ?? [])];
          e.target.value = "";
          if (chosen.length > 0) take(chosen);
        }}
      />
    ),
  };
}

export type PromptAttachments = ReturnType<typeof usePromptAttachments>;

/** The attachments above the words: photos as small previews, files and folders as chips. */
export function AttachmentTray({ attach }: { attach: PromptAttachments }) {
  if (attach.items.length === 0) return null;
  return (
    <div {...stylex.props(tray.row)}>
      {attach.items.map((item) =>
        item.kind === "image" ? (
          <div key={`${item.kind}:${item.path}`} title={item.name} {...stylex.props(tray.thumb)}>
            {item.preview ? (
              <img src={item.preview} alt={item.name} {...stylex.props(tray.img)} />
            ) : (
              <span {...stylex.props(tray.noPreview)}>{item.name}</span>
            )}
            <button
              type="button"
              aria-label={`Remove ${item.name}`}
              onClick={() => attach.remove(item)}
              {...stylex.props(tray.remove, tray.removeOnThumb)}
            >
              ×
            </button>
          </div>
        ) : (
          <span key={`${item.kind}:${item.path}`} title={item.path} {...stylex.props(tray.chip)}>
            <span aria-hidden="true" {...stylex.props(tray.glyph)}>
              {item.kind === "folder" ? "▤" : "▢"}
            </span>
            <span {...stylex.props(tray.chipName)}>{item.name}</span>
            <button
              type="button"
              aria-label={`Remove ${item.name}`}
              onClick={() => attach.remove(item)}
              {...stylex.props(tray.remove)}
            >
              ×
            </button>
          </span>
        ),
      )}
    </div>
  );
}
