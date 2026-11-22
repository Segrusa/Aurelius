// A design artifact, live: the HTML a run kept. It does not run in this page: the desktop lays a
// view of its own over the stage, in a session without network (clients/electron/src/main/
// artifact-session.ts). Its scripts run; it reaches nothing outside it. The page only says where
// the stage is. The view lies above the page, so it steps aside while a menu, a toast or a later
// dialog lies over the stage.
//
// Phase B: the board member comments on elements (the desktop picks them in an isolated world of
// the view), tries the tweaks the artifact declares, compares two versions side by side and asks
// for a PowerPoint. All of it goes back to the employee as one ordinary message in the
// conversation the artifact came from (artifact-feedback.ts).

import { Button } from "@astryxdesign/core/Button";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { Slider } from "@astryxdesign/core/Slider";
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
import type { ArtifactPick, ArtifactSlot, ArtifactTweak, Platform } from "../platform";
import { saveBlob } from "../ui/download";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import {
  type ArtifactSend,
  artifactComments,
  artifactPath,
  CommentStore,
  changedTweaks,
  commentsMessage,
  elementLabel,
  hexColor,
  numberOf,
  pptxMessage,
  tweakInput,
  tweaksMessage,
} from "./artifact-feedback";
import {
  type ArtifactVersion,
  DIALOG_SELECTOR,
  htmlOf,
  OVERLAY_SELECTOR,
  overlaysOver,
  stageBounds,
  VIEWPORTS,
  type Viewport,
  versionLabel,
  viewportWidth,
} from "./artifact-logic";
import { FormDialog, Hint, Loading } from "./org-kit";

const styles = stylex.create({
  bar: { display: "flex", flexWrap: "wrap", alignItems: "flex-end", gap: 8 },
  grow: { flexGrow: 1 },
  row: { display: "flex", gap: 8, minWidth: 0 },
  stage: {
    display: "flex",
    flexGrow: 1,
    flexBasis: 0,
    minWidth: 0,
    height: "calc(100vh - 260px)",
    minHeight: 320,
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
    height: "calc(100vh - 260px)",
    minHeight: 320,
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
  actions: { display: "flex", gap: 6, flexWrap: "wrap", marginTop: "auto" },
  colorRow: { display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8, fontSize: 13 },
  color: { width: 44, height: 28, padding: 0, borderWidth: 0, backgroundColor: "transparent", cursor: "pointer" },
  footerStart: { marginInlineEnd: "auto" },
});

type Artifacts = NonNullable<Platform["artifacts"]>;

/** The HTML of one version, handed to main for as long as it is shown. */
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

/** One kept version's HTML, from the server. */
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
 * the width toggle, the dialog moving). The view goes when the viewer closes, or a new version or
 * reload shows a new one. `ready`: the view of this url and reload is there, so the tools may
 * run in it.
 */
function useArtifactView(artifacts: Artifacts, url: string | null, reloads: number, slot: ArtifactSlot) {
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
      stageBounds(
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
  }, [artifacts, url, reloads, slot]);
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

/** The older version beside the current one, in a view of its own with the same guards. */
function CompareStage({
  artifacts,
  id,
  name,
  width,
}: {
  artifacts: Artifacts;
  id: string;
  name: string;
  width: number;
}) {
  const file = useArtifactFile(id);
  const url = useServed(artifacts, file.data?.html);
  const { frame, stage } = useArtifactView(artifacts, url, 0, "compare");
  return <Stage stage={stage} frame={frame} url={url} name={name} width={width} error={file.error?.message ?? null} />;
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
 * clicked; the board member types a comment on it. The comments of a version stay in the page
 * until they are sent.
 */
function CommentPanel({
  artifacts,
  ready,
  storeKey,
  version,
  path,
  send,
}: {
  artifacts: Artifacts;
  ready: boolean;
  storeKey: string;
  version: number;
  path: string;
  send: ArtifactSend | null;
}) {
  const comments = useSyncExternalStore(
    (changed) => artifactComments.subscribe(changed),
    () => artifactComments.list(storeKey),
  );
  const [target, setTarget] = useState<ArtifactPick | null>(null);
  const [draft, setDraft] = useState("");
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
      version,
      label: elementLabel(target),
      selector: target.selector,
      text: draft,
    });
    if (made === null) {
      toast(
        draft.trim() === ""
          ? "Type a comment first."
          : "This version holds as many comments as it can; send them first.",
      );
      return;
    }
    setTarget(null);
    setDraft("");
  };

  return (
    <aside aria-label="Comments" {...stylex.props(styles.panel)}>
      <p {...stylex.props(styles.panelTitle)}>{`Comments on v${version}`}</p>
      {target ? (
        <>
          <div {...stylex.props(styles.target)}>
            <span>{elementLabel(target)}</span>
            <span {...stylex.props(styles.code)}>{target.selector}</span>
          </div>
          <TextArea
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
              <Button
                label="Delete"
                size="sm"
                variant="ghost"
                aria-label={`Delete comment ${i + 1}`}
                onClick={() => artifactComments.remove(storeKey, c.id)}
              />
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
            void post(commentsMessage(path, version, comments)).then((ok) => {
              if (ok) artifactComments.clear(storeKey);
            })
          }
        />
        {send === null ? <p {...stylex.props(styles.quiet)}>{NO_SEND}</p> : null}
      </div>
    </aside>
  );
}

/** One control of a tweak, with the value it shows now. */
function TweakControl({
  tweak,
  value,
  onChange,
}: {
  tweak: ArtifactTweak;
  value: string | undefined;
  onChange: (input: string | number) => void;
}) {
  const now = value ?? tweak.value;
  switch (tweak.type) {
    case "color":
      return (
        <label {...stylex.props(styles.colorRow)}>
          <span>{tweak.label}</span>
          <input
            type="color"
            aria-label={tweak.label}
            value={hexColor(now) ?? "#000000"}
            onChange={(event) => onChange(event.target.value)}
            {...stylex.props(styles.color)}
          />
        </label>
      );
    case "length":
    case "number": {
      const unit = tweak.type === "length" ? tweak.unit : "";
      const current = numberOf(now, unit) ?? tweak.min ?? 0;
      const min = tweak.min ?? Math.min(0, current);
      const max = tweak.max ?? Math.max(current * 2, min + 1);
      return (
        <Slider
          label={tweak.label}
          min={min}
          max={max}
          step={tweak.step ?? (tweak.type === "length" ? 1 : 0.1)}
          value={Math.min(max, Math.max(min, current))}
          valueDisplay="text"
          formatValue={(n) => `${n}${unit}`}
          onChange={(n: number) => onChange(n)}
        />
      );
    }
    case "choice":
      return (
        <Selector
          label={tweak.label}
          size="sm"
          value={tweak.options.includes(now) ? now : (tweak.options[0] ?? "")}
          onChange={(v) => onChange(v)}
          options={tweak.options.map((o) => ({ value: o, label: o }))}
        />
      );
  }
}

/** The tweaks the artifact declares: each changes a CSS variable live; Send asks for new defaults. */
function TweaksPanel({
  artifacts,
  ready,
  version,
  path,
  send,
}: {
  artifacts: Artifacts;
  ready: boolean;
  version: number;
  path: string;
  send: ArtifactSend | null;
}) {
  const [tweaks, setTweaks] = useState<ArtifactTweak[] | null>(null);
  const [values, setValues] = useState<ReadonlyMap<string, string>>(new Map());
  const { post, sending } = usePost(send);

  useEffect(() => {
    setValues(new Map());
    if (!ready) {
      setTweaks(null);
      return;
    }
    let gone = false;
    artifacts
      .tweaks()
      .then((got) => {
        if (!gone) setTweaks(got);
      })
      .catch(() => {
        if (!gone) setTweaks([]);
      });
    return () => {
      gone = true;
    };
  }, [artifacts, ready]);

  const change = (tweak: ArtifactTweak, input: string | number) => {
    const value = tweakInput(tweak, input);
    setValues((before) => new Map(before).set(tweak.var, value));
    void artifacts.tweak(tweak.var, value).then((set) => {
      if (set === null) toast(`${tweak.label}: ${value} is not a value it takes.`);
    });
  };
  const changed = tweaks ? changedTweaks(tweaks, values) : [];

  return (
    <aside aria-label="Tweaks" {...stylex.props(styles.panel)}>
      <p {...stylex.props(styles.panelTitle)}>{`Tweaks for v${version}`}</p>
      {tweaks === null ? (
        <Loading />
      ) : tweaks.length === 0 ? (
        <p {...stylex.props(styles.quiet)}>
          This design declares no tweaks. An employee adds them with an aurelius-tweaks block (see the authoring guide).
        </p>
      ) : (
        tweaks.map((t) => (
          <TweakControl key={t.var} tweak={t} value={values.get(t.var)} onChange={(input) => change(t, input)} />
        ))
      )}
      <div {...stylex.props(styles.actions)}>
        <Button
          label="Reset"
          size="sm"
          isDisabled={values.size === 0}
          onClick={() => {
            setValues(new Map());
            void artifacts.resetTweaks();
          }}
        />
        <Button
          label={sending ? "Sending…" : `Send to ${send?.to ?? "the employee"}`}
          variant="primary"
          size="sm"
          isDisabled={send === null || changed.length === 0 || sending}
          onClick={() => void post(tweaksMessage(path, version, changed))}
        />
        {send === null ? <p {...stylex.props(styles.quiet)}>{NO_SEND}</p> : null}
      </div>
    </aside>
  );
}

type Tool = "comment" | "tweaks";

export function ArtifactViewer({
  versions,
  openId,
  artifacts,
  messages = [],
  conversationId = null,
  send = null,
  onClose,
}: {
  /** Newest first, as `artifactVersions` gives them. */
  versions: ArtifactVersion[];
  openId: string;
  artifacts: Artifacts;
  /** The conversation's messages: the answer that kept a version names its path. */
  messages?: readonly MessageView[];
  conversationId?: string | null;
  /** Where comments, tweaks and the PowerPoint request go; null: nowhere (no Send). */
  send?: ArtifactSend | null;
  onClose: () => void;
}) {
  const [id, setId] = useState(openId);
  const [viewport, setViewport] = useState<Viewport>("desktop");
  const [reloads, setReloads] = useState(0);
  const [exporting, setExporting] = useState<"pdf" | "png" | null>(null);
  const [tool, setTool] = useState<Tool | null>(null);
  const [compare, setCompare] = useState(false);
  const [compareId, setCompareId] = useState<string | null>(null);
  const current = versions.find((v) => v.file.id === id) ?? versions[0];
  const file = useArtifactFile(id);
  const url = useServed(artifacts, file.data?.html);
  const { frame, stage, ready } = useArtifactView(artifacts, url, reloads, "main");
  const { post } = usePost(send);
  // Another screen: the viewer closes with it (main takes the view away as well).
  const path = useRouterState({ select: (s) => s.location.href });
  const openedAt = useRef(path);
  useEffect(() => {
    if (path !== openedAt.current) onClose();
  }, [path, onClose]);
  const name = current?.file.name ?? file.data?.name ?? "Artifact";
  const version = current?.version ?? 1;
  const answer = messages.find((m) => m.id === current?.messageId)?.text ?? "";
  const artifactName = artifactPath(answer, name);
  const width = viewportWidth(viewport);
  const now = Date.now();
  // Beside it: the chosen version, else the one before the current (N-1), else any other.
  const others = versions.filter((v) => v.file.id !== current?.file.id);
  const left =
    others.find((v) => v.file.id === compareId) ?? others.find((v) => v.version < version) ?? others[0] ?? null;

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
            description: `${send.to} makes a PowerPoint of v${version}`,
            dividerBefore: true,
          },
        ]
      : []),
  ];
  const exportPick = (key: ExportKey) => {
    if (key === "html") {
      if (file.data) saveBlob(new Blob([file.data.html], { type: "text/html" }), name);
    } else if (key === "pptx") void post(pptxMessage(artifactName, version));
    else if (url !== null && exporting === null) void exportAs(key);
  };
  const chooseTool = (next: Tool) => {
    setTool((now) => (now === next ? null : next));
    setCompare(false);
  };

  return (
    <FormDialog
      title={name}
      width={1340}
      onClose={onClose}
      footer={
        <>
          <span {...stylex.props(styles.footerStart)}>
            <Button
              label="Reload"
              onClick={() => {
                setReloads((n) => n + 1);
                void file.refetch();
              }}
            />
          </span>
          <Menu
            label="Export"
            options={exportOptions}
            onPick={exportPick}
            trigger={<Button label={exporting ? "Exporting…" : "Export"} isDisabled={exporting !== null} />}
          />
          <Button label="Close" variant="primary" onClick={onClose} />
        </>
      }
    >
      <div {...stylex.props(styles.bar)}>
        {compare && left ? (
          <Selector
            label="Compare with"
            size="sm"
            value={left.file.id}
            onChange={(v) => setCompareId(v)}
            options={others.map((v) => ({ value: v.file.id, label: versionLabel(v, now) }))}
          />
        ) : null}
        <Selector
          label="Version"
          size="sm"
          value={current?.file.id ?? id}
          onChange={(v) => setId(v)}
          options={versions.map((v) => ({ value: v.file.id, label: versionLabel(v, now) }))}
        />
        <span {...stylex.props(styles.grow)} />
        <ToggleButton
          size="sm"
          label="Comment"
          isPressed={tool === "comment"}
          isDisabled={url === null}
          onPressedChange={() => chooseTool("comment")}
        />
        <ToggleButton
          size="sm"
          label="Tweaks"
          isPressed={tool === "tweaks"}
          isDisabled={url === null}
          onPressedChange={() => chooseTool("tweaks")}
        />
        <ToggleButton
          size="sm"
          label="Compare"
          isPressed={compare}
          isDisabled={others.length === 0}
          tooltip={others.length === 0 ? "There is only one version." : "Show another version beside this one"}
          onPressedChange={() => {
            setCompare((now) => !now);
            setTool(null);
          }}
        />
        <SegmentedControl label="Width" size="sm" value={viewport} onChange={(v) => setViewport(v as Viewport)}>
          {VIEWPORTS.map((v) => (
            <SegmentedControlItem key={v.value} value={v.value} label={v.label} />
          ))}
        </SegmentedControl>
      </div>
      <div {...stylex.props(styles.row)}>
        {compare && left ? (
          <CompareStage
            key={left.file.id}
            artifacts={artifacts}
            id={left.file.id}
            name={`${name} v${left.version}`}
            width={width}
          />
        ) : null}
        <Stage stage={stage} frame={frame} url={url} name={name} width={width} error={file.error?.message ?? null} />
        {tool === "comment" ? (
          <CommentPanel
            key={`${current?.file.id}#${reloads}`}
            artifacts={artifacts}
            ready={ready}
            storeKey={CommentStore.key(conversationId, name, version)}
            version={version}
            path={artifactName}
            send={send}
          />
        ) : tool === "tweaks" ? (
          <TweaksPanel
            key={`${current?.file.id}#${reloads}`}
            artifacts={artifacts}
            ready={ready}
            version={version}
            path={artifactName}
            send={send}
          />
        ) : null}
      </div>
    </FormDialog>
  );
}
