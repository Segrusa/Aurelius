// A design artifact, live: the HTML a run kept. It does not run in this page: the desktop lays a
// view of its own over the stage, in a session without network (clients/electron/src/main/
// artifact-session.ts). Its scripts run; it reaches nothing outside it. The page only says where
// the stage is. The view lies above the page, so it steps aside while a menu, a toast or a later
// dialog lies over the stage.
//
// A design has pages (its HTML files, each at its newest). The board member comments on elements
// (the desktop picks them in an isolated world of the view) and asks for a PowerPoint. Both go
// back to the employee as one ordinary message in the conversation the design came from
// (artifact-feedback.ts).

import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { Icon } from "@astryxdesign/core/Icon";
import { IconButton } from "@astryxdesign/core/IconButton";
import { ResizeHandle, useResizable } from "@astryxdesign/core/Resizable";
import { percent, pixel } from "@astryxdesign/core/Resizable/utils";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { TextArea } from "@astryxdesign/core/TextArea";
import { ToggleButton } from "@astryxdesign/core/ToggleButton";
import { arrived, expectBody } from "@aurelius/client";
import type { MessageView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useRouterState } from "@tanstack/react-router";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { useConnection } from "../connection";
import { useKeyed } from "../data";
import type { ArtifactPick, ArtifactSlot, Platform } from "../platform";
import { saveBlob } from "../ui/download";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import {
  type ArtifactSend,
  artifactComments,
  artifactPath,
  CommentStore,
  commentsMessage,
  elementLabel,
  pptxMessage,
} from "./artifact-feedback";
import {
  type DesignPage,
  DIALOG_SELECTOR,
  designTicket,
  designTitle,
  htmlOf,
  htmlTitle,
  liveStep,
  OVERLAY_SELECTOR,
  overlaysOver,
  stageBounds,
  VIEWPORTS,
  type Viewport,
  viewportWidth,
  withDrag,
} from "./artifact-logic";
import { FormDialog, Hint, Loading } from "./org-kit";
import { overlays } from "./overlays-store";

const styles = stylex.create({
  bar: { display: "flex", flexWrap: "wrap", alignItems: "flex-end", gap: 8 },
  grow: { flexGrow: 1 },
  row: { display: "flex", gap: 8, minWidth: 0 },
  stage: {
    display: "flex",
    flexGrow: 1,
    flexBasis: 0,
    minWidth: 0,
    // The dialog gives the stage a height; the dock lets it fill what is left (DOCK_FILL).
    height: "var(--artifact-h, calc(100vh - 260px))",
    minHeight: "var(--artifact-min-h, 320px)",
    justifyContent: "center",
    overflow: "auto",
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--precision-sunk)",
  },
  frame: {
    display: "block",
    flexShrink: 0,
    height: "100%",
    maxWidth: "100%",
    // Under the view, which paints the artifact; white like its page until the view is there.
    backgroundColor: "white",
  },
  panel: {
    display: "flex",
    flexDirection: "column",
    gap: 10,
    width: 320,
    flexShrink: 0,
    height: "var(--artifact-h, calc(100vh - 260px))",
    minHeight: "var(--artifact-min-h, 320px)",
    overflowY: "auto",
    padding: 12,
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    boxSizing: "border-box",
  },
  panelTitle: { margin: 0, fontSize: 13, fontWeight: 600 },
  quiet: { margin: 0, fontSize: 12, color: "var(--precision-text3)" },
  target: {
    display: "flex",
    flexDirection: "column",
    gap: 2,
    padding: 8,
    borderRadius: 6,
    backgroundColor: "var(--precision-sunk)",
    fontSize: 12.5,
  },
  code: { fontFamily: "var(--font-mono, monospace)", fontSize: 11, overflowWrap: "anywhere" },
  list: { display: "flex", flexDirection: "column", gap: 6, margin: 0, padding: 0, listStyle: "none" },
  item: {
    display: "flex",
    gap: 6,
    alignItems: "flex-start",
    padding: 8,
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    fontSize: 12.5,
  },
  itemText: { display: "flex", flexDirection: "column", gap: 2, flexGrow: 1, minWidth: 0, overflowWrap: "anywhere" },
  keep: { flexShrink: 0 },
  actions: { display: "flex", gap: 6, flexWrap: "wrap", marginTop: "auto" },
  footerStart: { marginInlineEnd: "auto" },
  // The viewer docked beside the conversation, as Claude Desktop shows an artifact.
  dock: {
    display: "flex",
    flexDirection: "column",
    flexShrink: 0,
    minHeight: 0,
    backgroundColor: "var(--color-background-surface)",
  },
  dockTop: {
    display: "flex",
    height: 44,
    flexShrink: 0,
    alignItems: "center",
    gap: 8,
    borderBottomWidth: 1,
    borderBottomStyle: "solid",
    borderBottomColor: "var(--precision-line2)",
    paddingInlineStart: 14,
    paddingInlineEnd: 8,
  },
  dockTitle: {
    margin: 0,
    minWidth: 0,
    flexGrow: 1,
    flexBasis: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    fontSize: 13,
    fontWeight: 600,
  },
  dockBody: { display: "flex", flexDirection: "column", gap: 10, flexGrow: 1, minHeight: 0, padding: 12 },
  dockRow: { flexGrow: 1, minHeight: 0 },
  dockFoot: {
    display: "flex",
    flexShrink: 0,
    gap: 8,
    borderTopWidth: 1,
    borderTopStyle: "solid",
    borderTopColor: "var(--precision-line2)",
    paddingBlock: 8,
    paddingInline: 12,
  },
});

/** In the dock the stage and the tool panels stretch to the row instead of a set height. */
/** The dock's width at first, as a share of the row beside the conversation, never below its minimum. */
export const DOCK_DEFAULT_PERCENT = 50;
/** The narrowest the dock gets; the artifact's own tools still fit. */
export const DOCK_MIN_PX = 360;
/** The widest the dock gets, so the conversation stays readable beside it. */
export const DOCK_MAX_PERCENT = 70;
/** Where the dock's width is remembered (one width for every conversation). */
export const DOCK_SAVE_ID = "artifact-dock";

const DOCK_FILL = { "--artifact-h": "auto", "--artifact-min-h": "0px" } as React.CSSProperties;

type Artifacts = NonNullable<Platform["artifacts"]>;

/**
 * A design shown as it is written, not a kept file: its HTML so far (null while it loads), and
 * whether it is still being written. It has no pages; its tools wait until it is written.
 */
export interface LiveDesign {
  name: string;
  html: string | null;
  writing: boolean;
  error: string | null;
}

/** The HTML of one page, handed to main for as long as it is shown. */
function useServed(artifacts: Artifacts, html: string | undefined): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (html === undefined) return;
    let gone = false;
    let made: string | null = null;
    artifacts
      .serve(html)
      .then((served) => {
        made = served;
        if (gone) void artifacts.release(served);
        else setUrl(served);
      })
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This artifact did not open."));
    return () => {
      gone = true;
      setUrl(null);
      if (made) void artifacts.release(made);
    };
  }, [artifacts, html]);
  return url;
}

/** One kept page's HTML, from the server. */
function useArtifactFile(id: string | null) {
  const connection = useConnection();
  return useKeyed(
    ["artifact", id ?? ""],
    async () => {
      const got = expectBody(await connection.call("file.get", { id: id ?? "" }), "file").value;
      return { name: got.name, html: htmlOf(got.data_base64) };
    },
    Boolean(id),
  );
}

/** A view that is never held away (`useArtifactView`'s `held`). */
const NOT_HELD: React.RefObject<boolean> = { current: false };

/** The dialogs open now; the ones open when the view comes are under it and never count. */
const openDialogs = () => new Set([...document.querySelectorAll(DIALOG_SELECTOR)].filter((d) => d.checkVisibility()));

/** The page's overlays that the view, which lies above the page, would cover (overlaysOver). */
const overlaysOf = (stage: Element, before: ReadonlySet<Element>) =>
  overlaysOver(
    [...document.querySelectorAll(OVERLAY_SELECTOR)].map((element) => ({
      rect: element.getBoundingClientRect(),
      visible: element.checkVisibility(),
      isDialog: element.matches(DIALOG_SELECTOR),
      openBefore: before.has(element),
      holdsStage: element.contains(stage),
    })),
  );

/**
 * Shows the served artifact in the desktop's view (in `slot`) over `frame`, and keeps the view on
 * it: every animation frame the bounds are measured, and sent when they changed (resize, scroll,
 * the width toggle, the dialog moving). The view goes when the viewer closes, or another page or
 * reload shows a new one. `ready`: the view of this url and reload is there, so the tools may
 * run in it.
 */
function useArtifactView(
  artifacts: Artifacts,
  url: string | null,
  reloads: number,
  slot: ArtifactSlot,
  held: React.RefObject<boolean> = NOT_HELD,
) {
  const frame = useRef<HTMLDivElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState<string | null>(null);
  const showing = url === null ? null : `${url}#${reloads}`;
  // A reload shows the artifact in a new view.
  useEffect(() => {
    const box = frame.current;
    const around = stage.current;
    if (url === null || box === null || around === null) return;
    const before = openDialogs();
    const measure = () =>
      held.current
        ? null
        : stageBounds(
            box.getBoundingClientRect(),
            around.getBoundingClientRect(),
            { width: window.innerWidth, height: window.innerHeight },
            overlaysOf(around, before),
          );
    let sent = "";
    let up = false;
    let gone = false;
    let next = 0;
    const follow = () => {
      const now = measure();
      const key = JSON.stringify(now);
      if (up && key !== sent) {
        sent = key;
        void artifacts.place(now, slot);
      }
      next = requestAnimationFrame(follow);
    };
    const first = measure();
    sent = JSON.stringify(first);
    artifacts
      .show(url, first, slot)
      .then(() => {
        up = true;
        if (!gone) setShown(`${url}#${reloads}`);
      })
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This artifact did not open."));
    next = requestAnimationFrame(follow);
    return () => {
      gone = true;
      cancelAnimationFrame(next);
      setShown(null);
      void artifacts.hide(slot);
    };
  }, [artifacts, url, reloads, slot, held]);
  return { frame, stage, ready: showing !== null && shown === showing };
}

/** The stage a view lies over: the frame box at the chosen width, or what it is waiting for. */
function Stage({
  stage,
  frame,
  url,
  name,
  width,
  error,
}: {
  stage: React.RefObject<HTMLDivElement | null>;
  frame: React.RefObject<HTMLDivElement | null>;
  url: string | null;
  name: string;
  width: number;
  error: string | null;
}) {
  return (
    <div ref={stage} {...stylex.props(styles.stage)}>
      {url ? (
        <div
          ref={frame}
          role="img"
          aria-label={name}
          {...stylex.props(styles.frame)}
          // After stylex's props, which carry a `style` of their own.
          style={{ width }}
        />
      ) : error ? (
        <Hint>{error}</Hint>
      ) : (
        <Loading />
      )}
    </div>
  );
}

/** Sends one message with the viewer's send; true once the server has it. */
function usePost(send: ArtifactSend | null) {
  const client = useQueryClient();
  const [sending, setSending] = useState(false);
  const busy = useRef(false);
  const post = async (text: string): Promise<boolean> => {
    // One message per press: a second press while the first is on its way does nothing.
    if (send === null || busy.current) return false;
    busy.current = true;
    setSending(true);
    try {
      await send.send(text);
    } catch (e) {
      // The server has it; only its answer was lost on the way back.
      if (!arrived(e)) {
        toast(e instanceof Error ? e.message : "This did not send.");
        return false;
      }
    } finally {
      busy.current = false;
      setSending(false);
    }
    void client.invalidateQueries({ queryKey: ["messages"] });
    toast(`Sent to ${send.to}.`);
    return true;
  };
  return { post, sending };
}

const NO_SEND = "Feedback goes back from the employee's own chat, session or task.";

/**
 * Comment mode: the desktop outlines the element under the pointer in the view and gives the one
 * clicked; the board member types a comment on it. The comments of a page stay until they are sent.
 */
function CommentPanel({
  artifacts,
  ready,
  storeKey,
  path,
  send,
}: {
  artifacts: Artifacts;
  ready: boolean;
  storeKey: string;
  path: string;
  send: ArtifactSend | null;
}) {
  const comments = useSyncExternalStore(
    (changed) => artifactComments.subscribe(changed),
    () => artifactComments.list(storeKey),
  );
  const [target, setTarget] = useState<ArtifactPick | null>(null);
  const [draft, setDraft] = useState("");
  // A picked element: its comment is typed at once.
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (target !== null) field.current?.focus();
  }, [target]);
  const { post, sending } = usePost(send);

  useEffect(() => {
    if (!ready) return;
    let on = true;
    const loop = async () => {
      while (on) {
        const got = await artifacts.pick().catch(() => null);
        if (!on) return;
        if (got) setTarget(got);
        // Stopped or refused: try again in a moment, while the mode is still on.
        else await new Promise((resolve) => setTimeout(resolve, 400));
      }
    };
    void loop();
    return () => {
      on = false;
      void artifacts.stopPick();
    };
  }, [artifacts, ready]);

  const add = () => {
    if (target === null) return;
    const made = artifactComments.add(storeKey, {
      label: elementLabel(target),
      selector: target.selector,
      text: draft,
    });
    if (made === null) {
      toast(
        draft.trim() === "" ? "Type a comment first." : "This page holds as many comments as it can; send them first.",
      );
      return;
    }
    setTarget(null);
    setDraft("");
  };

  return (
    <aside aria-label="Comments" {...stylex.props(styles.panel)}>
      <p {...stylex.props(styles.panelTitle)}>Comments</p>
      {target ? (
        <>
          <div {...stylex.props(styles.target)}>
            <span>{elementLabel(target)}</span>
            <span {...stylex.props(styles.code)}>{target.selector}</span>
          </div>
          <TextArea
            ref={field}
            label="Comment"
            rows={3}
            value={draft}
            onChange={setDraft}
            onKeyDown={(event) => {
              if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
                event.preventDefault();
                add();
              }
            }}
          />
          <div {...stylex.props(styles.actions)}>
            <Button label="Add comment" size="sm" variant="primary" isDisabled={draft.trim() === ""} onClick={add} />
            <Button label="Cancel" size="sm" onClick={() => setTarget(null)} />
          </div>
        </>
      ) : (
        <p {...stylex.props(styles.quiet)}>Click an element in the design to comment on it.</p>
      )}
      {comments.length > 0 ? (
        <ol {...stylex.props(styles.list)} aria-label="Comments to send">
          {comments.map((c, i) => (
            <li key={c.id} {...stylex.props(styles.item)}>
              <span {...stylex.props(styles.itemText)}>
                <span>{`${i + 1}. ${c.label}`}</span>
                <span>{c.text}</span>
              </span>
              <span {...stylex.props(styles.keep)}>
                <Button
                  label="Delete"
                  size="sm"
                  variant="ghost"
                  aria-label={`Delete comment ${i + 1}`}
                  onClick={() => artifactComments.remove(storeKey, c.id)}
                />
              </span>
            </li>
          ))}
        </ol>
      ) : null}
      <div {...stylex.props(styles.actions)}>
        <Button
          label={sending ? "Sending…" : `Send to ${send?.to ?? "the employee"}`}
          variant="primary"
          size="sm"
          isDisabled={send === null || comments.length === 0 || sending}
          onClick={() =>
            void post(commentsMessage(path, comments)).then((ok) => {
              if (ok) artifactComments.clear(storeKey);
            })
          }
        />
        {send === null ? <p {...stylex.props(styles.quiet)}>{NO_SEND}</p> : null}
      </div>
    </aside>
  );
}

export function ArtifactViewer({
  pages,
  openName,
  artifacts,
  messages = [],
  conversationId = null,
  send = null,
  live = null,
  docked = false,
  notice = null,
  onClose,
}: {
  /** The design's pages, as `designPages` gives them; empty for a design being written. */
  pages: DesignPage[];
  /** The page shown first. */
  openName: string;
  artifacts: Artifacts;
  /** The conversation's messages: the answer that kept a page names its path. */
  messages?: readonly MessageView[];
  conversationId?: string | null;
  /** Where comments and the PowerPoint request go; null: nowhere (no Send). */
  send?: ArtifactSend | null;
  /** A design being written instead of kept pages (then `pages` is empty). */
  live?: LiveDesign | null;
  /** In a panel beside the conversation instead of a dialog over it. */
  docked?: boolean;
  /** Shown in the docked head before Close, e.g. the "New plan waiting" pill. */
  notice?: React.ReactNode;
  onClose: () => void;
}) {
  const [pageName, setPageName] = useState(openName);
  const [viewport, setViewport] = useState<Viewport>("desktop");
  const [reloads, setReloads] = useState(0);
  const [exporting, setExporting] = useState<"pdf" | "png" | null>(null);
  const [commenting, setCommenting] = useState(false);
  const current = pages.find((p) => p.name === pageName) ?? pages[0];
  const file = useArtifactFile(live ? null : (current?.file.id ?? null));
  const html = live ? (live.html ?? undefined) : file.data?.html;
  const writing = live?.writing === true;
  // A design being written loads once and then grows in that same view (liveStep): a new view
  // per draft flashed and jumped back to the top.
  const [liveServed, setLiveServed] = useState<string | undefined>(undefined);
  const step = live ? liveStep(liveServed, html, writing) : "none";
  if (step === "serve") setLiveServed(html);
  const url = useServed(artifacts, live ? liveServed : html);
  // The dock's width: dragged at its left edge, remembered. While it is dragged the view steps
  // aside, because the pointer over the view would go to the artifact's page, not to the handle.
  const resizing = useRef(false);
  // Shares are of the row the dock sits in (beside the conversation), not of the window.
  const dockRow = useRef<HTMLElement | null>(null);
  const dockSize = useResizable({
    containerRef: dockRow,
    defaultSize: percent(DOCK_DEFAULT_PERCENT, { min: pixel(DOCK_MIN_PX) }),
    minSize: DOCK_MIN_PX,
    maxSize: `${DOCK_MAX_PERCENT}%`,
    autoSaveId: DOCK_SAVE_ID,
  });
  const dockHandle = withDrag(dockSize.props, (on) => {
    resizing.current = on;
  });
  const { frame, stage, ready } = useArtifactView(artifacts, url, reloads, "main", resizing);
  const growing = ready && step === "update" ? html : undefined;
  useEffect(() => {
    if (growing !== undefined) void artifacts.update(growing);
  }, [artifacts, growing]);
  const loadError = live ? live.error : (file.error?.message ?? null);
  const { post } = usePost(send);
  // Another screen: the viewer closes with it (main takes the view away as well).
  const path = useRouterState({ select: (s) => s.location.href });
  const openedAt = useRef(path);
  useEffect(() => {
    if (path !== openedAt.current) onClose();
  }, [path, onClose]);
  const name = live?.name ?? current?.file.name ?? file.data?.name ?? "Artifact";
  // A page shows by its own <title> (also one kept before the server read titles), else its readable file name.
  const pageTitle =
    (live ? null : current?.file.title?.trim()) ||
    (html !== undefined ? htmlTitle(html) : null) ||
    designTitle({ name });
  const answer = messages.find((m) => m.id === current?.messageId)?.text ?? "";
  const artifactName = artifactPath(answer, name);
  const width = viewportWidth(viewport);

  const exportAs = async (kind: "pdf" | "png") => {
    if (url === null) return;
    setExporting(kind);
    try {
      const path = await artifacts.export(url, kind, name, width);
      if (path) toast(`Saved ${path}`);
    } catch (e) {
      toast(e instanceof Error ? e.message : "The export did not work.");
    } finally {
      setExporting(null);
    }
  };
  type ExportKey = "html" | "pdf" | "png" | "pptx";
  const exportOptions: { key: ExportKey; label: string; description?: string; dividerBefore?: boolean }[] = [
    { key: "html", label: "Export HTML" },
    { key: "pdf", label: "Export PDF" },
    { key: "png", label: "Export PNG" },
    ...(send
      ? [
          {
            key: "pptx" as const,
            label: "Ask for PPTX",
            description: `${send.to} makes a PowerPoint of this page`,
            dividerBefore: true,
          },
        ]
      : []),
  ];
  const exportPick = (key: ExportKey) => {
    if (key === "html") {
      if (html !== undefined) saveBlob(new Blob([html], { type: "text/html" }), name);
    } else if (key === "pptx") void post(pptxMessage(artifactName));
    else if (url !== null && exporting === null) void exportAs(key);
  };

  // A ticket to build this page, with the page as its file.
  const ticket = () => {
    if (html === undefined) return;
    overlays.open("ticket", {
      ...designTicket(pageTitle, artifactName),
      files: [new File([html], name, { type: "text/html" })],
    });
  };

  const title = writing ? `${pageTitle} · Writing…` : pageTitle;
  const tools = (
    <div {...stylex.props(styles.bar)}>
      {pages.length > 1 ? (
        <Selector
          label="Page"
          size="sm"
          value={current?.name ?? pageName}
          onChange={(v) => setPageName(v)}
          options={pages.map((p) => ({ value: p.name, label: p.title }))}
        />
      ) : null}
      <span {...stylex.props(styles.grow)} />
      <ToggleButton
        size="sm"
        label="Comment"
        isPressed={commenting}
        isDisabled={url === null || writing}
        onPressedChange={() => setCommenting((now) => !now)}
      />
      <SegmentedControl label="Width" size="sm" value={viewport} onChange={(v) => setViewport(v as Viewport)}>
        {VIEWPORTS.map((v) => (
          <SegmentedControlItem key={v.value} value={v.value} label={v.label} />
        ))}
      </SegmentedControl>
    </div>
  );
  const actions = (
    <>
      <span {...stylex.props(styles.footerStart)}>
        <Button
          label="Reload"
          onClick={() => {
            setReloads((n) => n + 1);
            if (live) setLiveServed(html);
            else void file.refetch();
          }}
        />
      </span>
      <Button label="Create ticket" isDisabled={html === undefined || writing} onClick={ticket} />
      <Menu
        label="Export"
        options={exportOptions}
        onPick={exportPick}
        trigger={<Button label={exporting ? "Exporting…" : "Export"} isDisabled={exporting !== null || writing} />}
      />
    </>
  );
  const view = (extra: stylex.StyleXStyles | null) => (
    <div {...stylex.props(styles.row, extra)}>
      <Stage stage={stage} frame={frame} url={url} name={name} width={width} error={loadError} />
      {commenting && !writing ? (
        <CommentPanel
          key={`${current?.file.id}#${reloads}`}
          artifacts={artifacts}
          ready={ready}
          storeKey={CommentStore.key(conversationId, name, current?.file.id ?? "")}
          path={artifactName}
          send={send}
        />
      ) : null}
    </div>
  );

  if (docked)
    return (
      <>
        <ResizeHandle direction="horizontal" isReversed hasDivider label="Resize design panel" resizable={dockHandle} />
        <aside
          ref={(el) => {
            dockRow.current = el?.parentElement ?? null;
          }}
          aria-label={`Design ${name}`}
          {...stylex.props(styles.dock)}
          style={{ ...DOCK_FILL, width: dockSize.size }}
        >
          <div {...stylex.props(styles.dockTop)}>
            <Heading level={2} xstyle={styles.dockTitle}>
              {title}
            </Heading>
            {notice}
            <IconButton size="sm" variant="ghost" label="Close design" icon={<Icon icon="close" />} onClick={onClose} />
          </div>
          <div {...stylex.props(styles.dockBody)}>
            {tools}
            {view(styles.dockRow)}
          </div>
          <div {...stylex.props(styles.dockFoot)}>{actions}</div>
        </aside>
      </>
    );
  return (
    <FormDialog
      title={title}
      width={1340}
      onClose={onClose}
      footer={
        <>
          {actions}
          <Button label="Close" variant="primary" onClick={onClose} />
        </>
      }
    >
      {tools}
      {view(null)}
    </FormDialog>
  );
}
