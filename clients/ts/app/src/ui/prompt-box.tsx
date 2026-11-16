// The input as in a chat app, shared by New session and a work session: a rounded card with the
// attachments and the words on top, and under them + (photos, files, a folder), the run's chips and
// Send. New session tucks a tray under the card with where and by whom the work goes.

import { IconButton } from "@astryxdesign/core/IconButton";
import * as stylex from "@stylexjs/stylex";
import type { ComponentProps, ReactNode } from "react";
import { AttachmentTray, type PromptAttachments } from "./attach";
import { ArrowUpIcon, PlusIcon } from "./icons";
import { Menu } from "./menu";

const styles = stylex.create({
  stack: { display: "flex", flexDirection: "column" },
  card: {
    position: "relative",
    zIndex: 1,
    display: "flex",
    flexDirection: "column",
    gap: 4,
    borderRadius: 20,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border)", ":focus-within": "var(--color-border-emphasized)" },
    backgroundColor: "var(--color-background-card)",
    boxShadow: "var(--precision-card-shadow)",
    paddingBlock: 12,
    paddingInline: 12,
  },
  textarea: {
    display: "block",
    width: "100%",
    resize: "none",
    borderWidth: 0,
    backgroundColor: "transparent",
    paddingBlock: 2,
    paddingInline: 6,
    fontFamily: "inherit",
    fontSize: 15,
    lineHeight: 1.45,
    color: "var(--color-text-primary)",
    outlineStyle: { default: "none", ":focus-visible": "none" },
    "::placeholder": { color: "var(--precision-text3)" },
    ":disabled": { cursor: "not-allowed" },
  },
  bar: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 4 },
  right: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 4, marginInlineStart: "auto" },
  send: {
    display: "inline-flex",
    flexShrink: 0,
    width: 34,
    height: 34,
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    alignItems: "center",
    justifyContent: "center",
    marginInlineStart: 4,
    borderWidth: 0,
    borderRadius: 999,
    backgroundColor: { default: "var(--color-accent)", ":disabled": "var(--color-overlay-hover)" },
    color: { default: "var(--color-on-accent)", ":disabled": "var(--precision-text3)" },
    outline: { default: "none", ":focus-visible": "2px solid var(--focus-outline-color)" },
    outlineOffset: 2,
  },
  tray: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 4,
    // Starts under the card's rounded bottom, so the card seems to rest on it.
    marginBlockStart: -20,
    marginInline: 12,
    borderWidth: 1,
    borderTopWidth: 0,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderEndStartRadius: 14,
    borderEndEndRadius: 14,
    backgroundColor: "var(--color-background-muted)",
    paddingInline: 8,
    paddingBlockStart: 26,
    paddingBlockEnd: 6,
  },
});

/** The words of a prompt box: a borderless textarea in the box's type. */
export function PromptTextarea(props: Omit<ComponentProps<"textarea">, "className" | "style">) {
  return <textarea {...props} {...stylex.props(styles.textarea)} />;
}

type AddKey = "photos" | "files" | "folder";

export function PromptBox({
  attach,
  input,
  chips,
  sendActions,
  send,
  tray,
}: {
  attach: PromptAttachments;
  /** The words: a `PromptTextarea`, with what floats over it (a list of people to mention). */
  input: ReactNode;
  /** Beside Send: the next run's model, effort and mode. */
  chips?: ReactNode;
  /** Before Send, while a run is under way: Send now. */
  sendActions?: ReactNode;
  send: { label: string; title: string; disabled: boolean; onClick: () => void };
  /** Under the card: where and by whom the work goes. */
  tray?: ReactNode;
}) {
  const options: { key: AddKey; label: string; description: string }[] = [
    { key: "photos", label: "Photos…", description: "The employee sees them with your message" },
    ...(attach.pickFiles
      ? [{ key: "files" as const, label: "Files…", description: "Copied into the work folder" }]
      : []),
    ...(attach.pickFolder
      ? [{ key: "folder" as const, label: "Folder…", description: "Named in your message; it stays where it is" }]
      : []),
  ];
  const onPick = (key: AddKey) => {
    if (key === "photos") attach.pickPhotos();
    else if (key === "files") attach.pickFiles?.();
    else attach.pickFolder?.();
  };
  return (
    <div {...stylex.props(styles.stack)}>
      <div {...attach.target} {...stylex.props(styles.card)}>
        {attach.photoPicker}
        <AttachmentTray attach={attach} />
        {input}
        <div {...stylex.props(styles.bar)}>
          <Menu
            label="Add"
            options={options}
            onPick={onPick}
            trigger={<IconButton label="Add photos, files or a folder" icon={<PlusIcon />} variant="ghost" size="sm" />}
          />
          <div {...stylex.props(styles.right)}>
            {chips}
            {sendActions}
            <button
              type="button"
              aria-label={send.label}
              title={send.title}
              disabled={send.disabled}
              onClick={send.onClick}
              {...stylex.props(styles.send)}
            >
              <ArrowUpIcon />
            </button>
          </div>
        </div>
      </div>
      {tray ? <div {...stylex.props(styles.tray)}>{tray}</div> : null}
    </div>
  );
}
