import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { expectBody } from "@aurelius/client";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useEffect, useState } from "react";
import { type Appearance, useAppearance } from "../appearance";
import { useConnection, useLinkState } from "../connection";
import { useAction } from "../keys/provider";
import { usePlatform } from "../platform";
import { Icon } from "../ui/icons";
import { initial, Mark, markColor } from "../ui/marks";
import { ClaudeLogin, passwordProblem } from "./settings-accounts";
import { PairDevice, PendingPairings } from "./settings-devices";
import {
  Chip,
  ConfirmButton,
  Foot,
  Group,
  HINTS_KEY,
  IconBox,
  Keyed,
  Muted,
  Part,
  Problem,
  Row,
  SmallButton,
  Switch,
  TextInput,
  useChange,
  useSetting,
} from "./settings-kit";
import { agoText, deviceHint, OWN_DEVICES } from "./settings-logic";

const styles = stylex.create({
  name: { fontSize: 15, fontWeight: 600 },
  stack: { display: "flex", flexDirection: "column", gap: 10 },
  buttons: { display: "flex", flexWrap: "wrap", gap: 8 },
  status: { color: "var(--color-text-secondary)" },
  notes: { margin: 0, whiteSpace: "pre-wrap", color: "var(--color-text-secondary)" },
});

/** The open Change row: three fields, Save password and Cancel. */
function ChangePassword({ onClose }: { onClose: () => void }) {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [oldPassword, setOldPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [done, setDone] = useState(false);
  const problem = passwordProblem(newPassword, repeat);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setDone(false);
    if (problem) return;
    const ok = await run(() => connection.setPassword(oldPassword, newPassword));
    setOldPassword("");
    setNewPassword("");
    setRepeat("");
    setDone(ok);
  };
  return (
    <Part>
      <form {...stylex.props(styles.stack)} onSubmit={(e) => void submit(e)}>
        <TextInput
          label="Current password"
          placeholder="Current password"
          type="password"
          autoComplete="current-password"
          autoFocus
          required
          value={oldPassword}
          onChange={(e) => setOldPassword(e.target.value)}
        />
        <TextInput
          label="New password"
          placeholder="New password · at least 12 characters"
          type="password"
          autoComplete="new-password"
          required
          value={newPassword}
          onChange={(e) => setNewPassword(e.target.value)}
        />
        <TextInput
          label="Repeat new password"
          placeholder="Repeat new password"
          type="password"
          autoComplete="new-password"
          required
          value={repeat}
          onChange={(e) => setRepeat(e.target.value)}
        />
        {newPassword && problem ? <Muted>{problem}</Muted> : null}
        {done ? <Muted>Your password is changed.</Muted> : null}
        <div {...stylex.props(styles.buttons)}>
          <SmallButton kind="primary" type="submit" disabled={busy || problem !== null}>
            Save password
          </SmallButton>
          <SmallButton kind="ghost" onClick={onClose}>
            Cancel
          </SmallButton>
        </div>
      </form>
      <Problem error={error} />
    </Part>
  );
}

const PHONE = /ios|iphone|ipad|android/i;

/**
 * Account → Devices: where you are signed in and the devices you paired, in one list. A signed-in
 * session of a paired device shows once, as that device.
 */
function Devices() {
  const platform = usePlatform();
  const connection = useConnection();
  const { features } = useLinkState();
  const { busy, error, run } = useChange();
  const [adding, setAdding] = useState(false);
  const own = features.includes(OWN_DEVICES);
  const sessions = useSetting(
    "sessions",
    async () => expectBody(await connection.call("session.list", {}), "sessions").value.sessions,
  );
  const devices = useSetting("devices", async () =>
    own ? expectBody(await connection.call("device.list"), "devices").value.devices : [],
  );
  const remote = useSetting(
    "remote",
    async () => expectBody(await connection.call("remote.get_status"), "remote").value,
  );
  const now = Date.now();
  const all = sessions.data ?? [];
  const current = all.find((s) => s.current);
  const paired = (devices.data ?? []).filter((d) => d.id !== current?.client_id);
  const pairedIds = new Set((devices.data ?? []).map((d) => d.id));
  const others = all.filter((s) => !s.current && !(s.client_id && pairedIds.has(s.client_id)));
  const signedIn = all.filter((s) => !s.current);
  // A browser never allows other devices; a phone or other non-owner device cannot read the status.
  const canPair = own && platform.kind !== "web" && remote.data !== undefined;
  const count = (current ? 1 : 0) + paired.length + others.length;
  const signOutAll = () =>
    run(async () => {
      for (const s of signedIn) await connection.write("session.revoke", { session_id: s.id }, { retry: false });
    });
  return (
    <>
      <Group
        title="Devices"
        count={count}
        action={
          canPair ? (
            <SmallButton
              disabled={!remote.data?.enabled}
              title={remote.data?.enabled ? undefined : "Turn on remote access first, under Server › Remote & phones."}
              onClick={() => setAdding(!adding)}
            >
              {adding ? "Done" : "Add a device"}
            </SmallButton>
          ) : undefined
        }
      >
        {adding && canPair && remote.data?.enabled ? <PairDevice title="Add a device" /> : null}
        {current ? (
          <Row
            start={
              <IconBox>
                <Icon name={platform.kind === "web" ? "globe" : "laptop"} />
              </IconBox>
            }
            title={
              <>
                {current.device_name}
                <Chip tone="ok">This device</Chip>
              </>
            }
            hint={`${platform.kind === "web" ? "Browser" : "Desktop app"} · now`}
          />
        ) : null}
        {paired.map((d) => (
          <Row
            key={d.id}
            start={
              <IconBox>
                <Icon name={PHONE.test(d.platform) ? "phone" : "laptop"} />
              </IconBox>
            }
            title={d.name}
            hint={deviceHint(d, (ms) => agoText(ms, now))}
          >
            <Keyed action="SettingsRemove" chord="Del">
              <ConfirmButton
                label="Revoke"
                question={`Revoke ${d.name}? It is signed out and must be paired again.`}
                disabled={busy}
                onConfirm={() =>
                  void run(() => connection.write("device.revoke", { client_id: d.id }, { retry: false }))
                }
              />
            </Keyed>
          </Row>
        ))}
        {others.map((s) => (
          <Row
            key={s.id}
            start={
              <IconBox>
                <Icon name="globe" />
              </IconBox>
            }
            title={s.device_name}
            hint={`Signed in · ${agoText(s.last_seen_ms, now)}`}
          >
            <ConfirmButton
              label="Sign out"
              question={`Sign out ${s.device_name}?`}
              disabled={busy}
              onConfirm={() =>
                void run(() => connection.write("session.revoke", { session_id: s.id }, { retry: false }))
              }
            />
          </Row>
        ))}
        {signedIn.length > 0 ? (
          <Foot>
            <ConfirmButton
              label="Sign out all other devices"
              question="Sign out all other devices?"
              disabled={busy}
              onConfirm={() => void signOutAll()}
            />
          </Foot>
        ) : null}
        <Problem error={sessions.error ?? devices.error ?? error} />
      </Group>
      {canPair && remote.data ? <PendingPairings pending={remote.data.pending} /> : null}
    </>
  );
}

export function AccountSection() {
  const connection = useConnection();
  const { user } = useLinkState();
  const [changing, setChanging] = useState(false);
  const name = user?.username ?? "Nobody";
  return (
    <>
      <Group>
        <Row
          start={<Mark letter={initial(name)} color={markColor(user?.id ?? name)} size={40} radius={10} />}
          title={<span {...stylex.props(styles.name)}>{name}</span>}
          hint="Board member"
        >
          <SmallButton onClick={() => void connection.signOut()}>Sign out</SmallButton>
        </Row>
        <Row title="Password" joined={changing}>
          {changing ? null : <SmallButton onClick={() => setChanging(true)}>Change</SmallButton>}
        </Row>
        {changing ? <ChangePassword onClose={() => setChanging(false)} /> : null}
      </Group>
      <ClaudeLogin />
      <Devices />
    </>
  );
}

const APPEARANCES: { key: Appearance; label: string }[] = [
  { key: "system", label: "System" },
  { key: "light", label: "Light" },
  { key: "dark", label: "Dark" },
];

/** The service goes on when the window closes; whether it starts by itself is this computer's. */
function Background() {
  const { startup, kind } = usePlatform();
  const [on, setOn] = useState<boolean | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    if (startup) startup.get().then(setOn, () => setOn(null));
  }, [startup]);
  if (kind === "web") return null;
  return (
    <Group title="Background">
      {startup ? (
        <Row title="Start at sign-in">
          <Keyed action="SettingsLoginStart" chord="L">
            <Switch
              label="Start at sign-in"
              on={on === true}
              disabled={on === null}
              onChange={(next) => {
                setProblem(null);
                startup
                  .set(next)
                  .then(setOn, (e: unknown) => setProblem(e instanceof Error ? e.message : "This did not work."));
              }}
            />
          </Keyed>
        </Row>
      ) : null}
      <Row title="Closing the window">
        <span {...stylex.props(styles.status)}>Work continues</span>
      </Row>
      <Problem error={problem} />
    </Group>
  );
}

/**
 * This app's version and, for this computer's own install, its update: a paired server can't
 * update the app you look at.
 */
function AppVersion() {
  const connection = useConnection();
  const client = useQueryClient();
  const platform = usePlatform();
  const { remote } = useLinkState();
  const { busy, error, run } = useChange();
  const [notes, setNotes] = useState(false);
  const status = useSetting(
    "update",
    async () => expectBody(await connection.call("update.get", { check: false }), "update").value,
  );
  const app = platform.quit === null || remote ? undefined : status.data?.app;
  const title = `Aurelius ${platform.version}`;
  if (!app) {
    return (
      <Group title="App">
        <Row title={title} hint={platform.kind === "web" ? "In a browser" : "Desktop app"} />
      </Group>
    );
  }
  const check = () =>
    run(async () => {
      const body = await connection.call("update.get", { check: true });
      client.setQueryData(["settings", "update"], expectBody(body, "update").value);
    });
  const install = () =>
    run(async () => {
      const prepared = await connection.write("update.prepare_app", undefined, { retry: false, timeoutMs: 600_000 });
      const { result } = prepared;
      if (result.kind === "host" && result.value.type === "update" && result.value.error) {
        throw new Error(result.value.error);
      }
      await connection.write("update.apply_app", undefined, { retry: false });
      // The service replaces the app once it has closed, then opens the new one.
      platform.quit?.();
    });
  const chip = !app.enabled ? (
    <Chip>Updates are off</Chip>
  ) : app.available ? (
    <Chip tone="info">{`${app.latest ?? "A new version"} is available`}</Chip>
  ) : (
    <Chip tone="ok">Up to date</Chip>
  );
  return (
    <Group title="App">
      <Row
        joined={notes}
        title={
          <>
            {title}
            {chip}
          </>
        }
        hint={app.error ?? (app.checked_at_ms ? `Checked ${agoText(app.checked_at_ms, Date.now())}` : undefined)}
      >
        {app.notes ? (
          <SmallButton kind="ghost" onClick={() => setNotes(!notes)}>
            What's new
          </SmallButton>
        ) : null}
        {app.enabled && app.available ? (
          <Keyed action="SettingsInstallAppUpdate" chord="I">
            <ConfirmButton
              label="Update app"
              question="The app closes and comes back as the new version. Update now?"
              disabled={busy}
              onConfirm={() => void install()}
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
      {notes && app.notes ? (
        <Part>
          <p {...stylex.props(styles.notes)}>{app.notes}</p>
        </Part>
      ) : null}
      <Problem error={error} />
    </Group>
  );
}

/** You › This device: what this app and this computer keep for themselves. */
export function DeviceSection() {
  const { appearance, choose } = useAppearance();
  const { storage } = usePlatform();
  const [hints, setHints] = useState(() => storage.get(HINTS_KEY) === "true");
  useAction("SettingsAppearance", () => {
    const at = APPEARANCES.findIndex((a) => a.key === appearance);
    const next = APPEARANCES[(at + 1) % APPEARANCES.length];
    if (next) choose(next.key);
  });
  return (
    <>
      <Group>
        <Row title="Appearance">
          <SegmentedControl
            label="Appearance"
            size="sm"
            value={appearance}
            onChange={(next) => choose(next as Appearance)}
          >
            {APPEARANCES.map((a) => (
              <SegmentedControlItem key={a.key} value={a.key} label={a.label} />
            ))}
          </SegmentedControl>
        </Row>
        <Row title="Always show shortcut hints">
          <Keyed action="SettingsShowHints" chord="H">
            <Switch
              label="Always show shortcut hints"
              on={hints}
              onChange={(on) => {
                setHints(on);
                storage.set(HINTS_KEY, String(on));
                document.documentElement.toggleAttribute("data-hints", on);
              }}
            />
          </Keyed>
        </Row>
      </Group>
      <Background />
      <AppVersion />
    </>
  );
}
