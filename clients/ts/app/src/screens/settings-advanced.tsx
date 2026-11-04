import { expectBody } from "@aurelius/client";
import type { LoadLimitsUpdate, McpLogin, McpTransport, ProviderKind, TaskScheduleView } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import { useConnection } from "../connection";
import { useEmployees, useProjects } from "../data";
import { AttachRow, useAttachments } from "../ui/attach";
import { Button, Segments } from "../ui/kit";
import { Dialog } from "../ui/overlay";
import { FileChips } from "./run-tools";
import {
  bytes,
  ConfirmButton,
  Group,
  Muted,
  modelName,
  openWeb,
  Problem,
  Row,
  SmallButton,
  Switch,
  TextInput,
  useChange,
  useSetting,
  when,
} from "./settings-kit";
import { recurrenceForm, recurrenceOf, recurrenceText, WEEKDAYS } from "./settings-logic";

const PROVIDERS: { key: ProviderKind; label: string; about: string }[] = [
  {
    key: "claude_code",
    label: "Claude Code",
    about: "Your own, unmodified Claude Code. You sign in within Claude Code itself; Aurelius never sees your login.",
  },
  { key: "anthropic_api", label: "Anthropic API", about: "Your own API key, pay per use." },
  { key: "openai_api", label: "OpenAI API", about: "Your own API key, pay per use." },
];

function AddConnectionDialog({ onClose }: { onClose: () => void }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [provider, setProvider] = useState<ProviderKind | null>(null);
  const [path, setPath] = useState("");
  const chosen = PROVIDERS.find((p) => p.key === provider);
  useEffect(() => {
    if (provider !== "claude_code") return;
    connection
      .call("connection.detect_runtime")
      .then((body) => {
        const found = expectBody(body, "runtime_detected").value.claude_code;
        if (found) setPath(found);
      })
      .catch(() => undefined);
  }, [provider, connection]);
  const add = () =>
    chosen
      ? run(() =>
          connection.write(
            "connection.add",
            {
              display_name: chosen.label,
              provider: chosen.key,
              ...(chosen.key === "claude_code" && path.trim() ? { runtime_path: path.trim() } : {}),
            },
            { retry: false },
          ),
        ).then((ok) => {
          if (ok) onClose();
        })
      : undefined;
  return (
    <Dialog open onClose={onClose} title="Add connection" width={520}>
      <div className="flex flex-col gap-3 p-4">
        <p className="m-0 text-[15px] font-semibold">Add connection</p>
        <Muted>
          Choose a runtime. Nothing is connected automatically and there is never a silent switch to a paid service.
        </Muted>
        {PROVIDERS.map((p) => (
          <button
            key={p.key}
            type="button"
            aria-pressed={provider === p.key}
            onClick={() => setProvider(p.key)}
            className={`cursor-pointer rounded-[8px] border p-3 text-left text-text ${provider === p.key ? "border-acc-text bg-sel" : "border-line bg-transparent hover:bg-sel"}`}
          >
            <span className="block font-semibold">{p.label}</span>
            <span className="block text-[12px] text-text3">{p.about}</span>
          </button>
        ))}
        {provider === "claude_code" ? (
          <TextInput
            label="Program"
            placeholder="Path to claude"
            className="font-mono"
            value={path}
            onChange={(e) => setPath(e.target.value)}
          />
        ) : null}
        <Problem error={error} />
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={busy || !chosen} onClick={() => void add()}>
          Add connection
        </Button>
      </div>
    </Dialog>
  );
}

function ApiKey({ id, has }: { id: string; has: boolean }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [open, setOpen] = useState(false);
  const [secret, setSecret] = useState("");
  if (!open) return <SmallButton onClick={() => setOpen(true)}>{has ? "Replace API key" : "API key"}</SmallButton>;
  const close = () => {
    setSecret("");
    setOpen(false);
  };
  return (
    <form
      className="flex items-center gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        // Never kept for a reconnect: the parameters hold the key.
        void run(() => connection.write("secret.set", { connection_id: id, secret }, { retry: false })).then((ok) => {
          if (ok) close();
          else setSecret("");
        });
      }}
    >
      <TextInput
        label="API key"
        autoFocus
        type="password"
        autoComplete="off"
        placeholder="API key (Enter saves, Esc cancels)"
        className="w-[240px]"
        value={secret}
        onChange={(e) => setSecret(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            close();
          }
        }}
      />
      <SmallButton type="submit" kind="primary" disabled={busy || secret === ""}>
        Save
      </SmallButton>
      <Problem error={error} />
    </form>
  );
}

export function ConnectionsSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [adding, setAdding] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const list = useSetting(
    "connections",
    async () => expectBody(await connection.call("connection.list"), "connections").value,
  );
  return (
    <>
      <Group title="Connections">
        {list.data?.connections.length === 0 ? <Row title="No connections yet." /> : null}
        {(list.data ?? { connections: [] }).connections.map((c) => {
          const needsKey = c.provider === "anthropic_api" || c.provider === "openai_api";
          return (
            <div key={c.id}>
              <Row
                title={
                  <button
                    type="button"
                    aria-expanded={open === c.id}
                    onClick={() => setOpen(open === c.id ? null : c.id)}
                    className="cursor-pointer border-0 bg-transparent p-0 font-medium text-text"
                  >
                    {open === c.id ? "▾" : "▸"} {c.display_name}
                  </button>
                }
                hint={[
                  c.status,
                  c.default_model ? modelName(c.default_model) : null,
                  `Checked ${when(c.last_verified_ms)}`,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              >
                {needsKey ? <ApiKey id={c.id} has={c.has_secret} /> : null}
                <SmallButton
                  disabled={busy}
                  onClick={() =>
                    void run(() => connection.write("connection.verify", { connection_id: c.id }, { retry: false }))
                  }
                >
                  Check
                </SmallButton>
                <ConfirmButton
                  label="Remove"
                  question={`Remove ${c.display_name}?`}
                  disabled={busy}
                  onConfirm={() =>
                    void run(() => connection.write("connection.remove", { connection_id: c.id }, { retry: false }))
                  }
                />
              </Row>
              {open === c.id ? (
                <dl className="m-0 grid grid-cols-[110px_1fr] gap-x-3 gap-y-1 px-4 pb-[14px] text-[12px]">
                  <dt className="text-text3">Status</dt>
                  <dd className="m-0">
                    {c.status}
                    {c.detail ? ` · ${c.detail}` : ""}
                  </dd>
                  <dt className="text-text3">Billing</dt>
                  <dd className="m-0">{c.billing}</dd>
                  <dt className="text-text3">Sign-in</dt>
                  <dd className="m-0">
                    {needsKey
                      ? c.has_secret
                        ? "API key is in the OS credential store"
                        : "No API key yet"
                      : c.provider === "claude_code"
                        ? "In Claude Code itself (`claude auth login`)"
                        : "Not needed"}
                  </dd>
                  {c.runtime_path ? (
                    <>
                      <dt className="text-text3">Program</dt>
                      <dd className="m-0 font-mono">{c.runtime_path}</dd>
                    </>
                  ) : null}
                  <dt className="text-text3">Available</dt>
                  <dd className="m-0">{c.models.map(modelName).join(", ") || "None"}</dd>
                  <dt className="text-text3">Id</dt>
                  <dd className="m-0 font-mono">{c.id}</dd>
                </dl>
              ) : null}
            </div>
          );
        })}
      </Group>
      <div>
        <SmallButton kind="primary" onClick={() => setAdding(true)}>
          Add connection…
        </SmallButton>
      </div>
      {adding ? <AddConnectionDialog onClose={() => setAdding(false)} /> : null}
      <Problem error={list.error ?? error} />
    </>
  );
}

const MCP_AUTH: Record<string, string> = { authorized: "Connected", needs_login: "Needs login" };

function Browser() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const browser = useSetting("browser", async () => expectBody(await connection.call("browser.get"), "browser").value);
  const data = browser.data;
  const backend = data?.backend ?? "claude_in_chrome";
  return (
    <Group title="Browser">
      <Row
        title="Drive with"
        hint={
          backend === "playwright"
            ? "A separate Aurelius browser window on the server's computer: log in there once and the logins stay. Employees work in parallel, each in its own tab. Needs Node.js."
            : "The extension in your own browser profile, with your logins. Asks before each browser action unless the mode is Auto."
        }
      >
        <Segments
          label="Drive the browser with"
          value={backend}
          options={[
            { key: "claude_in_chrome", label: "Claude in Chrome" },
            { key: "playwright", label: "Playwright" },
          ]}
          onChange={(next) =>
            void run(() => connection.write("browser.set_backend", { backend: next }, { retry: false }))
          }
        />
      </Row>
      {backend === "claude_in_chrome" ? (
        <Row
          title="Browser"
          hint={
            data?.env_override
              ? `AURELIUS_BROWSER on the server forces ${data.env_override}.`
              : data?.choice === "automatic"
                ? "Whichever is running, else the one that has the extension. Used from the next run."
                : "Used from the next run."
          }
        >
          <Segments
            label="Browser"
            value={data?.choice ?? "automatic"}
            options={[
              { key: "automatic", label: "Automatic" },
              { key: "chrome", label: "Chrome" },
              { key: "edge", label: "Edge" },
            ]}
            onChange={(choice) => {
              if (!busy) void run(() => connection.write("browser.set", { choice }, { retry: false }));
            }}
          />
        </Row>
      ) : null}
      <Problem error={browser.error ?? error} />
    </Group>
  );
}

function AddMcp({ onClose }: { onClose: () => void }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [transport, setTransport] = useState<McpTransport>("stdio");
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [url, setUrl] = useState("");
  const [enable, setEnable] = useState(true);
  const local = transport === "stdio";
  const ready = name.trim() !== "" && (local ? command.trim() !== "" : url.trim() !== "");
  const add = () => {
    const [program, ...args] = command.trim().split(/\s+/);
    return run(() =>
      connection.write(
        "mcp.add",
        { name: name.trim(), transport, enable, ...(local ? { command: program ?? "", args } : { url: url.trim() }) },
        { retry: false },
      ),
    ).then((ok) => {
      if (ok) onClose();
    });
  };
  return (
    <Dialog open onClose={onClose} title="Add MCP server" width={520}>
      <div className="flex flex-col gap-3 p-4">
        <p className="m-0 text-[15px] font-semibold">Add MCP server</p>
        <Muted>Adds the server to Claude Code on the server PC (user scope).</Muted>
        <Segments
          label="Transport"
          value={transport}
          options={[
            { key: "stdio", label: "Command" },
            { key: "http", label: "HTTP" },
            { key: "sse", label: "SSE" },
          ]}
          onChange={setTransport}
        />
        <TextInput label="Name" placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} />
        {local ? (
          <TextInput
            label="Command"
            placeholder="Command and arguments"
            className="font-mono"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
          />
        ) : (
          <TextInput
            label="URL"
            placeholder="https://…"
            className="font-mono"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />
        )}
        <label className="flex cursor-pointer items-center gap-2">
          <input
            type="checkbox"
            className="accent-(--acc-text)"
            checked={enable}
            onChange={(e) => setEnable(e.target.checked)}
          />
          Turn on now
        </label>
        <Problem error={error} />
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={busy || !ready} onClick={() => void add()}>
          {busy ? "Adding…" : "Add server"}
        </Button>
      </div>
    </Dialog>
  );
}

function McpLoginDialog({ name, onClose }: { name: string; onClose: () => void }) {
  const connection = useConnection();
  const client = useQueryClient();
  const [login, setLogin] = useState<McpLogin | null>(null);
  const [redirect, setRedirect] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    connection
      .write("mcp.login_start", { name }, { retry: false })
      .then((receipt) => {
        const { result } = receipt;
        if (result.kind === "host" && result.value.type === "mcp_login") {
          setLogin(result.value);
          openWeb(result.value.authorization_url);
        }
      })
      .catch((e: unknown) => setProblem(e instanceof Error ? e.message : "This did not work."));
  }, [connection, name]);
  // It ends in the browser: look for that here.
  const id = login?.login_id;
  useEffect(() => {
    if (!id) return;
    const tick = setInterval(() => {
      connection
        .call("mcp.login_get", { login_id: id })
        .then((body) => {
          const next = expectBody(body, "mcp_login").value;
          setLogin(next);
          if (next.state === "succeeded") {
            void client.invalidateQueries({ queryKey: ["settings"] });
            onClose();
          }
        })
        .catch(() => undefined);
    }, 2000);
    return () => clearInterval(tick);
  }, [id, connection, client, onClose]);
  const finish = () =>
    id
      ? connection
          .write("mcp.login_complete", { login_id: id, redirect_url: redirect.trim() }, { retry: false })
          .catch((e: unknown) => setProblem(e instanceof Error ? e.message : "This did not work."))
      : undefined;
  return (
    <Dialog open onClose={onClose} title={`Log in to ${name}`} width={520}>
      <div className="flex flex-col gap-3 p-4">
        <p className="m-0 text-[15px] font-semibold">Log in to {name}</p>
        {login ? (
          <>
            <Muted>
              Log in in the browser. It then ends on a page that does not load: copy that page's address and paste it
              here.
            </Muted>
            <SmallButton className="self-start" onClick={() => openWeb(login.authorization_url)}>
              Open sign-in page ↗
            </SmallButton>
            <TextInput
              label="Page address"
              placeholder={login.redirect_uri}
              className="font-mono"
              value={redirect}
              onChange={(e) => setRedirect(e.target.value)}
            />
          </>
        ) : !problem ? (
          <p className="m-0 text-text3">Starting the login…</p>
        ) : null}
        <Problem error={problem ?? login?.error} />
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={redirect.trim() === ""} onClick={() => void finish()}>
          Finish
        </Button>
      </div>
    </Dialog>
  );
}

function AlwaysAllowed() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const list = useSetting(
    "always_allow",
    async () => expectBody(await connection.call("mcp_always_allow.list"), "mcp_always_allow").value.tools,
  );
  return (
    <Group title="Always allowed tools">
      <Row title="These MCP tools run without asking, in every project." hint="Plan mode still blocks them." />
      {list.data?.length === 0 ? (
        <Row title="No tools yet." hint="Choose 'Always allow' on an MCP tool's approval to add one." />
      ) : null}
      {(list.data ?? []).map((tool) => (
        <Row key={tool} title={<span className="font-mono text-[12px]">{tool}</span>}>
          <SmallButton
            disabled={busy}
            onClick={() => void run(() => connection.write("mcp_always_allow.remove", { tool }, { retry: false }))}
          >
            Remove
          </SmallButton>
        </Row>
      ))}
      <Problem error={list.error ?? error} />
    </Group>
  );
}

export function McpSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const list = useSetting("mcp", async () => expectBody(await connection.call("mcp.list"), "mcp_servers").value);
  const auto = useSetting(
    "auto_allow",
    async () => expectBody(await connection.call("auto_allow.get_setting"), "auto_allow_setting").value,
  );
  const servers = list.data?.servers ?? [];
  const [adding, setAdding] = useState(false);
  const [login, setLogin] = useState<string | null>(null);
  const setEnabled = (name: string, on: boolean) => {
    const enabled = servers.filter((s) => (s.name === name ? on : s.enabled)).map((s) => s.name);
    return run(() => connection.write("mcp.set", { enabled }, { retry: false }));
  };
  return (
    <>
      <Group title="Tools">
        <Row title="Allow safe tools without asking" hint={auto.data?.tools?.join(", ")}>
          <Switch
            label="Allow safe tools without asking"
            on={auto.data?.enabled ?? false}
            disabled={busy || !auto.data}
            onChange={(enabled) =>
              void run(() => connection.write("auto_allow.set_enabled", { enabled }, { retry: false }))
            }
          />
        </Row>
      </Group>
      <Browser />
      <Group title="MCP servers">
        {servers.length === 0 && (list.data?.missing ?? []).length === 0 ? <Row title="No MCP servers yet." /> : null}
        {(list.data?.missing ?? []).map((name) => (
          <Row key={name} title={name} hint="Claude Code no longer has this server." />
        ))}
        {servers.map((s) => (
          <Row
            key={s.name}
            title={s.name}
            hint={`${s.summary}${MCP_AUTH[s.auth ?? ""] ? ` · ${MCP_AUTH[s.auth ?? ""]}` : ""}`}
          >
            {s.auth === "needs_login" ? <SmallButton onClick={() => setLogin(s.name)}>Log in</SmallButton> : null}
            {s.auth === "authorized" ? (
              <SmallButton
                disabled={busy}
                onClick={() => void run(() => connection.write("mcp.logout", { name: s.name }, { retry: false }))}
              >
                Log out
              </SmallButton>
            ) : null}
            <Switch
              label={`Use ${s.name}`}
              on={s.enabled}
              disabled={busy}
              onChange={(on) => void setEnabled(s.name, on)}
            />
            <ConfirmButton
              label="Remove"
              question={`Remove ${s.name}?`}
              disabled={busy}
              onConfirm={() => void run(() => connection.write("mcp.remove", { name: s.name }, { retry: false }))}
            />
          </Row>
        ))}
      </Group>
      <div>
        <SmallButton kind="primary" onClick={() => setAdding(true)}>
          Add MCP server…
        </SmallButton>
      </div>
      <AlwaysAllowed />
      {adding ? <AddMcp onClose={() => setAdding(false)} /> : null}
      {login ? <McpLoginDialog name={login} onClose={() => setLogin(null)} /> : null}
      <Problem error={list.error ?? auto.error ?? error} />
    </>
  );
}

export function ScheduleDialog({
  schedule,
  projectId,
  onClose,
}: {
  schedule: TaskScheduleView | null;
  projectId?: string;
  onClose: () => void;
}) {
  const connection = useConnection();
  const employees = useEmployees();
  const projects = useProjects();
  const start = recurrenceForm(schedule?.recurrence ?? null);
  const attach = useAttachments(schedule?.attachments.length ?? 0);
  const [dropped, setDropped] = useState<string[]>([]);
  const [title, setTitle] = useState(schedule?.title ?? "");
  const [goal, setGoal] = useState(schedule?.goal ?? "");
  const [criteria, setCriteria] = useState((schedule?.criteria ?? []).join("\n"));
  const [employee, setEmployee] = useState(schedule?.employee_id ?? "");
  const [project, setProject] = useState(schedule?.project_id ?? projectId ?? "");
  const [form, setForm] = useState(start);
  const { busy, error, run } = useChange();
  const [problem, setProblem] = useState<string | null>(null);
  const save = async (confirmSecret = false) => {
    if (title.trim() === "" || goal.trim() === "") return setProblem("Enter a title and what to do.");
    const who = employee || employees.data?.find((e) => e.status === "active")?.id;
    const where = project || projects.data?.find((p) => p.status !== "archived")?.id;
    if (!who) return setProblem("Add an employee first.");
    if (!schedule && !where) return setProblem("Make a project first.");
    const made = recurrenceOf(form);
    if (typeof made === "string") return setProblem(made);
    setProblem(null);
    const fields = {
      title: title.trim(),
      goal,
      employee_id: who,
      recurrence: made,
      acceptance_criteria: criteria
        .split("\n")
        .map((c) => c.trim())
        .filter(Boolean),
      confirm_secret: confirmSecret,
    };
    const ok = await run(() =>
      schedule
        ? connection.write(
            "recurring_task.update",
            { schedule_id: schedule.id, ...fields, add_files: attach.paths, remove_files: dropped },
            { retry: false },
          )
        : connection.write(
            "recurring_task.create",
            { project_id: where as string, ...fields, files: attach.paths },
            { retry: false },
          ),
    );
    if (ok) onClose();
  };
  const label = "flex flex-col gap-1 text-[12px] text-text3";
  const pick = "h-7 rounded-[6px] border border-btn-line bg-btn px-2 text-[12px] text-text";
  const heading = schedule ? "Edit recurring task" : "New recurring task";
  return (
    <Dialog open onClose={onClose} title={heading} width={560}>
      <div className="flex max-h-[70vh] flex-col gap-3 overflow-y-auto p-4">
        <p className="m-0 text-[15px] font-semibold">{heading}</p>
        <Muted>
          Each time it is due, a normal task is made from this for the employee. If the previous one is still open, that
          time is skipped.
        </Muted>
        <div className={label}>
          Title
          <TextInput label="Title" data-autofocus value={title} onChange={(e) => setTitle(e.target.value)} />
        </div>
        <label className={label}>
          What to do
          <textarea
            rows={4}
            className="rounded-[6px] border border-btn-line bg-sunk p-2 text-[13px] text-text"
            value={goal}
            onChange={(e) => setGoal(e.target.value)}
          />
        </label>
        <label className={label}>
          Done when (one per line)
          <textarea
            rows={2}
            className="rounded-[6px] border border-btn-line bg-sunk p-2 text-[13px] text-text"
            value={criteria}
            onChange={(e) => setCriteria(e.target.value)}
          />
        </label>
        <div {...attach.target} className="flex flex-wrap items-center gap-[6px]">
          {(schedule?.attachments ?? [])
            .filter((f) => !dropped.includes(f.path))
            .map((f) => (
              <button
                key={f.path}
                type="button"
                aria-label={`Remove ${f.name}`}
                onClick={() => setDropped([...dropped, f.path])}
                className="cursor-pointer rounded-[6px] border border-line bg-transparent px-2 py-1 text-[12px] text-text"
              >
                {f.name} ×
              </button>
            ))}
          <AttachRow attach={attach} />
        </div>
        <div className="grid grid-cols-2 gap-2">
          <label className={label}>
            Employee
            <select
              className={pick}
              value={employee || (employees.data?.find((e) => e.status === "active")?.id ?? "")}
              onChange={(e) => setEmployee(e.target.value)}
            >
              {(employees.data ?? [])
                .filter((e) => e.status === "active" || e.id === employee)
                .map((e) => (
                  <option key={e.id} value={e.id}>
                    {e.name}
                  </option>
                ))}
            </select>
          </label>
          {schedule ? null : (
            <label className={label}>
              Project
              <select className={pick} value={project} onChange={(e) => setProject(e.target.value)}>
                {(projects.data ?? [])
                  .filter((p) => p.status !== "archived")
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
              </select>
            </label>
          )}
        </div>
        <Segments
          label="When"
          value={form.kind}
          options={[
            { key: "every_hours", label: "Every N hours" },
            { key: "daily", label: "Daily" },
            { key: "weekly", label: "Weekly" },
          ]}
          onChange={(kind) => setForm({ ...form, kind })}
        />
        {form.kind === "every_hours" ? (
          <div className={label}>
            Hours
            <TextInput
              label="Hours"
              type="number"
              min={1}
              className="w-[90px]"
              value={form.hours}
              onChange={(e) => setForm({ ...form, hours: e.target.value })}
            />
          </div>
        ) : (
          <div className={label}>
            Time
            <TextInput
              label="Time"
              placeholder="HH:MM"
              className="w-[90px]"
              value={form.time}
              onChange={(e) => setForm({ ...form, time: e.target.value })}
            />
          </div>
        )}
        {form.kind === "weekly" ? (
          <fieldset className="m-0 flex gap-1 border-0 p-0" aria-label="Weekdays">
            {WEEKDAYS.map((d, i) => {
              const on = form.weekdays.includes(i);
              return (
                <button
                  key={d}
                  type="button"
                  aria-pressed={on}
                  onClick={() =>
                    setForm({ ...form, weekdays: on ? form.weekdays.filter((x) => x !== i) : [...form.weekdays, i] })
                  }
                  className={`h-7 cursor-pointer rounded-[6px] border px-2 text-[12px] ${on ? "border-acc-text bg-sel text-text" : "border-line bg-transparent text-text2"}`}
                >
                  {d}
                </button>
              );
            })}
          </fieldset>
        ) : null}
        <Problem error={problem ?? error} />
      </div>
      <div className="flex justify-end gap-2 border-t border-line2 px-4 py-3">
        <Button onClick={onClose}>Cancel</Button>
        <Button kind="primary" disabled={busy} onClick={() => void save()}>
          {schedule ? "Save" : "Save recurring task"}
        </Button>
      </div>
    </Dialog>
  );
}

export function SchedulesSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [editing, setEditing] = useState<TaskScheduleView | "new" | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const list = useSetting(
    "schedules",
    async () => expectBody(await connection.call("recurring_task.list", {}), "task_schedules").value.schedules,
  );
  return (
    <>
      <Group title="Recurring schedules">
        {list.data?.length === 0 ? <Row title="No recurring tasks yet." /> : null}
        {(list.data ?? []).map((s) => (
          <div key={s.id}>
            <Row
              title={
                <button
                  type="button"
                  aria-expanded={open === s.id}
                  onClick={() => setOpen(open === s.id ? null : s.id)}
                  className="cursor-pointer border-0 bg-transparent p-0 text-left font-medium text-text"
                >
                  {open === s.id ? "▾" : "▸"} {s.title}
                </button>
              }
              hint={`${s.employee_name} · ${recurrenceText(s.recurrence)} · ${s.paused ? "Paused" : `Next ${when(s.next_run_ms)}`} · Last ${when(s.last_run_ms)}`}
            >
              <SmallButton
                disabled={busy}
                onClick={() =>
                  void run(() => connection.write("recurring_task.run_now", { schedule_id: s.id }, { retry: false }))
                }
              >
                Run now
              </SmallButton>
              <SmallButton onClick={() => setEditing(s)}>Edit</SmallButton>
              <ConfirmButton
                label="Delete"
                question="Delete this recurring task? Tasks it already made stay."
                disabled={busy}
                onConfirm={() =>
                  void run(() => connection.write("recurring_task.delete", { schedule_id: s.id }, { retry: false }))
                }
              />
              <Switch
                label={`${s.title} is active`}
                on={!s.paused}
                disabled={busy}
                onChange={(on) =>
                  void run(() =>
                    connection.write("recurring_task.set_paused", { schedule_id: s.id, paused: !on }, { retry: false }),
                  )
                }
              />
            </Row>
            {open === s.id ? (
              <div className="flex flex-col gap-3 px-4 pb-[14px] text-[12px]">
                <div>
                  <p className="m-0 mb-1 font-medium text-text3">Goal</p>
                  <p className="m-0 leading-[1.5] whitespace-pre-wrap">{s.goal}</p>
                </div>
                {s.criteria.length > 0 ? (
                  <div>
                    <p className="m-0 mb-1 font-medium text-text3">Acceptance criteria</p>
                    <ul className="m-0 pl-4">
                      {s.criteria.map((c) => (
                        <li key={c}>{c}</li>
                      ))}
                    </ul>
                  </div>
                ) : null}
                <div>
                  <p className="m-0 mb-1 font-medium text-text3">Each task runs with</p>
                  <p className="m-0">
                    Mode {s.run.mode ?? "default"} · Model {s.run.model ? modelName(s.run.model) : "default"} · Effort{" "}
                    {s.run.effort ?? "default"}
                  </p>
                </div>
                {s.attachments.length > 0 ? (
                  <div>
                    <p className="m-0 mb-1 font-medium text-text3">Attachments</p>
                    <FileChips files={s.attachments} />
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>
        ))}
      </Group>
      <div>
        <SmallButton kind="primary" onClick={() => setEditing("new")}>
          New recurring task
        </SmallButton>
      </div>
      {editing ? (
        <ScheduleDialog schedule={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      ) : null}
      <Problem error={list.error ?? error} />
    </>
  );
}

export function BackupsSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const list = useSetting("backups", async () => expectBody(await connection.call("backup.list"), "backups").value);
  const data = list.data;
  return (
    <>
      <Group>
        <Row title="Back up now" hint={data?.folder}>
          <SmallButton
            disabled={busy}
            onClick={() =>
              void run(() => connection.write("backup.create", undefined, { retry: false, timeoutMs: 600_000 }))
            }
          >
            {busy ? "Working…" : "Back up now"}
          </SmallButton>
        </Row>
        {data?.paused ? (
          <Row
            title="Restored from a backup. Nothing runs (tasks, chats, schedules) until you resume."
            hint={data.paused}
          >
            <SmallButton
              disabled={busy}
              onClick={() => void run(() => connection.write("backup.resume", undefined, { retry: false }))}
            >
              Resume
            </SmallButton>
          </Row>
        ) : null}
        {data?.restore_pending ? <Row title="A restore waits for the next start" hint={data.restore_pending} /> : null}
      </Group>
      <Group title="Backups">
        {data?.backups.length === 0 ? <Row title="No backups yet." /> : null}
        {(data?.backups ?? []).map((b) => (
          <Row
            key={b.id}
            title={when(b.created_at_ms)}
            hint={`${b.kind === "daily" ? "daily" : "manual"} · ${bytes(b.bytes)} · ${b.files} files · version ${b.app_version}`}
          >
            <ConfirmButton
              label="Restore"
              question={`Restore the backup of ${when(b.created_at_ms)}? Aurelius restarts and today's data is replaced.`}
              disabled={busy}
              onConfirm={() => void run(() => connection.write("backup.restore", { id: b.id }, { retry: false }))}
            />
          </Row>
        ))}
      </Group>
      <Problem error={list.error ?? data?.error ?? error} />
    </>
  );
}

const LIMITS: { key: "cpu_high" | "cpu_low" | "mem_min_mb" | "max_runs"; label: string; hint: string }[] = [
  { key: "max_runs", label: "Max runs at once", hint: "At most." },
  { key: "cpu_high", label: "Hold from CPU", hint: "Percent." },
  { key: "cpu_low", label: "Release below CPU", hint: "Percent." },
  { key: "mem_min_mb", label: "Min free memory", hint: "GB." },
];

/** Memory is kept in MB on the wire and written in GB. */
const shownLimit = (key: string, v: number | undefined) =>
  v === undefined ? "" : key === "mem_min_mb" ? String(v / 1024) : String(v);
const sentLimit = (key: string, v: number) => (key === "mem_min_mb" ? Math.round(v * 1024) : v);

/** What the server is doing or about to do; the sidebar's Live only knows tasks. */
function RunningNow() {
  const connection = useConnection();
  const jobs = useSetting("jobs", async () => expectBody(await connection.call("job.list"), "jobs").value.jobs);
  const open = (jobs.data ?? []).filter((j) => j.state === "leased" || j.state === "ready");
  if (open.length === 0) return null;
  return (
    <Group title="Running now">
      {open.map((j) => (
        <Row key={j.id} title={j.label} hint={j.reason ?? j.kind}>
          <span className="text-[12px] text-text2">{j.state === "leased" ? "Running" : "Waiting"}</span>
        </Row>
      ))}
    </Group>
  );
}

export function LoadSection() {
  const connection = useConnection();
  const client = useQueryClient();
  const { busy, error, run } = useChange();
  const load = useSetting("load", async () => expectBody(await connection.call("load.get"), "load").value);
  const [draft, setDraft] = useState<Partial<Record<(typeof LIMITS)[number]["key"], string>>>({});
  useEffect(
    () => connection.loads.on((snapshot) => client.setQueryData(["settings", "load"], snapshot)),
    [connection, client],
  );
  const data = load.data;
  const changes: LoadLimitsUpdate = {};
  for (const { key } of LIMITS) {
    const value = Number(draft[key]);
    if (
      draft[key] !== undefined &&
      draft[key] !== "" &&
      Number.isFinite(value) &&
      sentLimit(key, value) !== data?.limits[key]
    )
      changes[key] = sentLimit(key, value);
  }
  const low = changes.cpu_low ?? data?.limits.cpu_low ?? 0;
  const high = changes.cpu_high ?? data?.limits.cpu_high ?? 100;
  const invalid = low >= high ? "CPU low must be below CPU high." : null;
  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (invalid) return;
    if (await run(() => connection.write("load.set_limits", changes, { retry: false }))) setDraft({});
  };
  return (
    <>
      <RunningNow />
      <Group title="Now">
        <Row
          title={data?.gate === "held" ? "New runs are waiting" : "New runs start right away"}
          hint={data ? `${data.held} waiting · ${data.running} running` : undefined}
        >
          {data ? (
            <span className="text-[12px] text-text2">
              CPU {Math.round(data.cpu_pct)}% · {bytes(data.mem_free_mb * 1024 * 1024)} free of{" "}
              {bytes(data.mem_total_mb * 1024 * 1024)}
            </span>
          ) : null}
        </Row>
      </Group>
      <form onSubmit={(e) => void save(e)} className="contents">
        <Group title="CPU and memory">
          <Row title="Hold runs when the computer is busy">
            <Switch
              label="Hold runs when the computer is busy"
              on={data?.limits.enabled ?? false}
              disabled={busy || !data}
              onChange={(enabled) => void run(() => connection.write("load.set_limits", { enabled }, { retry: false }))}
            />
          </Row>
          {LIMITS.map(({ key, label, hint }) => (
            <Row key={key} title={label} hint={hint}>
              <TextInput
                label={label}
                type="number"
                min={0}
                step="any"
                className="w-[90px] text-right"
                value={draft[key] ?? shownLimit(key, data?.limits[key])}
                onChange={(e) => setDraft((d) => ({ ...d, [key]: e.target.value }))}
              />
            </Row>
          ))}
        </Group>
        {Object.keys(changes).length > 0 ? (
          <div>
            <SmallButton kind="primary" type="submit" disabled={busy || invalid !== null}>
              Save limits
            </SmallButton>
          </div>
        ) : null}
      </form>
      <Problem error={load.error ?? invalid ?? error} />
    </>
  );
}

export function TraceSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [asking, setAsking] = useState(false);
  const trace = useSetting(
    "trace",
    async () => expectBody(await connection.call("trace.get_setting"), "trace_setting").value,
  );
  const set = (enabled: boolean) => run(() => connection.write("trace.set_enabled", { enabled }, { retry: false }));
  return (
    <>
      <Group>
        <Row
          title="Trace mode"
          hint={
            trace.data
              ? `Keeps a trace of every run, for ${trace.data.keep_days} days and at most ${trace.data.keep_runs} runs.`
              : undefined
          }
        >
          {asking ? (
            <>
              <span className="text-[12px] text-text2">Turn trace mode off? Every kept trace is deleted.</span>
              <SmallButton
                kind="danger"
                onClick={() => {
                  setAsking(false);
                  void set(false);
                }}
              >
                Turn off and delete
              </SmallButton>
              <SmallButton onClick={() => setAsking(false)}>Keep on</SmallButton>
            </>
          ) : (
            <Switch
              label="Trace mode"
              on={trace.data?.enabled ?? false}
              disabled={busy || !trace.data}
              onChange={(enabled) => (enabled ? void set(true) : setAsking(true))}
            />
          )}
        </Row>
      </Group>
      <Problem error={trace.error ?? error} />
    </>
  );
}
