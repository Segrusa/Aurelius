import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Dialog } from "@astryxdesign/core/Dialog";
import { LayoutFooter } from "@astryxdesign/core/Layout";
import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { TextArea } from "@astryxdesign/core/TextArea";
import { TextInput } from "@astryxdesign/core/TextInput";
import { ToggleButton, ToggleButtonGroup } from "@astryxdesign/core/ToggleButton";
import { expectBody, ProtocolError } from "@aurelius/client";
import type { Receipt, TicketPriority } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEmployees, useMembers, useProjects } from "../data";
import { showChord } from "../keys/keymap";
import { isMac, useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { AttachRow, useAttachments } from "../ui/attach";
import { Chip } from "../ui/chip";
import { DialogHead } from "../ui/dialog-head";
import { KeyHint } from "../ui/key-hint";
import { initial, Mark, markColor } from "../ui/marks";
import { Menu } from "../ui/menu";
import { toast } from "../ui/toast";
import { useWorkspace } from "../workspace";
import {
  createdTicketId,
  createdTicketPlace,
  cycle,
  defaultAssignee,
  EFFORTS,
  giveWorkParams,
  LAST_GIVEN,
  MODES,
  parseDeadline,
  parseLabels,
  rootsOf,
  taskRun,
  ticketCreateParams,
  titleOfGoal,
} from "./dialog-logic";
import { overlays, useOpenOverlay } from "./overlays-store";
import { LAST_QUICK, QUICK_NOTE, quickAssignee, quickCreateParams, quickEnabled, quickProblem } from "./quick-logic";
import { modelName, useSetting } from "./settings-kit";
import { NoConnectionNote, useNoConnection } from "./tickets-kit";

const PRIORITIES: { key: TicketPriority; label: string }[] = [
  { key: "low", label: "Low" },
  { key: "normal", label: "Normal" },
  { key: "high", label: "High" },
  { key: "urgent", label: "Urgent" },
];

const styles = stylex.create({
  body: {
    display: "flex",
    minHeight: 0,
    flexDirection: "column",
    gap: 12,
    overflowY: "auto",
    padding: 16,
  },
  giveBody: { gap: 16 },
  chips: { display: "flex", flexWrap: "wrap", gap: 6 },
  twoColumns: { display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 },
  none: { fontSize: 12, color: "var(--precision-text3)" },
  caption: { fontSize: 12, fontWeight: 500, color: "var(--color-text-secondary)" },
  choices: { display: "flex", flexDirection: "column", gap: 6 },
  pick: { flexWrap: "wrap" },
  empty: { color: "var(--precision-text3)" },
  footer: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: 8,
    paddingInline: 16,
    paddingBlock: 12,
  },
  footerNote: { marginInlineEnd: "auto", fontSize: 12, color: "var(--precision-text3)" },
  switchDialog: {
    flexShrink: 0,
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    fontSize: 12,
    color: "var(--color-text-accent)",
  },
});

/** The Menu next to the pressed buttons reads as one of them when it holds the choice. */
const PICKED = {
  borderColor: "var(--color-text-accent)",
  backgroundColor: "var(--color-overlay-hover)",
  color: "var(--color-text-primary)",
};

/** Astryx's Dialog as both windows use it. */
function FormDialog({
  open,
  onClose,
  title,
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
}) {
  return (
    <Dialog
      isOpen={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      aria-label={title}
      width={640}
      maxHeight="calc(100vh - 48px)"
      padding={0}
      purpose="form"
    >
      {children}
    </Dialog>
  );
}

export function useModels() {
  const connection = useConnection();
  const list = useSetting(
    "connections",
    async () => expectBody(await connection.call("connection.list"), "connections").value,
  );
  return useMemo(() => {
    const models = [...new Set((list.data?.connections ?? []).flatMap((c) => c.models))];
    return [{ key: "default", label: "Default" }, ...models.map((m) => ({ key: m, label: modelName(m) }))];
  }, [list.data]);
}

export { parseDeadline, parseLabels };

/** Sends one write. A text that holds a vault value comes back held, to send again on purpose. */
export function useSend(onDone: (text: string) => void) {
  const client = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [held, setHeld] = useState<string[] | null>(null);
  const last = useRef<(confirmSecret: boolean) => Promise<string>>(null);
  const attempt = async (confirmSecret: boolean) => {
    if (!last.current) return;
    setBusy(true);
    setError(null);
    setHeld(null);
    try {
      const done = await last.current(confirmSecret);
      await client.invalidateQueries({ queryKey: ["tickets"] });
      await client.invalidateQueries({ queryKey: ["tasks"] });
      onDone(done);
    } catch (e) {
      if (e instanceof ProtocolError && e.code === "looks_like_secret") setHeld(e.matchedItems ?? []);
      else setError(e instanceof Error ? e.message : "This did not work.");
    } finally {
      setBusy(false);
    }
  };
  return {
    busy,
    error,
    setError,
    held,
    send: (write: (confirmSecret: boolean) => Promise<string>) => {
      last.current = write;
      void attempt(false);
    },
    sendAnyway: () => void attempt(true),
  };
}

/**
 * Remembers the ticket a create made and opens it on its chat (its channel in Chat) once the dialog is done,
 * so the board member lands where the work is discussed. Without ticket channels, or when the ticket
 * cannot be read, it opens the ticket on the Board instead.
 */
export function useOpenCreated() {
  const navigate = useNavigate();
  const connection = useConnection();
  const { features } = useLinkState();
  const client = useQueryClient();
  const created = useRef<string | null>(null);
  return {
    remember: (receipt: Receipt) => {
      created.current = createdTicketId(receipt);
    },
    open: () => {
      const ticket = created.current;
      created.current = null;
      if (!ticket) return;
      const go = (conversation: string | null) => void navigate(createdTicketPlace(ticket, conversation));
      if (!features.includes("ticket_channels")) return go(null);
      connection
        .call("ticket.get", { ticket_id: ticket })
        .then(async (body) => {
          const conversation = expectBody(body, "ticket").value.ticket.conversation_id ?? null;
          // Chat sends a channel it does not know back to general, and the cached list predates this one.
          if (conversation) await client.invalidateQueries({ queryKey: ["messages", "channels"], refetchType: "all" });
          go(conversation);
        })
        .catch(() => go(null));
    },
  };
}

export function Held({ names, onSend, busy }: { names: string[]; onSend: () => void; busy: boolean }) {
  return (
    <Banner
      status="warning"
      role="alert"
      title={`This text holds the value of ${names.length > 0 ? names.join(", ") : "a vault item"}. Anyone who reads it sees that value.`}
      endContent={<Button label="Send anyway" isDisabled={busy} onClick={onSend} />}
    />
  );
}

/** The two windows are one choice: start now or plan for later. What was typed goes along. */
function SwitchDialog({ label, onSwitch }: { label: string; onSwitch: () => void }) {
  return (
    <button type="button" onClick={onSwitch} {...stylex.props(styles.switchDialog)}>
      {label}
    </button>
  );
}

function NewTicketBody({ onClose }: { onClose: () => void }) {
  useKeyContext("AureliusDialog", true);
  const noConnection = useNoConnection();
  const connection = useConnection();
  const { user, features } = useLinkState();
  const canHaveResponsible = features.includes("ticket_responsible");
  const projects = useProjects();
  const employees = useEmployees();
  const models = useModels();
  const [seed] = useState(() => overlays.prefill());
  const workspace = useWorkspace();
  const [projectId, setProjectId] = useState<string | null>(seed?.projectId ?? workspace.id);
  const [title, setTitle] = useState(seed?.title ?? "");
  const [description, setDescription] = useState(seed?.description ?? "");
  const [priority, setPriority] = useState<TicketPriority>("normal");
  const [responsible, setResponsible] = useState("me");
  const members = useMembers();
  const [owner, setOwner] = useState("dispatcher");
  const [mode, setMode] = useState("default");
  const [model, setModel] = useState("default");
  const [labels, setLabels] = useState(seed?.labels ?? "");
  const [deadline, setDeadline] = useState("");
  const attach = useAttachments();
  const { storage } = usePlatform();
  const quickOn = quickEnabled(features);
  const [kind, setKind] = useState<"ticket" | "quick">(seed?.quick && quickOn ? "quick" : "ticket");
  const quick = quickOn && kind === "quick";
  const [quickPick, setQuickPick] = useState<string | null>(null);
  const activeEmployees = (employees.data ?? []).filter((e) => e.status === "active");
  const quickWho = quickPick ?? quickAssignee(employees.data ?? [], storage.get(LAST_QUICK));
  const created = useOpenCreated();
  const { busy, error, setError, held, send, sendAnyway } = useSend((text) => {
    toast(text);
    onClose();
    created.open();
  });
  const project = projects.data?.find((p) => p.id === projectId) ?? projects.data?.[0];
  const ownerName = owner === "dispatcher" ? "Dispatcher" : employees.data?.find((e) => e.id === owner)?.name;

  const createQuick = () => {
    if (!project || !quickWho || quickProblem({ description, employeeId: quickWho }) || busy) return;
    send(async (confirmSecret) => {
      const receipt = await connection.write(
        "ticket.create",
        quickCreateParams(
          {
            projectId: project.id,
            title,
            description,
            employeeId: quickWho,
            files: attach.paths,
            model: model === "default" ? null : model,
          },
          confirmSecret,
        ),
        { retry: false },
      );
      created.remember(receipt);
      storage.set(LAST_QUICK, quickWho);
      const name = employees.data?.find((e) => e.id === quickWho)?.name ?? "the employee";
      return `Quick task started with ${name}.`;
    });
  };

  const create = (start: boolean) => {
    if (quick) return createQuick();
    if (!project || title.trim() === "" || busy) return;
    let deadlineMs: number | null;
    try {
      deadlineMs = parseDeadline(deadline, -new Date().getTimezoneOffset());
    } catch (e) {
      setError((e as Error).message);
      return;
    }
    const responsibleUserId = !canHaveResponsible
      ? null
      : responsible === "me"
        ? (user?.id ?? null)
        : responsible === "none"
          ? null
          : responsible;
    send(async (confirmSecret) => {
      const receipt = await connection.write(
        "ticket.create",
        ticketCreateParams(
          {
            projectId: project.id,
            title,
            description,
            priority,
            labels: parseLabels(labels),
            deadlineMs,
            files: attach.paths,
            start,
            run: taskRun(mode, model),
            responsibleUserId,
            ownerEmployeeId: owner === "dispatcher" ? null : owner,
            fromMessage: seed?.fromMessage,
            fromIdea: seed?.fromIdea,
          },
          confirmSecret,
        ),
        { retry: false },
      );
      created.remember(receipt);
      return start ? "Ticket created and started." : "Ticket created in Backlog.";
    });
  };
  useAction("ConfirmPrimary", () => create(true));
  const ready = quick
    ? Boolean(project) && quickProblem({ description, employeeId: quickWho }) === null
    : Boolean(project) && title.trim() !== "";
  const quickName = activeEmployees.find((e) => e.id === quickWho)?.name;

  return (
    <>
      <DialogHead
        title={quick ? "New quick task" : "New ticket"}
        onOpenChange={(next) => {
          if (!next) onClose();
        }}
      />
      <div {...attach.target} {...stylex.props(styles.body)}>
        {quickOn ? (
          <SegmentedControl
            label="Kind"
            size="sm"
            value={kind}
            onChange={(next) => setKind(next === "quick" ? "quick" : "ticket")}
          >
            <SegmentedControlItem value="ticket" label="Ticket" />
            <SegmentedControlItem value="quick" label="Quick task" />
          </SegmentedControl>
        ) : null}
        {quick ? (
          <>
            <TextArea
              label="Description"
              hasAutoFocus
              placeholder="What needs doing? Required, in Markdown"
              rows={5}
              value={description}
              onChange={setDescription}
            />
            <TextInput
              label="Title (optional)"
              placeholder="Empty: the first line of the description"
              value={title}
              onChange={setTitle}
            />
          </>
        ) : (
          <>
            <TextInput label="Title" hasAutoFocus placeholder="What should happen?" value={title} onChange={setTitle} />
            <TextArea
              label="Description"
              placeholder="Details, in Markdown"
              rows={5}
              value={description}
              onChange={setDescription}
            />
          </>
        )}
        {quick ? (
          <div {...stylex.props(styles.chips)}>
            {project ? (
              <Menu
                label="Project"
                value={project.id}
                options={(projects.data ?? []).map((p) => ({ key: p.id, label: p.name }))}
                onPick={setProjectId}
                trigger={
                  <Chip name="Project">
                    <Mark letter={initial(project.name)} color={markColor(project.id)} size={14} radius={3} />
                    {project.name}
                  </Chip>
                }
              />
            ) : (
              <span {...stylex.props(styles.none)}>No project yet</span>
            )}
            <Menu
              label="Employee"
              {...(quickWho ? { value: quickWho } : {})}
              options={activeEmployees.map((e) => ({ key: e.id, label: e.name, description: e.title ?? e.role }))}
              onPick={setQuickPick}
              trigger={
                <Chip name="Employee" style={quickName ? undefined : PICKED}>
                  {quickName ?? "Choose who does it"}
                </Chip>
              }
            />
            <Menu
              label="Model"
              value={model}
              options={models}
              onPick={setModel}
              trigger={<Chip name="Model">{models.find((m) => m.key === model)?.label ?? "Default"}</Chip>}
            />
          </div>
        ) : null}
        {quick ? null : (
          <>
            <div {...stylex.props(styles.chips)}>
              {project ? (
                <Menu
                  label="Project"
                  value={project.id}
                  options={(projects.data ?? []).map((p) => ({ key: p.id, label: p.name }))}
                  onPick={setProjectId}
                  trigger={
                    <Chip name="Project">
                      <Mark letter={initial(project.name)} color={markColor(project.id)} size={14} radius={3} />
                      {project.name}
                    </Chip>
                  }
                />
              ) : (
                <span {...stylex.props(styles.none)}>No project yet</span>
              )}
              <Menu
                label="Priority"
                value={priority}
                options={PRIORITIES}
                onPick={setPriority}
                trigger={<Chip name="Priority">{PRIORITIES.find((p) => p.key === priority)?.label}</Chip>}
              />
              {canHaveResponsible ? (
                <Menu
                  label="Responsible"
                  value={responsible}
                  options={[
                    { key: "me", label: "You" },
                    ...(members.data ?? [])
                      .filter((u) => !u.disabled_at_ms && u.id !== user?.id)
                      .map((u) => ({ key: u.id, label: u.username })),
                    { key: "none", label: "No one" },
                  ]}
                  onPick={setResponsible}
                  trigger={
                    <Chip name="Responsible">
                      {responsible === "me"
                        ? "You"
                        : responsible === "none"
                          ? "No one"
                          : (members.data?.find((u) => u.id === responsible)?.username ?? "No one")}
                    </Chip>
                  }
                />
              ) : null}
              <Menu
                label="Owner"
                value={owner}
                options={[
                  { key: "dispatcher", label: "Dispatcher" },
                  ...(employees.data ?? []).map((e) => ({ key: e.id, label: e.name })),
                ]}
                onPick={setOwner}
                trigger={<Chip name="Owner">{ownerName}</Chip>}
              />
              <Menu
                label="Mode"
                value={mode}
                options={MODES.map((m) =>
                  m.key === "default"
                    ? { key: m.key as string, label: "Owner's default" }
                    : { key: m.key as string, label: m.label },
                )}
                onPick={setMode}
                trigger={
                  <Chip name="Mode">
                    {mode === "default" ? "Owner's default" : MODES.find((m) => m.key === mode)?.label}
                  </Chip>
                }
              />
              <Menu
                label="Model"
                value={model}
                options={models}
                onPick={setModel}
                trigger={<Chip name="Model">{models.find((m) => m.key === model)?.label ?? "Default"}</Chip>}
              />
            </div>
            <div {...stylex.props(styles.twoColumns)}>
              <TextInput label="Labels" size="sm" placeholder="backend, ui" value={labels} onChange={setLabels} />
              <TextInput
                label="Deadline"
                size="sm"
                placeholder="2026-11-04 or 2026-11-04 17:00"
                value={deadline}
                onChange={setDeadline}
              />
            </div>
          </>
        )}
        <AttachRow attach={attach} />
        {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
        {error ? <Banner status="error" role="alert" title={error} /> : null}
      </div>
      <LayoutFooter hasDivider padding={0}>
        <div {...stylex.props(styles.footer)}>
          <span {...stylex.props(styles.footerNote)}>
            {noConnection ? (
              <NoConnectionNote />
            ) : quick ? (
              QUICK_NOTE
            ) : (
              "Lands in Backlog. Nothing starts until you press Start."
            )}
          </span>
          {quick ? (
            <Button label="Create quick task" variant="primary" isDisabled={!ready || busy} onClick={createQuick}>
              Create quick task <KeyHint onAccent>{showChord("secondary-enter", isMac)}</KeyHint>
            </Button>
          ) : (
            <>
              <Button label="Create" isDisabled={!ready || busy} onClick={() => create(false)} />
              <Button
                label="Create and start"
                variant="primary"
                isDisabled={!ready || busy}
                onClick={() => create(true)}
              >
                Create and start <KeyHint onAccent>{showChord("secondary-enter", isMac)}</KeyHint>
              </Button>
            </>
          )}
        </div>
      </LayoutFooter>
    </>
  );
}

const CYCLE_MODES = MODES.map((m) => m.key as string);

function GiveWorkBody({ onClose }: { onClose: () => void }) {
  useKeyContext("GiveWork", true);
  const noConnection = useNoConnection();
  const connection = useConnection();
  const { storage } = usePlatform();
  const { user, features } = useLinkState();
  const projects = useProjects();
  const employees = useEmployees();
  const models = useModels();
  const [goal, setGoal] = useState(() => overlays.prefill()?.description ?? "");
  const attach = useAttachments();
  const [effort, setEffort] = useState("default");
  const [picked, setPicked] = useState<string | null>(() => overlays.prefill()?.employeeId ?? null);
  const workspace = useWorkspace();
  const [projectId, setProjectId] = useState<string | null>(workspace.id);
  const [mode, setMode] = useState("default");
  const [model, setModel] = useState("default");
  const quickOn = quickEnabled(features);
  const [kind, setKind] = useState<"work" | "quick">("work");
  const quick = quickOn && kind === "quick";
  const created = useOpenCreated();
  const { busy, error, held, send, sendAnyway } = useSend((text) => {
    toast(text);
    onClose();
    created.open();
  });
  const active = useMemo(() => (employees.data ?? []).filter((e) => e.status === "active"), [employees.data]);
  const roots = useMemo(() => rootsOf(active), [active]);
  const others = active.filter((e) => !roots.includes(e));
  const whoId =
    picked ??
    (quick ? quickAssignee(employees.data ?? [], storage.get(LAST_QUICK)) : null) ??
    defaultAssignee(employees.data ?? [], storage.get(LAST_GIVEN));
  const who = active.find((e) => e.id === whoId) ?? null;
  const shownProjects = projects.data ?? [];
  // Required: work should not land in a project nobody chose. The workspace chose it, or only one leaves no choice.
  const project = shownProjects.length === 1 ? shownProjects[0] : shownProjects.find((p) => p.id === projectId);
  const ready = Boolean(project) && Boolean(who) && goal.trim() !== "";

  const submit = () => {
    if (!project || !who || !ready || busy) return;
    if (quick) {
      send(async (confirmSecret) => {
        const receipt = await connection.write(
          "ticket.create",
          quickCreateParams(
            {
              projectId: project.id,
              title: "",
              description: goal,
              employeeId: who.id,
              files: attach.paths,
              model: model === "default" ? null : model,
              effort: effort === "default" ? null : effort,
            },
            confirmSecret,
          ),
          { retry: false },
        );
        created.remember(receipt);
        storage.set(LAST_QUICK, who.id);
        return `Quick task started with ${who.name}.`;
      });
      return;
    }
    send(async (confirmSecret) => {
      // A task only exists under a ticket: this one is for this employee, started at once.
      const receipt = await connection.write(
        "ticket.create",
        giveWorkParams(
          {
            projectId: project.id,
            goal,
            employeeId: who.id,
            run: taskRun(mode, model, effort),
            files: attach.paths,
            responsibleUserId: features.includes("ticket_responsible") ? (user?.id ?? null) : null,
          },
          confirmSecret,
        ),
        { retry: false },
      );
      created.remember(receipt);
      storage.set(LAST_GIVEN, who.id);
      return `Created and started with ${who.name}.`;
    });
  };
  useAction("SubmitGiveWork", submit);
  useAction("CycleTaskEffort", () => setEffort(cycle(EFFORTS, effort)));
  useAction(
    "CycleTaskManager",
    () =>
      active.length > 0 &&
      setPicked(
        cycle(
          active.map((e) => e.id),
          whoId,
        ),
      ),
  );
  useAction(
    "CycleGiveProject",
    () =>
      shownProjects.length > 0 &&
      setProjectId(
        cycle(
          shownProjects.map((p) => p.id),
          project?.id ?? "",
        ),
      ),
  );
  useAction("CycleTaskMode", () => setMode(cycle(CYCLE_MODES, mode)));
  useAction("CycleTaskModel", () =>
    setModel(
      cycle(
        models.map((m) => m.key),
        model,
      ),
    ),
  );

  const chosenOther = others.find((e) => e.id === whoId);

  return (
    <>
      <DialogHead
        title="Give work"
        subtitle={
          quick
            ? "a quick task: starts right away on its own branch"
            : "starts right away, or with a plan for you to approve"
        }
        endContent={
          <SwitchDialog
            label="New ticket instead"
            onSwitch={() => overlays.open("ticket", { title: titleOfGoal(goal), description: goal.trim() })}
          />
        }
        onOpenChange={(next) => {
          if (!next) onClose();
        }}
      />
      <div {...attach.target} {...stylex.props(styles.body, styles.giveBody)}>
        {quickOn ? (
          <SegmentedControl
            label="Kind"
            size="sm"
            value={kind}
            onChange={(next) => setKind(next === "quick" ? "quick" : "work")}
          >
            <SegmentedControlItem value="work" label="Ticket" />
            <SegmentedControlItem value="quick" label="Quick task" />
          </SegmentedControl>
        ) : null}
        <TextArea
          label="What needs doing"
          hasAutoFocus
          placeholder="What needs doing? For example: compare three hosting options"
          rows={4}
          value={goal}
          onChange={setGoal}
        />
        <div {...stylex.props(styles.choices)}>
          <span {...stylex.props(styles.caption)}>For</span>
          <div {...stylex.props(styles.chips)}>
            {active.length === 0 ? <span {...stylex.props(styles.empty)}>No employee yet.</span> : null}
            {roots.length > 0 ? (
              <ToggleButtonGroup
                label="For"
                type="single"
                size="sm"
                value={whoId ?? null}
                onChange={(next) => {
                  if (typeof next === "string") setPicked(next);
                }}
                xstyle={styles.pick}
              >
                {roots.map((e) => (
                  <ToggleButton
                    key={e.id}
                    value={e.id}
                    label={e.name}
                    icon={<Mark letter={initial(e.name)} color={markColor(e.id)} size={16} radius={4} />}
                  >
                    {e.name}
                  </ToggleButton>
                ))}
              </ToggleButtonGroup>
            ) : null}
            {others.length > 0 ? (
              <Menu
                label="Someone else"
                {...(whoId ? { value: whoId } : {})}
                options={others.map((e) => ({ key: e.id, label: e.name, description: e.title ?? e.role }))}
                onPick={setPicked}
                trigger={
                  <Chip style={chosenOther ? PICKED : undefined}>{chosenOther ? chosenOther.name : "Anyone…"}</Chip>
                }
              />
            ) : null}
          </div>
        </div>
        <div {...stylex.props(styles.choices)}>
          <span {...stylex.props(styles.caption)}>Project · its memory and guidelines come along</span>
          <div {...stylex.props(styles.chips)}>
            {shownProjects.length === 0 ? (
              <span {...stylex.props(styles.empty)}>Create a project first: work always belongs to one.</span>
            ) : (
              <ToggleButtonGroup
                label="Project"
                type="single"
                size="sm"
                value={project?.id ?? null}
                onChange={(next) => {
                  if (typeof next === "string") setProjectId(next);
                }}
                xstyle={styles.pick}
              >
                {shownProjects.map((p) => (
                  <ToggleButton
                    key={p.id}
                    value={p.id}
                    label={p.name}
                    icon={<Mark letter={initial(p.name)} color={markColor(p.id)} size={16} radius={4} />}
                  >
                    {p.name}
                  </ToggleButton>
                ))}
              </ToggleButtonGroup>
            )}
          </div>
        </div>
        <div {...stylex.props(styles.chips)}>
          <Menu
            label="Model"
            value={model}
            options={models}
            onPick={setModel}
            trigger={<Chip name="Model">{models.find((m) => m.key === model)?.label ?? "Default"}</Chip>}
          />
          <Menu
            label="Effort"
            options={EFFORTS.map((e) => ({ key: e, label: e === "default" ? "Default" : e }))}
            onPick={setEffort}
            trigger={<Chip name="Effort">{effort === "default" ? "Default" : effort}</Chip>}
          />
          {quick ? null : (
            <Menu
              label="Mode"
              value={mode}
              options={MODES.map((m) => ({
                key: m.key as string,
                label: m.key === "default" ? "Manager chooses" : m.label,
              }))}
              onPick={setMode}
              trigger={
                <Chip name="Mode">
                  {mode === "default" ? "Manager chooses" : MODES.find((m) => m.key === mode)?.label}
                </Chip>
              }
            />
          )}
        </div>
        <AttachRow attach={attach} />
        {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
        {error ? <Banner status="error" role="alert" title={error} /> : null}
      </div>
      <LayoutFooter hasDivider padding={0}>
        <div {...stylex.props(styles.footer)}>
          <span {...stylex.props(styles.footerNote)}>
            {noConnection ? (
              <NoConnectionNote />
            ) : quick ? (
              QUICK_NOTE
            ) : who ? (
              `${who.name} reports back to you when it is done.`
            ) : (
              "Choose who gets this work."
            )}
          </span>
          <Button
            label={quick ? "Start quick task" : "Give work"}
            variant="primary"
            isDisabled={!ready || busy}
            onClick={submit}
          >
            {quick ? "Start quick task" : "Give work"} <KeyHint onAccent>{showChord("secondary-enter", isMac)}</KeyHint>
          </Button>
        </div>
      </LayoutFooter>
    </>
  );
}

function useCloseWhenSignedOut(open: boolean, close: () => void) {
  const { user } = useLinkState();
  useEffect(() => {
    if (open && user === null) close();
  }, [open, user, close]);
}

export function NewTicketDialog() {
  const open = useOpenOverlay() === "ticket";
  const { close } = overlays;
  useCloseWhenSignedOut(open, close);
  return (
    <FormDialog open={open} onClose={close} title="New ticket">
      {open ? <NewTicketBody onClose={close} /> : null}
    </FormDialog>
  );
}

export function GiveWorkDialog() {
  const open = useOpenOverlay() === "give";
  const { close } = overlays;
  useCloseWhenSignedOut(open, close);
  return (
    <FormDialog open={open} onClose={close} title="Give work">
      {open ? <GiveWorkBody onClose={close} /> : null}
    </FormDialog>
  );
}
