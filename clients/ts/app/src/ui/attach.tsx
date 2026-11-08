// Files for a form: chosen, pasted or dropped. Each ends up as a path the server can read
// (`Platform.pickFiles` and `Platform.keepFiles`); the same limits as the server's.

import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { type ClipboardEvent, type DragEvent, useCallback, useState } from "react";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { toast } from "./toast";

const styles = stylex.create({
  row: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 6 },
  images: { display: "flex", flexWrap: "wrap", gap: 6, paddingInline: 10, paddingTop: 8 },
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

/** Images pasted in a chat's box. "+ File" and dropping stay hidden there (`ChatAttachments`). */
export function useImagePaste() {
  const platform = usePlatform();
  const [files, setFiles] = useState<Attached[]>([]);
  const onPaste = useCallback(
    (event: ClipboardEvent) => {
      const images = filesFrom(event.clipboardData).filter((f) => f.type.startsWith("image/"));
      if (images.length === 0) return;
      event.preventDefault();
      const { fit, problems } = acceptable(images);
      for (const problem of problems) toast(problem);
      platform.keepFiles(fit).then(
        (kept) => setFiles((now) => [...now, ...kept].slice(0, FILES_MAX)),
        (e: unknown) => toast(failure(e)),
      );
    },
    [platform],
  );
  return {
    files,
    paths: files.map((f) => f.path),
    remove: (path: string) => setFiles((now) => now.filter((f) => f.path !== path)),
    clear: () => setFiles([]),
    target: { onPaste },
  };
}

export function AttachedImages({ images }: { images: ReturnType<typeof useImagePaste> }) {
  if (images.files.length === 0) return null;
  return (
    <div {...stylex.props(styles.images)}>
      {images.files.map((f) => (
        <button
          key={f.path}
          type="button"
          aria-label={`Remove ${f.name}`}
          onClick={() => images.remove(f.path)}
          {...stylex.props(styles.file)}
        >
          ▣ {f.name} ×
        </button>
      ))}
    </div>
  );
}
