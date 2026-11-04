import { expectBody, ProtocolError } from "@aurelius/client";
import type { AgentMode, TaskRun, TicketPriority } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEmployees, useMembers, useProjects } from "../data";
import { showChord } from "../keys/keymap";
import { isMac, useAction, useKeyContext } from "../keys/provider";
import { usePlatform } from "../platform";
import { AttachRow, useAttachments } from "../ui/attach";
import { Button, Chip, initial, Kbd, Mark, markColor } from "../ui/kit";
import { Dialog, DialogClose, Menu } from "../ui/overlay";
import { toast } from "../ui/toast";
import { cycle, defaultAssignee, parseDeadline, parseLabels, rootsOf, titleOfGoal } from "./dialog-logic";
import { overlays, useOpenOverlay } from "./overlays-store";
import { modelName, useSetting } from "./settings-kit";
import { NoConnectionNote, useNoConnection } from "./tickets-kit";

const PRIORITIES: { key: TicketPriority; label: string }[] = [
  { key: "low", label: "Low" },
  { key: "normal", label: "Normal" },
  { key: "high", label: "High" },
  { key: "urgent", label: "Urgent" },
];

const MODES: { key: AgentMode | "default"; label: string }[] = [
  { key: "default", label: "Default" },
  { key: "ask", label: "Ask" },
  { key: "accept_edits", label: "Accept edits" },
  { key: "auto", label: "Auto" },
  { key: "plan", label: "Plan" },
];

const INPUT =
  "w-full rounded-[6px] border border-btn-line bg-sunk px-3 text-[13px] text-text outline-none focus:border-acc-text placeholder:text-text3";

function Labeled({ label, children }: { label: string; children: ReactNode }) {
  return (
    // biome-ignore lint/a11y/noLabelWithoutControl: the control comes in as children.
    <label className="flex flex-col gap-1 text-[12px] font-medium text-text2">
      <span>{label}</span>
      {children}
    </label>
  );
}

function runOf(mode: string, model: string): TaskRun {
  return { ...(mode === "default" ? {} : { mode: mode as AgentMode }), ...(model === "default" ? {} : { model }) };
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

export function Held({ names, onSend, busy }: { names: string[]; onSend: () => void; busy: boolean }) {
  return (
    <div
      role="alert"
      className="flex items-center gap-3 rounded-[8px] bg-amber-soft px-3 py-2 shadow-[inset_0_0_0_1px_var(--amber-line)]"
    >
      <span className="min-w-0 flex-1">
        This text holds the value of {names.length > 0 ? names.join(", ") : "a vault item"}. Anyone who reads it sees
        that value.
      </span>
      <Button disabled={busy} onClick={onSend}>
        Send anyway
      </Button>
    </div>
  );
}

/** The two windows are one choice: start now or plan for later. What was typed goes along. */
function SwitchDialog({ label, onSwitch }: { label: string; onSwitch: () => void }) {
  return (
    <button
      type="button"
      onClick={onSwitch}
      className="ml-auto flex-none cursor-pointer border-0 bg-transparent p-0 text-[12px] text-acc-text"
    >
      {label}
    </button>
  );
}

function CloseButton() {
  return (
    <DialogClose asChild>
      <button
        type="button"
        aria-label="Close"
        className="ml-auto grid size-6 cursor-pointer place-items-center rounded-[5px] border-0 bg-transparent text-text3 hover:bg-sel hover:text-text"
      >
        ✕
      </button>
    </DialogClose>
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
  const [projectId, setProjectId] = useState<string | null>(seed?.projectId ?? null);
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
  const { busy, error, setError, held, send, sendAnyway } = useSend((text) => {
    toast(text);
    onClose();
  });
  const project = projects.data?.find((p) => p.id === projectId) ?? projects.data?.[0];
  const ownerName = owner === "dispatcher" ? "Dispatcher" : employees.data?.find((e) => e.id === owner)?.name;

  const create = (start: boolean) => {
    if (!project || title.trim() === "" || busy) return;
    let deadlineMs: number | null;
    try {
      deadlineMs = parseDeadline(deadline, -new Date().getTimezoneOffset());
    } catch (e) {
      setError((e as Error).message);
      return;
    }
    send(async (confirmSecret) => {
      await connection.write(
        "ticket.create",
        {
          project_id: project.id,
          title: title.trim(),
          description: description.trim() || null,
          priority,
          labels: parseLabels(labels),
          deadline_ms: deadlineMs,
          ...(attach.paths.length > 0 ? { files: attach.paths } : {}),
          start,
          run: runOf(mode, model),
          confirm_secret: confirmSecret,
          // A server without the feature refuses a field it does not know.
          ...(canHaveResponsible && responsible !== "me" && responsible !== "none"
            ? { responsible_user_id: responsible }
            : {}),
          ...(canHaveResponsible && responsible === "me" && user ? { responsible_user_id: user.id } : {}),
          ...(owner === "dispatcher" ? {} : { owner_employee_id: owner }),
          ...(seed?.fromMessage ? { from_message: seed.fromMessage } : {}),
          ...(seed?.fromIdea ? { from_idea: seed.fromIdea } : {}),
        },
        { retry: false },
      );
      return start ? "Ticket created and started." : "Ticket created in Backlog.";
    });
  };
  useAction("ConfirmPrimary", () => create(true));
  const ready = Boolean(project) && title.trim() !== "";

  return (
    <>
      <div className="flex h-11 flex-none items-center gap-2 border-b border-line2 px-4 text-text3">
        {project ? (
          <Menu
            label="Project"
            value={project.id}
            options={(projects.data ?? []).map((p) => ({ key: p.id, label: p.name }))}
            onPick={setProjectId}
            trigger={
              <button
                type="button"
                className="inline-flex cursor-pointer items-center gap-[6px] border-0 bg-transparent p-0 font-medium text-text2 hover:text-text"
              >
                <Mark letter={initial(project.name)} color={markColor(project.id)} size={16} radius={4} />
                {project.name}
              </button>
            }
          />
        ) : (
          <span>No project yet</span>
        )}
        <span className="text-faint">›</span>
        <span className="font-medium text-text">New ticket</span>
        <SwitchDialog
          label="Give work instead"
          onSwitch={() =>
            overlays.open("give", { title: "", description: [title, description].filter(Boolean).join("\n\n") })
          }
        />
        <CloseButton />
      </div>
      <div {...attach.target} className="flex min-h-0 flex-col gap-3 overflow-y-auto p-4">
        <Labeled label="Title">
          <input
            data-autofocus
            className={`${INPUT} h-9`}
            placeholder="What should happen?"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </Labeled>
        <Labeled label="Description">
          <textarea
            className={`${INPUT} min-h-[120px] resize-y py-2 leading-[1.5]`}
            placeholder="Details, in Markdown"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </Labeled>
        <div className="flex flex-wrap gap-[6px]">
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
        <div className="grid grid-cols-2 gap-3">
          <Labeled label="Labels">
            <input
              className={`${INPUT} h-8`}
              placeholder="backend, ui"
              value={labels}
              onChange={(e) => setLabels(e.target.value)}
            />
          </Labeled>
          <Labeled label="Deadline">
            <input
              className={`${INPUT} h-8`}
              placeholder="2026-11-04 or 2026-11-04 17:00"
              value={deadline}
              onChange={(e) => setDeadline(e.target.value)}
            />
          </Labeled>
        </div>
        <AttachRow attach={attach} />
        {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex flex-none flex-wrap items-center gap-2 border-t border-line2 px-4 py-3">
        {noConnection ? (
          <span className="mr-auto">
            <NoConnectionNote />
          </span>
        ) : null}
        <span className={`${noConnection ? "hidden" : "mr-auto"} text-[12px] text-text3`}>
          Lands in Backlog. Nothing starts until you press Start.
        </span>
        <Button disabled={!ready || busy} onClick={() => create(false)}>
          Create
        </Button>
        <Button kind="primary" disabled={!ready || busy} onClick={() => create(true)}>
          Create and start <Kbd onAccent>{showChord("secondary-enter", isMac)}</Kbd>
        </Button>
      </div>
    </>
  );
}

const CYCLE_MODES = MODES.map((m) => m.key as string);

const LAST_GIVEN = "last_given_to";

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
  const [projectId, setProjectId] = useState<string | null>(null);
  const [mode, setMode] = useState("default");
  const [model, setModel] = useState("default");
  const { busy, error, held, send, sendAnyway } = useSend((text) => {
    toast(text);
    onClose();
  });
  const active = useMemo(() => (employees.data ?? []).filter((e) => e.status === "active"), [employees.data]);
  const roots = useMemo(() => rootsOf(active), [active]);
  const others = active.filter((e) => !roots.includes(e));
  const whoId = picked ?? defaultAssignee(employees.data ?? [], storage.get(LAST_GIVEN));
  const who = active.find((e) => e.id === whoId) ?? null;
  const shownProjects = projects.data ?? [];
  // Required, without a default: work should not land in a project nobody chose. Only one leaves no choice.
  const project = shownProjects.length === 1 ? shownProjects[0] : shownProjects.find((p) => p.id === projectId);
  const ready = Boolean(project) && Boolean(who) && goal.trim() !== "";

  const submit = () => {
    if (!project || !who || !ready || busy) return;
    send(async (confirmSecret) => {
      // A task only exists under a ticket: this one is for this employee, started at once.
      await connection.write(
        "ticket.create",
        {
          project_id: project.id,
          title: titleOfGoal(goal),
          description: goal.trim(),
          owner_employee_id: who.id,
          start: true,
          run: { ...runOf(mode, model), ...(effort === "default" ? {} : { effort }) },
          ...(attach.paths.length > 0 ? { files: attach.paths } : {}),
          confirm_secret: confirmSecret,
          ...(features.includes("ticket_responsible") && user ? { responsible_user_id: user.id } : {}),
        },
        { retry: false },
      );
      storage.set(LAST_GIVEN, who.id);
      return `Created and started with ${who.name}.`;
    });
  };
  useAction("SubmitGiveWork", submit);
  useAction("CycleTaskEffort", () => setEffort(cycle(["default", "low", "medium", "high", "xhigh", "max"], effort)));
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

  const pick =
    "inline-flex h-7 cursor-pointer items-center gap-[6px] rounded-[6px] border px-2 text-[12px] font-medium";
  const on = "border-acc-text bg-sel text-text";
  const off = "border-line bg-transparent text-text2 hover:bg-sel hover:text-text";
  const chosenOther = others.find((e) => e.id === whoId);

  return (
    <>
      <div className="flex h-11 flex-none items-center gap-2 border-b border-line2 px-4">
        <span className="font-medium">Give work</span>
        <span className="truncate text-text3">· starts right away, or with a plan for you to approve</span>
        <SwitchDialog
          label="New ticket instead"
          onSwitch={() => overlays.open("ticket", { title: titleOfGoal(goal), description: goal.trim() })}
        />
        <CloseButton />
      </div>
      <div {...attach.target} className="flex min-h-0 flex-col gap-4 overflow-y-auto p-4">
        <Labeled label="What needs doing">
          <textarea
            data-autofocus
            className={`${INPUT} min-h-[110px] resize-y py-2 leading-[1.5]`}
            placeholder="What needs doing? For example: compare three hosting options"
            value={goal}
            onChange={(e) => setGoal(e.target.value)}
          />
        </Labeled>
        <div className="flex flex-col gap-[6px]">
          <span className="text-[12px] font-medium text-text2">For</span>
          <div className="flex flex-wrap gap-[6px]">
            {active.length === 0 ? <span className="text-text3">No employee yet.</span> : null}
            {roots.map((e) => (
              <button
                key={e.id}
                type="button"
                aria-pressed={e.id === whoId}
                onClick={() => setPicked(e.id)}
                className={`${pick} ${e.id === whoId ? on : off}`}
              >
                <Mark letter={initial(e.name)} color={markColor(e.id)} size={16} radius={4} />
                {e.name}
              </button>
            ))}
            {others.length > 0 ? (
              <Menu
                label="Someone else"
                {...(whoId ? { value: whoId } : {})}
                options={others.map((e) => ({ key: e.id, label: e.name }))}
                onPick={setPicked}
                trigger={
                  <button type="button" className={`${pick} ${chosenOther ? on : off}`}>
                    {chosenOther ? chosenOther.name : "Anyone…"}
                  </button>
                }
              />
            ) : null}
          </div>
        </div>
        <div className="flex flex-col gap-[6px]">
          <span className="text-[12px] font-medium text-text2">Project · its memory and guidelines come along</span>
          <div className="flex flex-wrap gap-[6px]">
            {shownProjects.length === 0 ? (
              <span className="text-text3">Create a project first: work always belongs to one.</span>
            ) : null}
            {shownProjects.map((p) => (
              <button
                key={p.id}
                type="button"
                aria-pressed={p.id === project?.id}
                onClick={() => setProjectId(p.id)}
                className={`${pick} ${p.id === project?.id ? on : off}`}
              >
                <Mark letter={initial(p.name)} color={markColor(p.id)} size={16} radius={4} />
                {p.name}
              </button>
            ))}
          </div>
        </div>
        <div className="flex flex-wrap gap-[6px]">
          <Menu
            label="Model"
            value={model}
            options={models}
            onPick={setModel}
            trigger={<Chip name="Model">{models.find((m) => m.key === model)?.label ?? "Default"}</Chip>}
          />
          <Menu
            label="Effort"
            options={[
              { key: "default", label: "Default" },
              ...["low", "medium", "high", "xhigh", "max"].map((e) => ({ key: e, label: e })),
            ]}
            onPick={setEffort}
            trigger={<Chip name="Effort">{effort === "default" ? "Default" : effort}</Chip>}
          />
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
        </div>
        <AttachRow attach={attach} />
        {held ? <Held names={held} busy={busy} onSend={sendAnyway} /> : null}
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex flex-none items-center gap-2 border-t border-line2 px-4 py-3">
        <span className="mr-auto text-[12px] text-text3">
          {noConnection ? (
            <NoConnectionNote />
          ) : who ? (
            `${who.name} reports back to you when it is done.`
          ) : (
            "Choose who gets this work."
          )}
        </span>
        <Button kind="primary" disabled={!ready || busy} onClick={submit}>
          Give work <Kbd onAccent>{showChord("secondary-enter", isMac)}</Kbd>
        </Button>
      </div>
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
    <Dialog open={open} onClose={close} title="New ticket">
      {open ? <NewTicketBody onClose={close} /> : null}
    </Dialog>
  );
}

export function GiveWorkDialog() {
  const open = useOpenOverlay() === "give";
  const { close } = overlays;
  useCloseWhenSignedOut(open, close);
  return (
    <Dialog open={open} onClose={close} title="Give work">
      {open ? <GiveWorkBody onClose={close} /> : null}
    </Dialog>
  );
}
