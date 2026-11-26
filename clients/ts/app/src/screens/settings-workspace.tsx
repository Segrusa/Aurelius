import { Button } from "@astryxdesign/core/Button";
import { FileInput } from "@astryxdesign/core/FileInput";
import { Selector } from "@astryxdesign/core/Selector";
import { ToggleButton } from "@astryxdesign/core/ToggleButton";
import { expectBody } from "@aurelius/client";
import type { PushStatus, Setting2 } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { usePlatform } from "../platform";
import { Icon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/marks";
import { FormDialog } from "./org-kit";
import { ServerChoicesPanel } from "./servers";
import { MemberAccess, passwordProblem } from "./settings-accounts";
import { PairDevice, PendingPairings } from "./settings-devices";
import {
  Chip,
  ConfirmButton,
  day,
  Facts,
  Foot,
  Group,
  GroupTitle,
  IconBox,
  Keyed,
  Muted,
  modelName,
  NameEditor,
  Part,
  Problem,
  Row,
  SmallButton,
  Switch,
  TextInput,
  useChange,
  useSetting,
  when,
} from "./settings-kit";
import { agoText, deviceHint, OWN_DEVICES, remoteAccessDevices, uptimeText } from "./settings-logic";
import { USER_RUN_DEFAULTS_FEATURE } from "./settings-run-defaults";

const styles = stylex.create({
  form: { display: "flex", flexDirection: "column", gap: 12 },
  stack: { display: "flex", flexDirection: "column", gap: 10 },
  models: { display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(150px, 1fr))", gap: 8 },
  model: {
    height: "auto",
    whiteSpace: "normal",
    justifyContent: "flex-start",
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: 8,
    padding: 12,
    textAlign: "left",
    fontWeight: 600,
    color: "var(--color-text-primary)",
  },
  modelOn: {
    borderColor: "var(--color-text-accent)",
    backgroundColor: "var(--color-overlay-hover)",
    boxShadow: "0 0 0 1px var(--color-text-accent)",
  },
  connection: { display: "block", fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  quiet: { color: "var(--precision-text3)" },
  value: { color: "var(--color-text-secondary)" },
  member: { display: "inline-flex", alignItems: "center", gap: 6 },
  tight: { padding: 12, backgroundColor: "transparent" },
  name: { fontSize: 15, fontWeight: 600 },
  mono: { fontFamily: "var(--font-family-code)", fontSize: 12 },
  notes: { margin: 0, whiteSpace: "pre-wrap", color: "var(--color-text-secondary)" },
});

/** The Add board member dialog: a name and a first password, typed twice. */
function AddMemberDialog({ onClose }: { onClose: () => void }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const problem = passwordProblem(password, repeat);
  const ready = username.trim() !== "" && problem === null;
  const add = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!ready) return;
    const ok = await run(() =>
      connection.write("user.create", { username: username.trim(), password }, { retry: false }),
    );
    if (ok) onClose();
  };
  return (
    <FormDialog
      title="Add board member"
      width={400}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" label="Cancel" onClick={onClose} />
          <Button variant="primary" label="Add" isDisabled={busy || !ready} onClick={() => void add()} />
        </>
      }
    >
      <form {...stylex.props(styles.form)} onSubmit={(e) => void add(e)}>
        <TextInput
          label="Name"
          showLabel
          required
          autoFocus
          autoComplete="off"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
        <TextInput
          label="First password"
          showLabel
          placeholder="At least 12 characters"
          type="password"
          required
          autoComplete="new-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <TextInput
          label="Repeat password"
          showLabel
          type="password"
          required
          autoComplete="new-password"
          value={repeat}
          onChange={(e) => setRepeat(e.target.value)}
        />
        {password && problem ? <Muted>{problem}</Muted> : null}
        {/* Enter in a field adds, as the Add button does. */}
        <button type="submit" hidden />
      </form>
      <Problem error={error} />
    </FormDialog>
  );
}

/** Company › Board members: who may sign in, with Add board member in a dialog. */
export function BoardMembersSection() {
  const connection = useConnection();
  const { user } = useLinkState();
  const { busy, error, run } = useChange();
  const users = useSetting("users", async () => expectBody(await connection.call("user.list"), "users").value.users);
  const [adding, setAdding] = useState(false);
  const [access, setAccess] = useState<{ id: string; username: string } | null>(null);
  const list = users.data ?? [];
  return (
    <>
      <Group
        title="Board members"
        count={list.length}
        action={
          <Keyed action="SettingsNew" chord="N">
            <SmallButton kind="primary" onClick={() => setAdding(true)}>
              Add board member
            </SmallButton>
          </Keyed>
        }
      >
        {list.map((u) => {
          const you = u.id === user?.id;
          const off = Boolean(u.disabled_at_ms);
          return (
            <Row
              key={u.id}
              dim={off}
              start={<Mark letter={initial(u.username)} color={markColor(u.id)} size={24} radius={6} />}
              title={
                <>
                  {u.username}
                  {you ? <Chip>You</Chip> : null}
                  {off ? <Chip>Disabled</Chip> : null}
                </>
              }
              hint={off ? `Since ${day(u.disabled_at_ms)}` : `Added ${day(u.created_at_ms)}`}
            >
              {you || off ? null : <SmallButton onClick={() => setAccess(u)}>Sessions</SmallButton>}
              {you || off ? null : (
                <ConfirmButton
                  label="Disable"
                  question={`Disable ${u.username}? They are signed out everywhere and can no longer sign in.`}
                  disabled={busy}
                  onConfirm={() =>
                    void run(() => connection.write("user.disable", { user_id: u.id }, { retry: false }))
                  }
                />
              )}
            </Row>
          );
        })}
        <Problem error={users.error ?? error} />
      </Group>
      {adding ? <AddMemberDialog onClose={() => setAdding(false)} /> : null}
      {access ? <MemberAccess user={access.id} name={access.username} onClose={() => setAccess(null)} /> : null}
    </>
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
    <section {...stylex.props(styles.stack)}>
      <GroupTitle>Employee default model{current ? ` · Now: ${modelName(current)}` : ""}</GroupTitle>
      <div {...stylex.props(styles.models)}>
        {connections.flatMap((c) =>
          c.models.map((m) => {
            const on = c.id === active?.id && m === current;
            return (
              <ToggleButton
                key={`${c.id}|${m}`}
                label={`${modelName(m)}, ${c.display_name}`}
                isPressed={on}
                isDisabled={busy}
                onPressedChange={() => void choose(c.id, m)}
                xstyle={[styles.model, on && styles.modelOn]}
              >
                {modelName(m)}
                <span {...stylex.props(styles.connection)}>{c.display_name}</span>
              </ToggleButton>
            );
          }),
        )}
      </div>
      <Problem error={error} />
    </section>
  );
}

/**
 * The connection employees' runs use. Where each board member keeps their own model, effort and
 * mode, the company only chooses the connection.
 */
export function DefaultConnection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const { list, settings } = useDefaults("employee");
  const connections = list?.connections ?? [];
  const chosen = settings.connection?.mode === "value" ? settings.connection.value : (connections[0]?.id ?? "");
  if (connections.length === 0) {
    return (
      <Group>
        <Row title="Company connection" hint="Add a connection first." />
      </Group>
    );
  }
  const choose = (id: string) =>
    run(() =>
      connection.write(
        "connection.set_defaults",
        {
          purpose: "employee",
          scope: { kind: "global" },
          settings: { ...settings, connection: { mode: "value", value: id } },
        },
        { retry: false },
      ),
    );
  return (
    <Group>
      <Row title="Company connection">
        <Selector
          label="Company connection"
          isLabelHidden
          size="sm"
          variant="secondary"
          width={220}
          isDisabled={busy}
          value={chosen}
          options={connections.map((c) => ({ value: c.id, label: c.display_name }))}
          onChange={(id) => void choose(id)}
        />
      </Row>
      <Problem error={error} />
    </Group>
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
  return (
    <Group title="Subagents">
      <Row title="Subagents" hint="What the helpers of an employee run on, unless the employee says otherwise.">
        <Selector
          label="Subagent model"
          isLabelHidden
          size="sm"
          variant="secondary"
          width={200}
          isDisabled={busy}
          value={pick(settings.model)}
          options={[{ value: "inherit", label: "Inherit" }, ...models.map((m) => ({ value: m, label: modelName(m) }))]}
          onChange={(model) => void save({ model })}
        />
        <Selector
          label="Subagent effort"
          isLabelHidden
          size="sm"
          variant="secondary"
          width={200}
          isDisabled={busy}
          value={pick(settings.effort)}
          options={SUB_EFFORTS.map(([value, label]) => ({ value, label }))}
          onChange={(effort) => void save({ effort })}
        />
      </Row>
      <Problem error={error} />
    </Group>
  );
}

/** Selector has no empty option: this stands for "no tester", never an employee's id. */
const NO_TESTER = "none";

export function CompanySection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const { user, claudeLogin, features } = useLinkState();
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
        <Row title="Management team">
          {roots.length === 0 ? <span {...stylex.props(styles.quiet)}>Nobody yet</span> : null}
          {roots.map((e) => (
            <span key={e.id} title={e.name} {...stylex.props(styles.member)}>
              <Mark letter={initial(e.name)} color={markColor(e.id)} size={24} radius={6} />
              <span {...stylex.props(styles.value)}>{e.name}</span>
            </span>
          ))}
        </Row>
        <Problem error={company.error ?? null} />
      </Group>
      <Group title="How work flows">
        <Row title="Follow the org chart">
          <Switch
            label="Follow the org chart"
            on={data?.follow_org_chart ?? false}
            disabled={busy || !data}
            onChange={(follow) =>
              void run(() => connection.write("company.set_follow_org_chart", { follow }, { retry: false }))
            }
          />
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
        <Row title="Tester">
          <Selector
            label="Tester"
            isLabelHidden
            size="sm"
            variant="secondary"
            width={220}
            value={testerId || NO_TESTER}
            isDisabled={busy || !data}
            options={[
              { value: NO_TESTER, label: "None: no Testing step" },
              ...(data?.employees ?? []).map((e) => ({ value: e.id, label: e.name })),
            ]}
            onChange={(picked) =>
              void run(() =>
                connection.write(
                  "company.update",
                  { tester: { employee_id: picked === NO_TESTER ? null : picked } },
                  { retry: false },
                ),
              )
            }
          />
        </Row>
        <Problem error={mode.error ?? null} />
      </Group>
      <Group title="Background jobs">
        {background ? (
          <Row
            start={
              <IconBox>
                <Icon name="plug" />
              </IconBox>
            }
            title={`Runs on ${mine ? "your" : `${background.username}'s`} Claude login`}
            hint="Memory consolidation"
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
          <Row
            start={
              <IconBox tone="warn">
                <Icon name="alert" />
              </IconBox>
            }
            title="No Claude login"
            hint="Memory consolidation waits for a login"
          >
            <SmallButton
              kind="primary"
              disabled={busy || claudeLogin?.status !== "linked"}
              title={claudeLogin?.status === "linked" ? undefined : "Connect your Claude login first (You › Account)."}
              onClick={() =>
                void run(() => connection.write("company.set_background_login", { enabled: true }, { retry: false }))
              }
            >
              Use my login
            </SmallButton>
          </Row>
        )}
      </Group>
      {features.includes(USER_RUN_DEFAULTS_FEATURE) ? null : (
        <>
          <DefaultModel />
          <SubagentDefault />
        </>
      )}
      <Problem error={error} />
    </>
  );
}

/** The server's version and its update: Check now, What's new, and Update in one row. */
function ServerUpdate() {
  const connection = useConnection();
  const client = useQueryClient();
  const { busy, error, run } = useChange();
  const [notes, setNotes] = useState(false);
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
  // Download and install in one go: the staged version is applied, which restarts the server.
  const update = () =>
    run(async () => {
      if (!data?.staged) await connection.write("update.prepare", undefined, { retry: false, timeoutMs: 600_000 });
      await connection.write("update.apply", undefined, { retry: false });
    });
  if (!data) return <Problem error={status.error} />;
  const ready = data.enabled && (data.available || Boolean(data.staged));
  const title = !data.enabled ? (
    <>
      {data.current}
      <Chip>Updates are off</Chip>
    </>
  ) : ready ? (
    `${data.staged ?? data.latest ?? "A new version"} is available`
  ) : (
    <>
      {data.current}
      <Chip tone="ok">Up to date</Chip>
    </>
  );
  return (
    <Group title="Update">
      <Row
        joined={notes}
        start={
          <IconBox tone={ready ? "info" : "plain"}>
            <Icon name="up" />
          </IconBox>
        }
        title={title}
        hint={
          data.error ??
          data.reason ??
          (data.checked_at_ms ? `Checked ${agoText(data.checked_at_ms, Date.now())}` : undefined)
        }
      >
        {data.notes ? (
          <SmallButton kind="ghost" onClick={() => setNotes(!notes)}>
            What's new
          </SmallButton>
        ) : null}
        {ready ? (
          <Keyed action="SettingsInstallUpdate" chord="U">
            <ConfirmButton
              label="Update"
              kind="primary"
              question="Answers that are running now are interrupted. Update now?"
              disabled={busy}
              onConfirm={() => void update()}
            />
          </Keyed>
        ) : (
          <Keyed action="SettingsCheckUpdate" chord="C">
            <SmallButton disabled={busy} onClick={() => void check()}>
              Check now
            </SmallButton>
          </Keyed>
        )}
      </Row>
      {notes && data.notes ? (
        <Part>
          <p {...stylex.props(styles.notes)}>{data.notes}</p>
        </Part>
      ) : null}
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
          : undefined
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
          <Keyed action="SettingsShutdownIdle" chord="G">
            <SmallButton disabled={busy} onClick={() => void asked("when_idle")}>
              When idle
            </SmallButton>
          </Keyed>
          <Keyed action="SettingsShutdownNow" chord="Q">
            <ConfirmButton
              label="Now"
              question="Powers off after a 60 second countdown; running answers are lost."
              disabled={busy}
              onConfirm={() => void asked("now")}
            />
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
    <Group title="Other servers">
      <Part xstyle={styles.tight}>
        <ServerChoicesPanel />
      </Part>
    </Group>
  );
}

/** Server › Overview: which server this is, its update, its name and power, and the other servers. */
export function ServerSection() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const { remote, hostName } = useLinkState();
  const name = useSetting(
    "server_name",
    async () => expectBody(await connection.call("server.get_name"), "server_name").value,
  );
  const health = useSetting(
    "health",
    async () => expectBody(await connection.call("server.get_health"), "health").value,
  );
  const load = useSetting("load", async () => expectBody(await connection.call("load.get"), "load").value);
  const update = useSetting(
    "update",
    async () => expectBody(await connection.call("update.get", { check: false }), "update").value,
  );
  const { daemonVersion } = useLinkState();
  const busyNow = load.data
    ? `${load.data.running} run${load.data.running === 1 ? "" : "s"} · ${load.data.held} waiting`
    : `${health.data?.active_jobs ?? 0} jobs`;
  return (
    <>
      <Group>
        <Row
          start={
            <IconBox large>
              <Icon name="server" size={18} />
            </IconBox>
          }
          title={<span {...stylex.props(styles.name)}>{name.data?.name || hostName || "Aurelius"}</span>}
          hint={remote ? "Through remote access" : "On this computer"}
        >
          <Chip tone="ok" dot>
            Connected
          </Chip>
        </Row>
        <Facts
          facts={[
            { label: "Version", value: update.data?.current ?? daemonVersion ?? "Unknown" },
            { label: "Running since", value: uptimeText(health.data?.started_at_ms, Date.now()) },
            { label: "Busy now", value: busyNow },
          ]}
        />
      </Group>
      <ServerUpdate />
      <Group title="This server">
        <Row
          title="Server name"
          hint={name.data && !name.data.name ? `Default: ${name.data.computer_name}` : undefined}
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
        <Row title="Restart server">
          <Keyed action="SettingsRestartServer" chord="E">
            <ConfirmButton
              label="Restart"
              kind="secondary"
              question="Answers that are running now are interrupted. Restart the server now?"
              disabled={busy}
              onConfirm={() => void run(() => connection.write("server.restart", undefined, { retry: false }))}
            />
          </Keyed>
        </Row>
        <Power />
      </Group>
      <OtherServers />
      <Problem error={name.error ?? health.error ?? error} />
    </>
  );
}

/** Phone notifications: on or off, a test, and the Apple key with its ids. */
function PushKey({ clients, status }: { clients: { id: string; name: string }[]; status: PushStatus }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const saved = { team_id: status.team_id ?? "", key_id: status.key_id ?? "", bundle_id: status.bundle_id ?? "" };
  const [form, setForm] = useState(saved);
  const [keyFile, setKeyFile] = useState<string | null>(null);
  const [chosenFile, setChosenFile] = useState<File | null>(null);
  const [phone, setPhone] = useState("");
  const [sent, setSent] = useState(false);
  const changed =
    keyFile !== null ||
    form.team_id !== saved.team_id ||
    form.key_id !== saved.key_id ||
    form.bundle_id !== saved.bundle_id;
  const save = () =>
    run(() =>
      connection.write("push.configure", { ...form, ...(keyFile ? { key_p8: keyFile } : {}) }, { retry: false }),
    ).then((ok) => {
      // The private key stays only as long as it takes to send it.
      if (ok) {
        setKeyFile(null);
        setChosenFile(null);
      }
    });
  const field = (key: keyof typeof form, label: string, width: number) => (
    <Row title={label}>
      <TextInput
        label={label}
        mono
        width={width}
        value={form[key]}
        onChange={(e) => setForm({ ...form, [key]: e.target.value })}
      />
    </Row>
  );
  return (
    <Group title="Phone notifications">
      <Row
        title={
          <>
            Notifications to phones
            {status.apns ? <Chip tone="ok">On</Chip> : <Chip>Off</Chip>}
          </>
        }
        hint={status.last_error ?? (sent ? "Sent. It should show on that phone." : undefined)}
      >
        {status.apns && clients.length > 0 ? (
          <>
            <Selector
              label="Phone"
              isLabelHidden
              size="sm"
              variant="secondary"
              width={180}
              placeholder="Choose a device"
              {...(phone ? { value: phone } : {})}
              options={clients.map((c) => ({ value: c.id, label: c.name }))}
              onChange={setPhone}
            />
            <Keyed action="SettingsPushTest" chord="T">
              <SmallButton
                disabled={busy || !phone}
                onClick={() =>
                  void run(() => connection.write("push.test", { client_id: phone }, { retry: false })).then(setSent)
                }
              >
                Send a test
              </SmallButton>
            </Keyed>
          </>
        ) : null}
      </Row>
      <Row
        title="APNs key"
        hint={
          keyFile ? (
            "Chosen. It is sent when you save."
          ) : status.apns ? (
            <span {...stylex.props(styles.mono)}>{`AuthKey_${status.key_id ?? ""}.p8`}</span>
          ) : (
            "An Apple APNs key (.p8)"
          )
        }
      >
        <FileInput
          label="APNs key file"
          isLabelHidden
          accept=".p8"
          width={180}
          value={chosenFile}
          onChange={(picked) => {
            const file = Array.isArray(picked) ? (picked[0] ?? null) : picked;
            setChosenFile(file);
            if (file) void file.text().then(setKeyFile);
            else setKeyFile(null);
          }}
        />
      </Row>
      {field("team_id", "Team id", 180)}
      {field("key_id", "Key id", 180)}
      {field("bundle_id", "Bundle id", 180)}
      {changed ? (
        <Foot>
          <SmallButton
            kind="primary"
            disabled={busy || !form.team_id || !form.key_id || !form.bundle_id}
            onClick={() => void save()}
          >
            Save
          </SmallButton>
        </Foot>
      ) : null}
      <Problem error={error} />
    </Group>
  );
}

/** Server › Remote & phones: remote access, devices waiting or without an owner, and notifications. */
export function RemoteSection() {
  const platform = usePlatform();
  const connection = useConnection();
  const { features } = useLinkState();
  const { busy, error, run } = useChange();
  const status = useSetting(
    "remote",
    async () => expectBody(await connection.call("remote.get_status"), "remote").value,
  );
  const data = status.data;
  // Each board member's devices are in Account (`own_devices`); here only what is the server's.
  const own = features.includes(OWN_DEVICES);
  const shown = remoteAccessDevices(data?.clients ?? [], own);
  const now = Date.now();
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
  return (
    <>
      <Group>
        <Row
          title={
            <>
              Remote access
              {data?.enabled ? (
                <Chip tone="ok" dot>
                  On
                </Chip>
              ) : (
                <Chip>Off</Chip>
              )}
            </>
          }
          hint={
            data?.tailscale.dns_name ? (
              <span {...stylex.props(styles.mono)}>{data.tailscale.dns_name}</span>
            ) : (
              (data?.tailscale.error ??
              (data?.tailscale.installed === false
                ? "Tailscale is not installed."
                : "Through Tailscale, on your own devices only."))
            )
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
        {!own && data?.enabled ? <PairDevice title="Pair a device" /> : null}
      </Group>
      {data && !own ? <PendingPairings pending={data.pending} /> : null}
      {shown.length > 0 ? (
        <Group title={own ? "Devices without an owner" : "Paired devices"} count={shown.length}>
          {shown.map((c) => (
            <Row
              key={c.id}
              start={
                <IconBox>
                  <Icon name={/ios|iphone|ipad|android/i.test(c.platform) ? "phone" : "laptop"} />
                </IconBox>
              }
              title={c.name}
              hint={deviceHint(c, (ms) => agoText(ms, now))}
            >
              <Keyed action="SettingsRemove" chord="Del">
                <ConfirmButton
                  label="Revoke"
                  kind="secondary"
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
