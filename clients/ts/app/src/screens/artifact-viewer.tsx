// A design artifact, live: the HTML a run kept. It does not run in this page: the desktop lays a
// view of its own over the stage, in a session without network (clients/electron/src/main/
// artifact-session.ts). Its scripts run; it reaches nothing outside it. The page only says where
// the stage is. The view lies above the page, so a toast that falls over the stage is under it.

import { Button } from "@astryxdesign/core/Button";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { expectBody } from "@aurelius/client";
import * as stylex from "@stylexjs/stylex";
import { useRouterState } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { useConnection } from "../connection";
import { useKeyed } from "../data";
import type { Platform } from "../platform";
import { saveBlob } from "../ui/download";
import { toast } from "../ui/toast";
import {
  type ArtifactVersion,
  htmlOf,
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
  stage: {
    display: "flex",
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

/**
 * The open menus and lists of the page: the view, which lies above the page, would cover them.
 * Closed menus stay in the document, hidden, and do not count.
 */
const openMenus = () =>
  [...document.querySelectorAll('[role="listbox"], [role="menu"]')]
    .filter((menu) => menu.checkVisibility())
    .map((menu) => menu.getBoundingClientRect())
    .filter((r) => r.width > 0 && r.height > 0);

/**
 * Shows the served artifact in the desktop's view over `frame`, and keeps the view on it: every
 * animation frame the bounds are measured, and sent when they changed (resize, scroll, the width
 * toggle, the dialog moving). The view goes when the viewer closes, or a new version or reload
 * shows a new one.
 */
function useArtifactView(artifacts: Artifacts, url: string | null, reloads: number) {
  const frame = useRef<HTMLDivElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a reload shows the artifact in a new view.
  useEffect(() => {
    const box = frame.current;
    const around = stage.current;
    if (url === null || box === null || around === null) return;
    const measure = () =>
      stageBounds(
        box.getBoundingClientRect(),
        around.getBoundingClientRect(),
        { width: window.innerWidth, height: window.innerHeight },
        openMenus(),
      );
    let sent = "";
    let shown = false;
    let next = 0;
    const follow = () => {
      const now = measure();
      const key = JSON.stringify(now);
      if (shown && key !== sent) {
        sent = key;
        void artifacts.place(now);
      }
      next = requestAnimationFrame(follow);
    };
    const first = measure();
    sent = JSON.stringify(first);
    artifacts
      .show(url, first)
      .then(() => {
        shown = true;
      })
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This artifact did not open."));
    next = requestAnimationFrame(follow);
    return () => {
      cancelAnimationFrame(next);
      void artifacts.hide();
    };
  }, [artifacts, url, reloads]);
  return { frame, stage };
}

export function ArtifactViewer({
  versions,
  openId,
  artifacts,
  onClose,
}: {
  /** Newest first, as `artifactVersions` gives them. */
  versions: ArtifactVersion[];
  openId: string;
  artifacts: Artifacts;
  onClose: () => void;
}) {
  const connection = useConnection();
  const [id, setId] = useState(openId);
  const [viewport, setViewport] = useState<Viewport>("desktop");
  const [reloads, setReloads] = useState(0);
  const [exporting, setExporting] = useState<"pdf" | "png" | null>(null);
  const current = versions.find((v) => v.file.id === id) ?? versions[0];
  const file = useKeyed(
    ["artifact", id],
    async () => {
      const got = expectBody(await connection.call("file.get", { id }), "file").value;
      return { name: got.name, html: htmlOf(got.data_base64) };
    },
    Boolean(id),
  );
  const url = useServed(artifacts, file.data?.html);
  const { frame, stage } = useArtifactView(artifacts, url, reloads);
  // Another screen: the viewer closes with it (main takes the view away as well).
  const path = useRouterState({ select: (s) => s.location.href });
  const openedAt = useRef(path);
  useEffect(() => {
    if (path !== openedAt.current) onClose();
  }, [path, onClose]);
  const name = current?.file.name ?? file.data?.name ?? "Artifact";
  const width = viewportWidth(viewport);
  const now = Date.now();

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
          <Button
            label="Export HTML"
            isDisabled={!file.data}
            onClick={() => {
              if (file.data) saveBlob(new Blob([file.data.html], { type: "text/html" }), name);
            }}
          />
          <Button
            label={exporting === "pdf" ? "Exporting…" : "Export PDF"}
            isDisabled={url === null || exporting !== null}
            onClick={() => void exportAs("pdf")}
          />
          <Button
            label={exporting === "png" ? "Exporting…" : "Export PNG"}
            isDisabled={url === null || exporting !== null}
            onClick={() => void exportAs("png")}
          />
          <Button label="Close" variant="primary" onClick={onClose} />
        </>
      }
    >
      <div {...stylex.props(styles.bar)}>
        <Selector
          label="Version"
          size="sm"
          value={current?.file.id ?? id}
          onChange={(v) => setId(v)}
          options={versions.map((v) => ({ value: v.file.id, label: versionLabel(v, now) }))}
        />
        <span {...stylex.props(styles.grow)} />
        <SegmentedControl label="Width" size="sm" value={viewport} onChange={(v) => setViewport(v as Viewport)}>
          {VIEWPORTS.map((v) => (
            <SegmentedControlItem key={v.value} value={v.value} label={v.label} />
          ))}
        </SegmentedControl>
      </div>
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
        ) : file.error ? (
          <Hint>{file.error.message}</Hint>
        ) : (
          <Loading />
        )}
      </div>
    </FormDialog>
  );
}
