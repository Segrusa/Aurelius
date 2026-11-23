// The Designs page of a workspace: every design its employees made (`design.list`), the latest
// first, each with its pages. A design opens its conversation with the page beside it, as a
// session does; one without a screen of its own (a project chat) opens beside the list.

import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Link } from "@astryxdesign/core/Link";
import { expectBody } from "@aurelius/client";
import type { DesignView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed, useProjects } from "../data";
import { usePlatform } from "../platform";
import { useWorkspace } from "../workspace";
import { ArtifactViewer } from "./artifact-viewer";
import { designLink, designPlace, pagesOf } from "./designs-logic";
import { failureOf, Hint, Loading, Page, PageHead, Problem } from "./org-kit";

const styles = stylex.create({
  body: { display: "flex", minHeight: 0, flex: 1 },
  scroller: { minWidth: 0, minHeight: 0, flex: 1, overflowY: "auto" },
  list: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
    gap: 12,
    maxWidth: 1100,
    padding: 24,
  },
  card: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    padding: 14,
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
  },
  open: { borderColor: "var(--color-border-emphasized)" },
  title: { margin: 0, fontSize: 13.5, fontWeight: 600, overflowWrap: "anywhere" },
  titleButton: {
    padding: 0,
    borderWidth: 0,
    cursor: "pointer",
    backgroundColor: "transparent",
    font: "inherit",
    color: "inherit",
    textAlign: "start",
    textDecoration: { default: "none", ":hover": "underline" },
  },
  meta: { display: "flex", flexWrap: "wrap", gap: 6, fontSize: 12, color: "var(--precision-text3)" },
  pages: { display: "flex", flexWrap: "wrap", gap: 6 },
  page: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    cursor: "pointer",
    borderRadius: 6,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: { default: "var(--color-border)", ":hover": "var(--color-border-emphasized)" },
    backgroundColor: "transparent",
    paddingInline: 8,
    paddingBlock: 4,
    fontSize: 12.5,
    color: "var(--color-text-primary)",
  },
  accent: { color: "var(--color-text-accent)" },
  pad: { padding: 24 },
});

const when = (ms: number) =>
  new Date(ms).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

/** The designs of a workspace (all of them under All workspaces), the latest first. */
export function useDesigns(projectId: string | null, wanted: boolean) {
  const connection = useConnection();
  return useKeyed(
    ["messages", "designs", projectId],
    async () =>
      expectBody(await connection.call("design.list", projectId ? { project_id: projectId } : {}), "designs").value
        .designs,
    wanted,
  );
}

/** One design: its title, where it was made, when, and its pages. The Overview shows the same card. */
export function DesignCard({
  design,
  project,
  openPage,
  onOpen,
}: {
  design: DesignView;
  project: string | null;
  openPage: string | null;
  onOpen: (page: string, fileId: string) => void;
}) {
  const place = designPlace(design);
  const newest = design.pages.reduce((best, p) => (p.at_ms > best.at_ms ? p : best));
  return (
    <article aria-label={`Design ${design.title}`} {...stylex.props(styles.card, openPage !== null && styles.open)}>
      <h2 {...stylex.props(styles.title)}>
        <button
          type="button"
          onClick={() => onOpen(newest.file.name, newest.file.id)}
          {...stylex.props(styles.titleButton)}
        >
          {design.title}
        </button>
      </h2>
      <div {...stylex.props(styles.meta)}>
        {project ? <span>{project}</span> : null}
        {place.to ? <Link href={place.to}>{place.label}</Link> : <span>{place.label}</span>}
        <span>{when(design.last_at_ms)}</span>
      </div>
      <div {...stylex.props(styles.pages)}>
        {design.pages.map((p) => (
          <button
            key={p.file.id}
            type="button"
            aria-label={`Open the page ${p.file.name} of ${design.title}`}
            aria-pressed={openPage === p.file.name}
            onClick={() => onOpen(p.file.name, p.file.id)}
            {...stylex.props(styles.page)}
          >
            <span {...stylex.props(styles.accent)}>◧</span>
            {p.file.name}
          </button>
        ))}
      </div>
    </article>
  );
}

export function DesignsScreen() {
  const { features } = useLinkState();
  const { artifacts } = usePlatform();
  const supported = features.includes("designs");
  const workspace = useWorkspace().id;
  const designs = useDesigns(workspace, supported);
  const projects = useProjects();
  const navigate = useNavigate();
  const [open, setOpen] = useState<{ conversation: string; page: string } | null>(null);
  const opened = designs.data?.find((d) => d.conversation_id === open?.conversation) ?? null;
  const projectName = (id: string) => (workspace ? null : (projects.data?.find((p) => p.id === id)?.name ?? null));

  return (
    <Page>
      <PageHead title="Designs" />
      <div {...stylex.props(styles.body)}>
        <div {...stylex.props(styles.scroller)}>
          {!supported ? (
            <div {...stylex.props(styles.pad)}>
              <Hint>This server does not list designs yet.</Hint>
            </div>
          ) : designs.isPending ? (
            <div {...stylex.props(styles.pad)}>
              <Loading rows={3} />
            </div>
          ) : designs.error ? (
            <div {...stylex.props(styles.pad)}>
              <Problem>{`The designs did not load: ${failureOf(designs.error)}`}</Problem>
            </div>
          ) : (designs.data ?? []).length === 0 ? (
            <EmptyState
              title="No designs yet."
              description="When an employee makes an HTML design in a session, a ticket or a task, it shows here."
            />
          ) : (
            <div {...stylex.props(styles.list)}>
              {(designs.data ?? []).map((d) => (
                <DesignCard
                  key={d.conversation_id}
                  design={d}
                  project={projectName(d.project_id)}
                  openPage={open?.conversation === d.conversation_id ? open.page : null}
                  onOpen={(page, fileId) => {
                    // A design opens with its conversation, as a session does.
                    const link = designLink(d, fileId);
                    if (link) void navigate({ href: link });
                    else setOpen({ conversation: d.conversation_id, page });
                  }}
                />
              ))}
            </div>
          )}
        </div>
        {opened && open && artifacts ? (
          <ArtifactViewer
            key={`${opened.conversation_id}:${open.page}`}
            pages={pagesOf(opened)}
            openName={open.page}
            artifacts={artifacts}
            conversationId={opened.conversation_id}
            docked
            onClose={() => setOpen(null)}
          />
        ) : null}
      </div>
    </Page>
  );
}
