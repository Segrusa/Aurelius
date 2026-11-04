import { expectBody } from "@aurelius/client";
import type { PushStatus, Setting2 } from "@aurelius/protocol";
import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/kit";
import { QrCode } from "./qr-code";
import { ServerChoicesPanel } from "./servers";
import { MemberAccess, passwordProblem } from "./settings-accounts";
import {
  ConfirmButton,
  Group,
  Keyed,
  Muted,
  modelName,
  NameEditor,
  Problem,
  Row,
  SmallButton,
  Switch,
  TextInput,
  useChange,
  useSetting,
  when,
} from "./settings-kit";

function BoardMembers() {
  const connection = useConnection();
  const { user } = useLinkState();
  const { busy, error, run } = useChange();
  const users = useSetting("users", async () => expectBody(await connection.call("user.list"), "users").value.users);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [access, setAccess] = useState<{ id: string; username: string } | null>(null);
  const problem = passwordProblem(password, repeat);
  const add = async (event: FormEvent) => {
    event.preventDefault();
    if (problem) return;
    const ok = await run(() =>
      connection.write("user.create", { username: username.trim(), password }, { retry: false }),
    );
    setPassword("");
    setRepeat("");
    if (ok) setUsername("");
  };
  return (
    <Group title="Board members">
      {(users.data ?? []).map((u) => (
        <Row
          key={u.id}
          title={u.username}
          hint={u.disabled_at_ms ? "Disabled" : u.id === user?.id ? "You" : "Signs in with their own Claude login."}
        >
          {u.id === user?.id || u.disabled_at_ms ? null : (
            <SmallButton onClick={() => setAccess(u)}>Sessions</SmallButton>
          )}
          {u.id === user?.id || u.disabled_at_ms ? null : (
            <ConfirmButton
              label="Disable"
              question={`Disable ${u.username}? They are signed out everywhere and can no longer sign in.`}
              disabled={busy}
              onConfirm={() => void run(() => connection.write("user.disable", { user_id: u.id }, { retry: false }))}
            />
          )}
        </Row>
      ))}
      <div className="flex flex-col gap-2 px-4 py-[14px]">
        <form className="flex flex-wrap items-center gap-2" onSubmit={(e) => void add(e)}>
          <TextInput
            label="New board member's name"
            placeholder="Name"
            required
            autoComplete="off"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
          <TextInput
            label="New board member's password"
            placeholder="First password"
            type="password"
            required
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          <TextInput
            label="Repeat the first password"
            placeholder="Repeat password"
            type="password"
            required
            autoComplete="new-password"
            value={repeat}
            onChange={(e) => setRepeat(e.target.value)}
          />
          <SmallButton type="submit" disabled={busy || problem !== null}>
            Add board member
          </SmallButton>
        </form>
        <Muted>{password && problem ? problem : "At least 12 characters."}</Muted>
        <Problem error={users.error ?? error} />
        {access ? <MemberAccess user={access.id} name={access.username} onClose={() => setAccess(null)} /> : null}
      </div>
    </Group>
  );
}

function useDefaults(purpose: "employee" | "subagent") {
  const connection = useConnection();
  const list = useSetting(
    "connections",
    async () => expectBody(await connection.call("connection.list"), "connections").value,
  );
  const defaults = list.data?.defaults.find((d) => d.purpose === purpose && d.scope.kind === "global");
  return { list: list.data, settings: defaults?.settings ?? {} };
}

function DefaultModel() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const { list, settings } = useDefaults("employee");
  const connections = (list?.connections ?? []).filter((c) => c.models.length > 0);
  const chosen = settings.connection;
  const active = chosen?.mode === "value" ? connections.find((c) => c.id === chosen.value) : connections[0];
  const model = settings.model;
  const current = model?.mode === "value" ? model.value : active?.default_model;
  if (connections.length === 0) {
    return (
      <Group title="Employee default model">
        <Row title="No connection with models" hint="Add a connection first." />
      </Group>
    );
  }
  const choose = (connectionId: string, value: string) => {
    const next: Setting2 = { mode: "value", value };
    return run(() =>
      connection.write(
        "connection.set_defaults",
        {
          purpose: "employee",
          scope: { kind: "global" },
          settings: { ...settings, connection: { mode: "value", value: connectionId }, model: next },
        },
        { retry: false },
      ),
    );
  };
  return (
    <section className="flex flex-col gap-[10px]">
      <h2 className="m-0 text-[12px] font-medium text-text3">
        Employee default model{current ? ` · Now: ${modelName(current)}` : ""}
      </h2>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-2">
        {connections.flatMap((c) =>
          c.models.map((m) => {
            const on = c.id === active?.id && m === current;
            return (
              <button
                key={`${c.id}|${m}`}
                type="button"
                aria-pressed={on}
                disabled={busy}
                onClick={() => void choose(c.id, m)}
                className={`cursor-pointer rounded-[8px] border p-3 text-left font-semibold text-text ${on ? "border-acc-text bg-sel shadow-[0_0_0_1px_var(--acc-text)]" : "border-line bg-transparent hover:bg-sel"}`}
              >
                {modelName(m)}
                <span className="block text-[12px] font-normal text-text3">{c.display_name}</span>
              </button>
            );
          }),
        )}
      </div>
      <Problem error={error} />
    </section>
  );
}

const SUB_EFFORTS = [
  ["inherit", "Effort: follows the employee"],
  ["low", "Low"],
  ["medium", "Medium"],
  ["high", "High"],
  ["xhigh", "Extra high"],
  ["max", "Max"],
] as const;

function SubagentDefault() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const { list, settings } = useDefaults("subagent");
  const models = [...new Set((list?.connections ?? []).flatMap((c) => c.models))];
  const pick = (s: Setting2 | undefined) => (s?.mode === "value" ? s.value : "inherit");
  const save = (patch: { model?: string; effort?: string }) => {
    const to = (v: string): Setting2 => (v === "inherit" ? { mode: "inherit" } : { mode: "value", value: v });
    return run(() =>
      connection.write(
        "connection.set_defaults",
        {
          purpose: "subagent",
          scope: { kind: "global" },
          settings: {
            ...settings,
            ...(patch.model !== undefined ? { model: to(patch.model) } : {}),
            ...(patch.effort !== undefined ? { effort: to(patch.effort) } : {}),
          },
        },
        { retry: false },
      ),
    );
  };
  const select = "h-7 max-w-[200px] rounded-[6px] border border-btn-line bg-btn px-2 text-[12px] font-medium text-text";
  return (
    <Group title="Subagents">
      <Row title="Subagents" hint="What the helpers of an employee run on, unless the employee says otherwise.">
        <select
          aria-label="Subagent model"
          className={select}
          disabled={busy}
          value={pick(settings.model)}
          onChange={(e) => void save({ model: e.target.value })}
        >
          <option value="inherit">Inherit</option>
          {models.map((m) => (
            <option key={m} value={m}>
              {modelName(m)}
            </option>
          ))}
        </select>
        <select
          aria-label="Subagent effort"
          className={select}
          disabled={busy}
          value={pick(settings.effort)}
          onChange={(e) => void save({ effort: e.target.value })}
        >
          {SUB_EFFORTS.map(([k, l]) => (
            <option key={k} value={k}>
              {l}
            </option>
          ))}
        </select>
      </Row>
      <Problem error={error} />
    </Group>
  );
}

export function CompanySection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const { user, claudeLogin } = useLinkState();
  const company = useSetting("company", async () => expectBody(await connection.call("company.get"), "company").value);
  const mode = useSetting(
    "management_mode",
    async () => expectBody(await connection.call("management_mode.get_setting"), "management_mode_setting").value,
  );
  const data = company.data;
  const roots = (data?.employees ?? []).filter((e) => data?.roots.includes(e.id));
  const background = data?.background_login;
  const mine = background?.id === user?.id;
  const testerId = data?.company.tester_employee_id ?? "";
  return (
    <>
      <Group>
        <Row title="Company name">
          {data ? (
            <NameEditor
              label="Company name"
              value={data.company.name}
              busy={busy}
              onSave={(name) => void run(() => connection.write("company.update", { name }, { retry: false }))}
            />
          ) : null}
        </Row>
        <Row title="Management chooses the mode">
          <Switch
            label="Management chooses the mode"
            on={mode.data?.enabled ?? false}
            disabled={busy || !mode.data}
            onChange={(enabled) =>
              void run(() => connection.write("management_mode.set_enabled", { enabled }, { retry: false }))
            }
          />
        </Row>
        <Row title="Follow the org chart" hint="Work goes through whom someone reports to.">
          <Switch
            label="Follow the org chart"
            on={data?.follow_org_chart ?? false}
            disabled={busy || !data}
            onChange={(follow) =>
              void run(() => connection.write("company.set_follow_org_chart", { follow }, { retry: false }))
            }
          />
        </Row>
        <Row title="Management team" hint="Employees that report to no one.">
          {roots.length === 0 ? <span className="text-text3">Nobody yet</span> : null}
          {roots.map((e) => (
            <span key={e.id} className="inline-flex items-center gap-[6px]">
              <Mark letter={initial(e.name)} color={markColor(e.id)} />
              {e.name}
            </span>
          ))}
        </Row>
        <Problem error={company.error ?? mode.error ?? null} />
      </Group>
      <BoardMembers />
      <Group title="Testing">
        <Row
          title="Tester"
          hint="Runs the full test suite once when the owner reports, in the Testing column. Green goes on to Review; red goes back to Doing with the failures."
        >
          <select
            aria-label="Tester"
            value={testerId}
            disabled={busy || !data}
            onChange={(e) =>
              void run(() =>
                connection.write(
                  "company.update",
                  { tester: { employee_id: e.target.value || null } },
                  { retry: false },
                ),
              )
            }
            className="h-7 max-w-[220px] rounded-[6px] border border-btn-line bg-btn px-2 text-[12px] font-medium text-text"
          >
            <option value="">None: no Testing step</option>
            {(data?.employees ?? []).map((e) => (
              <option key={e.id} value={e.id}>
                {e.name}
              </option>
            ))}
          </select>
        </Row>
      </Group>
      <Group title="Background jobs">
        {background ? (
          <Row
            title={`Background jobs run on ${mine ? "your" : `${background.username}'s`} Claude login.`}
            hint="Memory consolidation runs on this login."
          >
            {mine ? (
              <SmallButton
                onClick={() =>
                  void run(() => connection.write("company.set_background_login", { enabled: false }, { retry: false }))
                }
              >
                Stop using my login
              </SmallButton>
            ) : null}
          </Row>
        ) : (
          <div className="flex items-center gap-4 rounded-[10px] bg-amber-soft px-4 py-[14px] shadow-[inset_0_0_0_1px_var(--amber-line)]">
            <span className="size-[7px] flex-none rounded-full bg-amber" />
            <div className="min-w-0 flex-1">
              No Claude login is chosen: background jobs (memory consolidation) do not run.
            </div>
            <SmallButton
              kind="primary"
              disabled={busy || claudeLogin?.status !== "linked"}
              title={
                claudeLogin?.status === "linked" ? undefined : "Connect your Claude login first (Settings → Account)."
              }
              onClick={() =>
                void run(() => connection.write("company.set_background_login", { enabled: true }, { retry: false }))
              }
            >
              Use my Claude login
            </SmallButton>
          </div>
        )}
      </Group>
      <DefaultModel />
      <SubagentDefault />
      <Problem error={error} />
    </>
  );
}

function ServerUpdate() {
  const connection = useConnection();
  const client = useQueryClient();
  const { busy, error, run } = useChange();
  const status = useSetting(
    "update",
    async () => expectBody(await connection.call("update.get", { check: false }), "update").value,
  );
  const data = status.data;
  const check = () =>
    run(async () => {
      const body = await connection.call("update.get", { check: true });
      client.setQueryData(["settings", "update"], expectBody(body, "update").value);
    });
  if (!data) return <Problem error={status.error} />;
  return (
    <Group title="Server update">
      <Row title="Installed">
        <span className="font-mono text-[12px] text-text2">{data.current}</span>
      </Row>
      <Row title="Latest" hint={data.checked_at_ms ? `Checked ${when(data.checked_at_ms)}` : undefined}>
        <span className="font-mono text-[12px] text-text2">{data.latest ?? "Unknown"}</span>
        <Keyed action="SettingsCheckUpdate" chord="C">
          <SmallButton disabled={busy} onClick={() => void check()}>
            Check now
          </SmallButton>
        </Keyed>
      </Row>
      <Row title="Status" hint={data.error ?? data.reason ?? undefined}>
        <span className="text-[12px] text-text2">
          {!data.enabled
            ? "Updates are off"
            : data.staged
              ? `Version ${data.staged} is ready`
              : data.available
                ? "An update is available"
                : "Up to date"}
        </span>
        {data.enabled && data.available && !data.staged ? (
          <Keyed action="SettingsInstallUpdate" chord="U">
            <SmallButton
              disabled={busy}
              onClick={() =>
                void run(() => connection.write("update.prepare", undefined, { retry: false, timeoutMs: 600_000 }))
              }
            >
              Download update
            </SmallButton>
          </Keyed>
        ) : null}
        {data.staged ? (
          <Keyed action="SettingsInstallUpdate" chord="U">
            <ConfirmButton
              label="Update server"
              question="Answers that are running now are interrupted. Update now?"
              disabled={busy}
              onConfirm={() => void run(() => connection.write("update.apply", undefined, { retry: false }))}
            />
          </Keyed>
        ) : null}
      </Row>
      {data.notes ? <Row title="What's new" hint={<span className="whitespace-pre-wrap">{data.notes}</span>} /> : null}
      <Problem error={error} />
    </Group>
  );
}

function Power() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const power = useSetting("power", async () => expectBody(await connection.call("power.get"), "power").value);
  const pending = power.data?.pending;
  const asked = (when: "now" | "when_idle") =>
    run(() => connection.write("power.shutdown", { when }, { retry: false }));
  return (
    <Row
      title="Shut down this PC"
      hint={
        pending
          ? pending.when === "now"
            ? `The PC powers off ${pending.deadline_ms ? `at ${when(pending.deadline_ms)}` : "shortly"}.`
            : "The PC powers off when all work is done."
          : "The server is unreachable until someone starts the PC again."
      }
    >
      {pending ? (
        <SmallButton
          kind="primary"
          disabled={busy}
          onClick={() => void run(() => connection.write("power.cancel", undefined, { retry: false }))}
        >
          Cancel shutdown
        </SmallButton>
      ) : (
        <>
          <Keyed action="SettingsShutdownNow" chord="Q">
            <ConfirmButton
              label="Shut down now"
              question="Powers off after a 60 second countdown; running answers are lost."
              disabled={busy}
              onConfirm={() => void asked("now")}
            />
          </Keyed>
          <Keyed action="SettingsShutdownIdle" chord="G">
            <SmallButton disabled={busy} onClick={() => void asked("when_idle")}>
              When all work is done
            </SmallButton>
          </Keyed>
        </>
      )}
      <Problem error={power.error ?? error} />
    </Row>
  );
}

function OtherServers() {
  const { servers } = usePlatform();
  if (servers === null) return null;
  return (
    <Group title="Servers">
      <div className="p-3">
        <ServerChoicesPanel />
      </div>
    </Group>
  );
}

export function ServerSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const { environment, hostId, remote } = useLinkState();
  const name = useSetting(
    "server_name",
    async () => expectBody(await connection.call("server.get_name"), "server_name").value,
  );
  const health = useSetting(
    "health",
    async () => expectBody(await connection.call("server.get_health"), "health").value,
  );
  return (
    <>
      <OtherServers />
      <Group title="This server">
        <Row
          title="Name"
          hint={name.data ? `Default: ${name.data.computer_name}. Leave it empty to use that again.` : undefined}
        >
          {name.data ? (
            <NameEditor
              label="Server name"
              value={name.data.name}
              busy={busy}
              onSave={(next) => void run(() => connection.write("server.set_name", { name: next }, { retry: false }))}
            />
          ) : null}
        </Row>
        <Row title="Connected through" hint={remote ? "Remote access" : "This computer"}>
          <span className="font-mono text-[12px] text-text2">{environment}</span>
        </Row>
        <Row title="Running since" hint={hostId ?? undefined}>
          <span className="text-[12px] text-text2">{when(health.data?.started_at_ms)}</span>
        </Row>
        <Row title="Busy now">
          <span className="text-[12px] text-text2">
            {health.data?.active_jobs ?? 0} job{health.data?.active_jobs === 1 ? "" : "s"}
          </span>
        </Row>
      </Group>
      <ServerUpdate />
      <Group title="Power">
        <Row
          title="Restart server"
          hint="Answers that are running now are interrupted; agents pick up again afterwards."
        >
          <Keyed action="SettingsRestartServer" chord="E">
            <ConfirmButton
              label="Restart"
              question="Restart the server now?"
              disabled={busy}
              onConfirm={() => void run(() => connection.write("server.restart", undefined, { retry: false }))}
            />
          </Keyed>
        </Row>
        <Power />
      </Group>
      <Problem error={name.error ?? health.error ?? error} />
    </>
  );
}

function PushKey({ clients, status }: { clients: { id: string; name: string }[]; status: PushStatus }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [form, setForm] = useState({
    team_id: status.team_id ?? "",
    key_id: status.key_id ?? "",
    bundle_id: status.bundle_id ?? "",
  });
  const [keyFile, setKeyFile] = useState<string | null>(null);
  const [phone, setPhone] = useState("");
  const [sent, setSent] = useState(false);
  const save = () =>
    run(() =>
      connection.write("push.configure", { ...form, ...(keyFile ? { key_p8: keyFile } : {}) }, { retry: false }),
    ).then((ok) => {
      // The private key stays only as long as it takes to send it.
      if (ok) setKeyFile(null);
    });
  return (
    <Group title="Phone notifications">
      <Row
        title={status.apns ? "Notifications to phones are on" : "Notifications to phones are off"}
        hint={status.last_error ?? "Needs an Apple APNs key (.p8)."}
      >
        <TextInput
          label="Team id"
          placeholder="Team id"
          className="w-[110px]"
          value={form.team_id}
          onChange={(e) => setForm({ ...form, team_id: e.target.value })}
        />
        <TextInput
          label="Key id"
          placeholder="Key id"
          className="w-[110px]"
          value={form.key_id}
          onChange={(e) => setForm({ ...form, key_id: e.target.value })}
        />
        <TextInput
          label="Bundle id"
          placeholder="Bundle id"
          className="w-[150px]"
          value={form.bundle_id}
          onChange={(e) => setForm({ ...form, bundle_id: e.target.value })}
        />
      </Row>
      <Row
        title="APNs key (.p8)"
        hint={
          keyFile ? "Chosen. It is sent when you save." : status.apns ? "Leave it to keep the current key." : undefined
        }
      >
        <input
          aria-label="APNs key file"
          type="file"
          accept=".p8"
          className="max-w-[220px] text-[12px] text-text2"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) void file.text().then(setKeyFile);
          }}
        />
        <SmallButton
          kind="primary"
          disabled={busy || !form.team_id || !form.key_id || !form.bundle_id}
          onClick={() => void save()}
        >
          Save
        </SmallButton>
      </Row>
      {status.apns && clients.length > 0 ? (
        <Row title="Test notification" hint={sent ? "Sent. It should show on that phone." : undefined}>
          <select
            aria-label="Phone"
            className="h-7 max-w-[200px] rounded-[6px] border border-btn-line bg-btn px-2 text-[12px] text-text"
            value={phone}
            onChange={(e) => setPhone(e.target.value)}
          >
            <option value="">Choose a device</option>
            {clients.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
          <Keyed action="SettingsPushTest" chord="T">
            <SmallButton
              disabled={busy || !phone}
              onClick={() =>
                void run(() => connection.write("push.test", { client_id: phone }, { retry: false })).then(setSent)
              }
            >
              Send test
            </SmallButton>
          </Keyed>
        </Row>
      ) : null}
      <Problem error={error} />
    </Group>
  );
}

export function RemoteSection() {
  const platform = usePlatform();
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [code, setCode] = useState<{ code: string; expires: number; text: string; qr: string } | null>(null);
  const status = useSetting(
    "remote",
    async () => expectBody(await connection.call("remote.get_status"), "remote").value,
  );
  const data = status.data;
  if (platform.kind === "web") {
    return (
      <Group>
        <Row
          title="Remote access is managed on the server's computer."
          hint="A browser can never allow other devices."
        />
      </Group>
    );
  }
  const startPairing = () =>
    run(async () => {
      const receipt = await connection.write("remote.start_pairing", undefined, { retry: false });
      const { result } = receipt;
      if (result.kind === "host" && result.value.type === "pairing_code")
        setCode({
          code: result.value.code,
          expires: result.value.expires_at_ms,
          text: result.value.pairing_text,
          qr: result.value.qr,
        });
    });
  return (
    <>
      <Group>
        <Row
          title="Remote access"
          hint={
            data?.tailscale.dns_name ??
            data?.tailscale.error ??
            (data?.tailscale.installed === false
              ? "Tailscale is not installed."
              : "Through Tailscale, on your own devices only.")
          }
        >
          <Keyed action="SettingsRemoteToggle" chord="O">
            <Switch
              label="Remote access"
              on={data?.enabled ?? false}
              disabled={busy || !data}
              onChange={(on) =>
                void run(() => connection.write(on ? "remote.enable" : "remote.disable", undefined, { retry: false }))
              }
            />
          </Keyed>
        </Row>
        {data?.enabled ? (
          <Row
            title="Pair a device"
            hint={code ? `Valid until ${when(code.expires)}.` : "Shows a one-time code to enter on the other device."}
          >
            {code ? <span className="font-mono text-[15px] font-medium tracking-[0.08em]">{code.code}</span> : null}
            <Keyed action="SettingsNew" chord="N">
              <SmallButton disabled={busy} onClick={() => void startPairing()}>
                {code ? "New code" : "Show code"}
              </SmallButton>
            </Keyed>
          </Row>
        ) : null}
        {data?.enabled && code ? (
          <Row
            title="On a phone: scan this"
            hint={code.text ? "Another computer: copy the pairing text and paste it there." : undefined}
          >
            {code.text ? (
              <Keyed action="SettingsCopyPairing" chord="Y">
                <SmallButton onClick={() => void platform.copy(code.text)}>Copy pairing text</SmallButton>
              </Keyed>
            ) : null}
            {code.qr ? <QrCode payload={code.qr} label="Pairing code for a phone" /> : null}
          </Row>
        ) : null}
      </Group>
      {data && data.pending.length > 0 ? (
        <Group title="Waiting for your answer">
          {data.pending.map((p) => (
            <Row
              key={p.pairing_id}
              title={p.device_name}
              hint={`${p.platform} · asked ${when(p.requested_at_ms)}${p.owner ? ". It asks for full access: it can do everything this app does here, including API keys, folders on this PC, backups and updates." : ""}`}
            >
              <Keyed action="SettingsUse" chord="Enter">
                <SmallButton
                  kind="primary"
                  disabled={busy}
                  onClick={() =>
                    void run(() =>
                      connection.write(
                        "remote.decide_pairing",
                        { pairing_id: p.pairing_id, approve: true, owner: false },
                        { retry: false },
                      ),
                    )
                  }
                >
                  Allow
                </SmallButton>
              </Keyed>
              {p.owner ? (
                <SmallButton
                  disabled={busy}
                  onClick={() =>
                    void run(() =>
                      connection.write(
                        "remote.decide_pairing",
                        { pairing_id: p.pairing_id, approve: true, owner: true },
                        { retry: false },
                      ),
                    )
                  }
                >
                  Allow as owner
                </SmallButton>
              ) : null}
              <Keyed action="SettingsRefusePairing" chord="X">
                <SmallButton
                  kind="danger"
                  disabled={busy}
                  onClick={() =>
                    void run(() =>
                      connection.write(
                        "remote.decide_pairing",
                        { pairing_id: p.pairing_id, approve: false, owner: false },
                        { retry: false },
                      ),
                    )
                  }
                >
                  Refuse
                </SmallButton>
              </Keyed>
            </Row>
          ))}
        </Group>
      ) : null}
      {data && data.clients.length > 0 ? (
        <Group title="Paired devices">
          {data.clients.map((c) => (
            <Row
              key={c.id}
              title={c.name}
              hint={`${c.platform} · ${c.connected ? "Connected now" : `Last seen ${when(c.last_seen_ms)}`}`}
            >
              <Keyed action="SettingsRemove" chord="Del">
                <ConfirmButton
                  label="Revoke"
                  question={`Revoke ${c.name}?`}
                  disabled={busy}
                  onConfirm={() =>
                    void run(() => connection.write("remote.revoke", { client_id: c.id }, { retry: false }))
                  }
                />
              </Keyed>
            </Row>
          ))}
        </Group>
      ) : null}
      {data ? <PushKey clients={data.clients} status={data.push} /> : null}
      <Problem error={status.error ?? error} />
      {status.error ? <Muted>Remote access needs the owner of this server.</Muted> : null}
    </>
  );
}
