import { expectBody } from "@aurelius/client";
import type { Setting2 } from "@aurelius/protocol";
import { type FormEvent, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { usePlatform } from "../platform";
import { initial, Mark, markColor } from "../ui/kit";
import {
  ConfirmButton,
  Group,
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
  const add = async (event: FormEvent) => {
    event.preventDefault();
    const ok = await run(() =>
      connection.write("user.create", { username: username.trim(), password }, { retry: false }),
    );
    setPassword("");
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
            <ConfirmButton
              label="Disable"
              question={`Disable ${u.username}?`}
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
          <SmallButton type="submit" disabled={busy}>
            Add board member
          </SmallButton>
        </form>
        <Problem error={users.error ?? error} />
      </div>
    </Group>
  );
}

function DefaultModel() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const list = useSetting(
    "connections",
    async () => expectBody(await connection.call("connection.list"), "connections").value,
  );
  const defaults = list.data?.defaults.find((d) => d.purpose === "employee" && d.scope.kind === "global");
  const chosen = defaults?.settings.connection;
  const active =
    chosen?.mode === "value" ? list.data?.connections.find((c) => c.id === chosen.value) : list.data?.connections[0];
  const model = defaults?.settings.model;
  const current = model?.mode === "value" ? model.value : active?.default_model;
  if (!active || active.models.length === 0) {
    return (
      <Group title="Employee default model">
        <Row title="No connection with models" hint="Add a connection first." />
      </Group>
    );
  }
  const choose = (value: string) => {
    const next: Setting2 = { mode: "value", value };
    return run(() =>
      connection.write(
        "connection.set_defaults",
        {
          purpose: "employee",
          scope: { kind: "global" },
          settings: { ...defaults?.settings, connection: { mode: "value", value: active.id }, model: next },
        },
        { retry: false },
      ),
    );
  };
  return (
    <section className="flex flex-col gap-[10px]">
      <h2 className="m-0 text-[12px] font-medium text-text3">Employee default model</h2>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-2">
        {active.models.map((m) => (
          <button
            key={m}
            type="button"
            aria-pressed={m === current}
            disabled={busy}
            onClick={() => void choose(m)}
            className={`cursor-pointer rounded-[8px] border p-3 text-left font-semibold text-text ${m === current ? "border-acc-text bg-sel shadow-[0_0_0_1px_var(--acc-text)]" : "border-line bg-transparent hover:bg-sel"}`}
          >
            {modelName(m)}
          </button>
        ))}
      </div>
      <Problem error={error} />
    </section>
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
          hint="Runs the full test suite once when the owner reports, in the Testing column. Green goes on to Review; red goes back."
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
            hint="Memory consolidation."
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
      <Problem error={error} />
    </>
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
      <Group title="This server">
        <Row title="Name" hint={name.data ? `The computer is called ${name.data.computer_name}.` : undefined}>
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
        <Row title="Restart the server" hint="Agents at work stop and pick up again afterwards.">
          <ConfirmButton
            label="Restart"
            question="Restart the server now?"
            disabled={busy}
            onConfirm={() => void run(() => connection.write("server.restart", undefined, { retry: false }))}
          />
        </Row>
      </Group>
      <Problem error={name.error ?? health.error ?? error} />
    </>
  );
}

export function RemoteSection() {
  const platform = usePlatform();
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [code, setCode] = useState<{ code: string; expires: number } | null>(null);
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
        setCode({ code: result.value.code, expires: result.value.expires_at_ms });
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
          <Switch
            label="Remote access"
            on={data?.enabled ?? false}
            disabled={busy || !data}
            onChange={(on) =>
              void run(() => connection.write(on ? "remote.enable" : "remote.disable", undefined, { retry: false }))
            }
          />
        </Row>
        {data?.enabled ? (
          <Row
            title="Pair a device"
            hint={code ? `Valid until ${when(code.expires)}.` : "Shows a one-time code to enter on the other device."}
          >
            {code ? <span className="font-mono text-[15px] font-medium tracking-[0.08em]">{code.code}</span> : null}
            <SmallButton disabled={busy} onClick={() => void startPairing()}>
              {code ? "New code" : "Show code"}
            </SmallButton>
          </Row>
        ) : null}
      </Group>
      {data && data.pending.length > 0 ? (
        <Group title="Waiting for your answer">
          {data.pending.map((p) => (
            <Row key={p.pairing_id} title={p.device_name} hint={`${p.platform} · asked ${when(p.requested_at_ms)}`}>
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
              <ConfirmButton
                label="Revoke"
                question={`Revoke ${c.name}?`}
                disabled={busy}
                onConfirm={() =>
                  void run(() => connection.write("remote.revoke", { client_id: c.id }, { retry: false }))
                }
              />
            </Row>
          ))}
        </Group>
      ) : null}
      <Problem error={status.error ?? error} />
      {status.error ? <Muted>Remote access needs the owner of this server.</Muted> : null}
    </>
  );
}
