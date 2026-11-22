// A design artifact, live: the HTML a run kept, in a sandboxed frame that main serves on its own
// scheme (clients/electron/src/main/artifact.ts). Its scripts run; it reaches nothing outside it.

import { Button } from "@astryxdesign/core/Button";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { expectBody } from "@aurelius/client";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useState } from "react";
import { useConnection } from "../connection";
import { useKeyed } from "../data";
import type { Platform } from "../platform";
import { saveBlob } from "../ui/download";
import { toast } from "../ui/toast";
import {
  ARTIFACT_SANDBOX,
  type ArtifactVersion,
  htmlOf,
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
    borderWidth: 0,
    // An artifact paints its own page; without a background of its own it is a white one.
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
      <div {...stylex.props(styles.stage)}>
        {url ? (
          <iframe
            // A new frame for every version and reload: main lets a frame load one artifact only.
            key={`${url}#${reloads}`}
            src={url}
            title={name}
            sandbox={ARTIFACT_SANDBOX}
            referrerPolicy="no-referrer"
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
