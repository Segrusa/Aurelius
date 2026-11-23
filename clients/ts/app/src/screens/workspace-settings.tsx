import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { List } from "@astryxdesign/core/List";
import { Section } from "@astryxdesign/core/Section";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import { ProtocolError } from "@aurelius/client";
import type { ProjectSummary } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useEffect, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useCompany, useEmployees, useMemory } from "../data";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/marks";
import { toast } from "../ui/toast";
import { Confirm, FormDialog, failureOf } from "./org-kit";
import { useFolderChooser } from "./run-tools";
import { Row } from "./settings-kit";
import { archiveConfirmed, descriptionCounter, dispatcherWarning, nameSavable } from "./workspace-settings-logic";

// Selector takes no empty value, so "the company's default dispatcher" needs a word of its own.
const COMPANY_DEFAULT = "company-default";
const CONFLICT = "Someone else changed this workspace while you typed.";

const styles = stylex.create({
  scroll: { minHeight: 0, flex: 1, overflowY: "auto" },
  column: {
    width: "100%",
    maxWidth: 784,
    marginInline: "auto",
    paddingBlockStart: 28,
    paddingBlockEnd: 64,
    paddingInline: { default: 32, "@media (max-width: 640px)": 16 },
  },
  head: { display: "flex", alignItems: "center", gap: 14, marginBlockEnd: 28 },
  title: { margin: 0, fontSize: 20, fontWeight: 650, letterSpacing: "-0.01em" },
  notice: { marginBlockEnd: 20 },
  group: { display: "flex", flexDirection: "column", gap: 4, marginBlockEnd: 32 },
  groupTitle: { margin: 0, fontSize: 15, fontWeight: 600 },
  card: { borderWidth: 1, borderStyle: "solid", borderColor: "var(--color-border)", borderRadius: 8 },
  danger: { borderColor: "color-mix(in srgb, var(--color-text-error) 30%, transparent)" },
  wide: {
    display: "flex",
    flexDirection: "column",
    gap: 8,
    paddingInline: 16,
    paddingBlock: 14,
    borderBlockEndWidth: { default: 1, ":last-child": 0 },
    borderBlockEndStyle: "solid",
    borderBlockEndColor: "var(--precision-line2)",
  },
  label: { fontWeight: 500 },
  line: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: 8 },
  grow: { minWidth: 240, flex: 1 },
  field: { width: 300, maxWidth: "100%" },
  counter: {
    marginInlineEnd: "auto",
    fontSize: 11.5,
    fontVariantNumeric: "tabular-nums",
    color: "var(--precision-text3)",
  },
  bad: { color: "var(--color-text-error)" },
  warn: { margin: 0, fontSize: 12, color: "var(--color-text-warning)" },
  err: { margin: 0, fontSize: 12, color: "var(--color-text-error)" },
  reload: {
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    color: "inherit",
    font: "inherit",
    textDecoration: "underline",
  },
  path: {
    minWidth: 0,
    flex: 1,
    overflow: "hidden",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 6,
    backgroundColor: "var(--color-background-muted, var(--color-overlay-hover))",
    paddingInline: 10,
    paddingBlock: 6,
    fontFamily: "var(--font-family-code)",
    fontSize: 12,
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
  noFolder: { fontFamily: "inherit", color: "var(--precision-text3)" },
  side: { fontSize: 12, fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap", color: "var(--precision-text3)" },
  pill: {
    borderRadius: 10,
    backgroundColor: "var(--color-overlay-hover)",
    paddingInline: 8,
    paddingBlock: 2,
    fontSize: 11,
    fontWeight: 600,
    color: "var(--color-text-secondary)",
  },
  confirmText: { margin: 0, lineHeight: 1.45, color: "var(--color-text-secondary)" },
});

function Group({ title, danger = false, children }: { title: string; danger?: boolean; children: ReactNode }) {
  return (
    <section aria-label={title} {...stylex.props(styles.group)}>
      <Heading level={2} xstyle={styles.groupTitle}>
        {title}
      </Heading>
      <Section variant="transparent" padding={0} xstyle={[styles.card, danger && styles.danger]}>
        <List hasDividers>{children}</List>
      </Section>
    </section>
  );
}

/** A row whose control sits under its label, across the card. */
function Wide({ label, children }: { label: string; children: ReactNode }) {
  return (
    <li data-setting-row tabIndex={-1} {...stylex.props(styles.wide)}>
      <span {...stylex.props(styles.label)}>{label}</span>
      {children}
    </li>
  );
}

/** Writes one setting, reads the workspaces again and tells a version conflict apart. */
function useWrite() {
  const client = useQueryClient();
  const refresh = () =>
    client
      .invalidateQueries({ queryKey: ["employees"] })
      .then(() => client.invalidateQueries({ queryKey: ["projects"] }));
  const write = async (change: () => Promise<unknown>, done: string | null): Promise<"ok" | "conflict" | string> => {
    try {
      await change();
      await refresh();
      if (done) toast(done);
      return "ok";
    } catch (e) {
      if (e instanceof ProtocolError && e.code === "version_conflict") return "conflict";
      return failureOf(e);
    }
  };
  return { write, refresh };
}

function Problem({ text, onReload }: { text: string | null; onReload: () => void }) {
  if (!text) return null;
  if (text !== CONFLICT)
    return (
      <p role="alert" {...stylex.props(styles.err)}>
        {text}
      </p>
    );
  return (
    <p role="alert" {...stylex.props(styles.err)}>
      {CONFLICT}{" "}
      <button type="button" onClick={onReload} {...stylex.props(styles.reload)}>
        Load their version
      </button>{" "}
      and try again.
    </p>
  );
}

function General({ project }: { project: ProjectSummary }) {
  const connection = useConnection();
  const { features } = useLinkState();
  const { write, refresh } = useWrite();
  const savedDescription = project.description ?? "";
  const [name, setName] = useState(project.name);
  const [description, setDescription] = useState(savedDescription);
  const [nameProblem, setNameProblem] = useState<string | null>(null);
  const [descriptionProblem, setDescriptionProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const canDescribe = features.includes("project_description");
  const counter = descriptionCounter(description);
  const descriptionDirty = description.trim() !== savedDescription;
  const reload = (reset: () => void, clear: () => void) => () =>
    void refresh().then(() => {
      reset();
      clear();
      toast("Loaded the latest version.");
    });
  // A save elsewhere (or Load their version) brings the field back to what is stored.
  useEffect(() => setName(project.name), [project.name]);
  useEffect(() => setDescription(savedDescription), [savedDescription]);
  const saveName = async () => {
    if (busy || !nameSavable(name, project.name)) return;
    setBusy(true);
    const result = await write(
      () =>
        connection.write(
          "project.rename",
          { project_id: project.id, name: name.trim() },
          { retry: false, expectedVersion: project.version },
        ),
      "Saved.",
    );
    setBusy(false);
    setNameProblem(result === "ok" ? null : result === "conflict" ? CONFLICT : result);
  };
  const saveDescription = async () => {
    if (busy || counter.tooLong || !descriptionDirty) return;
    setBusy(true);
    const result = await write(
      () =>
        connection.write(
          "project.set_description",
          { project_id: project.id, description: description.trim() },
          { retry: false, expectedVersion: project.version },
        ),
      "Saved.",
    );
    setBusy(false);
    setDescriptionProblem(result === "ok" ? null : result === "conflict" ? CONFLICT : result);
  };
  const setStatus = async (status: "active" | "paused") => {
    if (status === project.status) return;
    const result = await write(
      () => connection.write("project.set_status", { project_id: project.id, status }, { retry: false }),
      status === "paused" ? "Workspace paused." : "Workspace resumed.",
    );
    if (result !== "ok") toast(result === "conflict" ? CONFLICT : result);
  };
  return (
    <Group title="General">
      <Row title="Name">
        <span {...stylex.props(styles.field)}>
          <TextInput
            label="Name"
            isLabelHidden
            size="sm"
            width="100%"
            value={name}
            onChange={setName}
            onKeyDown={(event) => {
              if (event.key === "Enter") void saveName();
            }}
          />
        </span>
        {name.trim() !== project.name ? (
          <Button
            size="sm"
            variant="primary"
            label="Save"
            isDisabled={busy || !nameSavable(name, project.name)}
            onClick={() => void saveName()}
          />
        ) : null}
      </Row>
      {nameProblem ? (
        <li {...stylex.props(styles.wide)}>
          <Problem
            text={nameProblem}
            onReload={reload(
              () => setName(project.name),
              () => setNameProblem(null),
            )}
          />
        </li>
      ) : null}
      {canDescribe ? (
        <Wide label="Description">
          <TextArea label="Description" isLabelHidden rows={3} value={description} onChange={setDescription} />
          <span {...stylex.props(styles.line)}>
            <span aria-live="polite" {...stylex.props(styles.counter, counter.tooLong && styles.bad)}>
              {counter.text}
            </span>
            {descriptionDirty ? (
              <>
                <Button size="sm" variant="ghost" label="Undo" onClick={() => setDescription(savedDescription)} />
                <Button
                  size="sm"
                  variant="primary"
                  label="Save"
                  isDisabled={busy || counter.tooLong}
                  onClick={() => void saveDescription()}
                />
              </>
            ) : null}
          </span>
          <Problem
            text={descriptionProblem}
            onReload={reload(
              () => setDescription(savedDescription),
              () => setDescriptionProblem(null),
            )}
          />
        </Wide>
      ) : (
        <Row title="Description">
          <span {...stylex.props(styles.pill)}>Not on this server</span>
        </Row>
      )}
      <Row title="Status">
        <SegmentedControl
          label="Status"
          size="sm"
          value={project.status === "paused" ? "paused" : "active"}
          onChange={(v) => void setStatus(v as "active" | "paused")}
        >
          <SegmentedControlItem value="active" label="Active" />
          <SegmentedControlItem value="paused" label="Paused" />
        </SegmentedControl>
      </Row>
    </Group>
  );
}

function Folder({
  project,
  changing,
  onChanging,
}: {
  project: ProjectSummary;
  changing: boolean;
  onChanging: (on: boolean) => void;
}) {
  const connection = useConnection();
  const platform = usePlatform();
  const folders = useFolderChooser();
  const { write } = useWrite();
  const choose = () =>
    folders.choose("Choose a project folder", (path) => {
      void write(
        () => connection.write("project.set_folder", { project_id: project.id, path }, { retry: false }),
        "Saved.",
      ).then((result) => {
        if (result !== "ok") toast(result === "conflict" ? CONFLICT : result);
      });
    });
  return (
    <Group title="Folder">
      <Wide label="Project folder">
        <span {...stylex.props(styles.line)}>
          {project.folder ? (
            <>
              <code title={project.folder} {...stylex.props(styles.path)}>
                {project.folder}
              </code>
              {platform.openFolder ? (
                <Button size="sm" label="Open" onClick={() => void platform.openFolder?.(project.folder as string)} />
              ) : null}
              <Button size="sm" label="Change…" onClick={() => onChanging(true)} />
            </>
          ) : (
            <>
              <span {...stylex.props(styles.path, styles.noFolder)}>No folder</span>
              <Button size="sm" variant="primary" label="Choose a folder…" onClick={choose} />
            </>
          )}
        </span>
        {project.folder ? null : (
          <p {...stylex.props(styles.warn)}>◆ Choose a folder before you start a coding ticket.</p>
        )}
      </Wide>
      {changing ? (
        <Confirm
          title="Change the folder?"
          body={`Running sessions keep their current folder. New work in ${project.name} starts in the folder you choose. Nothing is moved or copied.`}
          action="Choose folder…"
          onClose={() => onChanging(false)}
          onConfirm={() => {
            onChanging(false);
            choose();
          }}
        />
      ) : null}
      {folders.dialog}
    </Group>
  );
}

function Work({ project }: { project: ProjectSummary }) {
  const connection = useConnection();
  const employees = useEmployees();
  const company = useCompany();
  const { write } = useWrite();
  const chosen = project.dispatcher_employee_id ?? "";
  const defaultId = company.data?.company.dispatcher_employee_id ?? null;
  const defaultName = employees.data?.find((e) => e.id === defaultId)?.name ?? "none";
  const warning = dispatcherWarning(chosen, defaultId, employees.data ?? []);
  return (
    <Group title="Work">
      <Row title="Dispatcher">
        <span {...stylex.props(styles.field)}>
          <Selector
            label="Dispatcher"
            isLabelHidden
            size="sm"
            width="100%"
            value={chosen || COMPANY_DEFAULT}
            options={[
              { value: COMPANY_DEFAULT, label: `Company default (${defaultName})` },
              ...(employees.data ?? [])
                .filter((e) => e.status === "active" || e.id === chosen)
                .map((e) => ({ value: e.id, label: e.name })),
            ]}
            onChange={(picked) => {
              const id = picked === COMPANY_DEFAULT ? "" : picked;
              const who = employees.data?.find((x) => x.id === id)?.name;
              void write(
                () =>
                  connection.write(
                    "project.update",
                    { project_id: project.id, dispatcher: { employee_id: id || null } },
                    { retry: false },
                  ),
                id
                  ? `${who} chooses the owners of this workspace's tickets.`
                  : "This workspace uses the company's dispatcher again.",
              ).then((result) => {
                if (result !== "ok") toast(result === "conflict" ? CONFLICT : result);
              });
            }}
          />
        </span>
      </Row>
      {warning ? (
        <li {...stylex.props(styles.wide)}>
          <p {...stylex.props(styles.warn)}>{warning}</p>
        </li>
      ) : null}
    </Group>
  );
}

function MemoryAndExport({ project }: { project: ProjectSummary }) {
  const connection = useConnection();
  const platform = usePlatform();
  const navigate = useNavigate();
  const memory = useMemory(project.id);
  const count = memory.data?.length;
  const exportNow = () =>
    void connection
      .write("project.export", { project_id: project.id }, { retry: false })
      .then((receipt) => {
        const made = receipt.result;
        const path = made.kind === "host" && made.value.type === "exported" ? made.value.markdown_path : "";
        toast(`Exported (Markdown + JSON) to ${path.replace(/[/\\][^/\\]*$/, "")}`);
      })
      .catch((e: unknown) => toast(failureOf(e)));
  return (
    <Group title="Memory and export">
      <Row title="Memory">
        {count === undefined ? null : (
          <span {...stylex.props(styles.side)}>
            {count} {count === 1 ? "memory" : "memories"}
          </span>
        )}
        <Button
          size="sm"
          label="Open memory →"
          onClick={() =>
            void navigate({ to: "/knowledge/$tab", params: { tab: "memory" }, search: { project: project.id } })
          }
        />
      </Row>
      <Row title="Export">
        <Button
          size="sm"
          label="Export"
          isDisabled={!project.folder}
          {...(project.folder ? {} : { tooltip: "Choose a folder first" })}
          onClick={exportNow}
        />
      </Row>
      {platform.openProjectWindow && !platform.windowProject ? (
        <Row title="Open in a new window">
          <Button
            size="sm"
            label="Open window"
            onClick={() => void platform.openProjectWindow?.(project.id, project.name)}
          />
        </Row>
      ) : null}
    </Group>
  );
}

function Archive({
  project,
  asking,
  onAsking,
}: {
  project: ProjectSummary;
  asking: boolean;
  onAsking: (on: boolean) => void;
}) {
  const connection = useConnection();
  const navigate = useNavigate();
  const { write } = useWrite();
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const close = () => {
    setTyped("");
    onAsking(false);
  };
  const archive = async () => {
    if (busy || !archiveConfirmed(typed, project.name)) return;
    setBusy(true);
    const result = await write(
      () => connection.write("project.set_status", { project_id: project.id, status: "archived" }, { retry: false }),
      "Workspace archived.",
    );
    setBusy(false);
    if (result !== "ok") return toast(result === "conflict" ? CONFLICT : result);
    close();
    void navigate({ to: "/projects" });
  };
  return (
    <Group title="Archive" danger>
      <Row title="Archive this workspace">
        <Button variant="destructive" label="Archive…" onClick={() => onAsking(true)} />
      </Row>
      {asking ? (
        <FormDialog
          title={`Archive ${project.name}?`}
          width={440}
          onClose={close}
          footer={
            <>
              <Button label="Cancel" onClick={close} />
              <Button
                variant="destructive"
                label="Archive"
                isDisabled={busy || !archiveConfirmed(typed, project.name)}
                onClick={() => void archive()}
              />
            </>
          }
        >
          <p {...stylex.props(styles.confirmText)}>
            It leaves the rail, running sessions stop and recurring tasks no longer run. Nothing is deleted; the history
            is kept.
          </p>
          <p {...stylex.props(styles.confirmText)}>
            Type <b>{project.name}</b> to confirm.
          </p>
          <TextInput
            hasAutoFocus
            label="Type the workspace name to confirm"
            isLabelHidden
            value={typed}
            onChange={setTyped}
            onKeyDown={(event) => {
              if (event.key === "Enter") void archive();
            }}
          />
        </FormDialog>
      ) : null}
    </Group>
  );
}

/** The workspace settings page (ticket #81): one centred column of cards, each field saved on its own. */
export function WorkspaceSettingsBody({
  project,
  changingFolder,
  onChangingFolder,
  archiving,
  onArchiving,
}: {
  project: ProjectSummary;
  changingFolder: boolean;
  onChangingFolder: (on: boolean) => void;
  archiving: boolean;
  onArchiving: (on: boolean) => void;
}) {
  const connection = useConnection();
  const client = useQueryClient();
  const { features } = useLinkState();
  const resume = () =>
    void connection
      .write("project.set_status", { project_id: project.id, status: "active" }, { retry: false })
      .then(() => client.invalidateQueries({ queryKey: ["projects"] }))
      .then(() => toast("Workspace resumed."))
      .catch((e: unknown) => toast(failureOf(e)));
  return (
    <div {...stylex.props(styles.scroll)}>
      <div {...stylex.props(styles.column)}>
        <header {...stylex.props(styles.head)}>
          <Mark letter={initial(project.name)} color={markColor(project.id)} size={40} radius={10} />
          <Heading level={1} xstyle={styles.title}>
            Workspace settings
          </Heading>
        </header>
        {project.status === "paused" ? (
          <div {...stylex.props(styles.notice)}>
            <Banner
              status="warning"
              title="This workspace is paused. No new automatic work starts."
              endContent={<Button size="sm" label="Resume" onClick={resume} />}
            />
          </div>
        ) : null}
        {features.includes("project_description") ? null : (
          <div {...stylex.props(styles.notice)}>
            <Banner status="info" title="This server is older than the app. Some settings are not on it yet." />
          </div>
        )}
        <General project={project} />
        <Folder project={project} changing={changingFolder} onChanging={onChangingFolder} />
        <Work project={project} />
        <MemoryAndExport project={project} />
        <Archive project={project} asking={archiving} onAsking={onArchiving} />
      </div>
    </div>
  );
}
