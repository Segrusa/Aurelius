import { expectBody } from "@aurelius/client";
import { type FormEvent, useState } from "react";
import { type Appearance, useAppearance } from "../appearance";
import { useConnection, useLinkState } from "../connection";
import { usePlatform } from "../platform";
import { Segments } from "../ui/kit";
import {
  ConfirmButton,
  Group,
  Muted,
  Problem,
  Row,
  SmallButton,
  TextInput,
  useChange,
  useSetting,
  when,
} from "./settings-kit";

function ChangePassword() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const [oldPassword, setOldPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [done, setDone] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setDone(false);
    const ok = await run(() => connection.setPassword(oldPassword, newPassword));
    setOldPassword("");
    setNewPassword("");
    setDone(ok);
  };
  return (
    <div className="flex flex-col gap-2 px-4 py-[14px]">
      <div className="font-medium">Password</div>
      <form className="flex flex-wrap items-center gap-2" onSubmit={(e) => void submit(e)}>
        <TextInput
          label="Current password"
          placeholder="Current password"
          type="password"
          autoComplete="current-password"
          required
          value={oldPassword}
          onChange={(e) => setOldPassword(e.target.value)}
        />
        <TextInput
          label="New password"
          placeholder="New password"
          type="password"
          autoComplete="new-password"
          required
          value={newPassword}
          onChange={(e) => setNewPassword(e.target.value)}
        />
        <SmallButton type="submit" disabled={busy}>
          Change password
        </SmallButton>
      </form>
      {done ? <Muted>Your password is changed.</Muted> : null}
      <Problem error={error} />
    </div>
  );
}

function Sessions() {
  const connection = useConnection();
  const { busy, error, run } = useChange();
  const { user } = useLinkState();
  const sessions = useSetting(
    "sessions",
    async () => expectBody(await connection.call("session.list", {}), "sessions").value.sessions,
  );
  return (
    <Group title="Signed in on">
      {(sessions.data ?? []).map((s) => (
        <Row key={s.id} title={s.device_name} hint={s.current ? "This device" : `Last seen ${when(s.last_seen_ms)}`}>
          {s.current ? null : (
            <ConfirmButton
              label="Sign out"
              question={`Sign out ${s.device_name}?`}
              disabled={busy}
              onConfirm={() =>
                void run(() => connection.write("session.revoke", { session_id: s.id }, { retry: false }))
              }
            />
          )}
        </Row>
      ))}
      {sessions.data?.length === 0 ? <Row title={user?.username ?? ""} hint="No other devices." /> : null}
      <Problem error={sessions.error ?? error} />
    </Group>
  );
}

const CLAUDE_LOGIN = {
  linked: "Connected",
  expired: "Expired. Claude asked to sign in again.",
  missing: "Not connected. Your tickets wait until you connect.",
} as const;

export function AccountSection() {
  const connection = useConnection();
  const { user, claudeLogin } = useLinkState();
  return (
    <>
      <Group>
        <Row title={user?.username ?? "Nobody"} hint="The board member signed in on this device.">
          <SmallButton onClick={() => void connection.signOut()}>Sign out</SmallButton>
        </Row>
        <ChangePassword />
      </Group>
      <Group title="Claude login">
        <Row
          title={claudeLogin ? CLAUDE_LOGIN[claudeLogin.status] : "Unknown"}
          hint={
            [claudeLogin?.email, claudeLogin?.plan].filter(Boolean).join(" · ") ||
            "Connecting a login still goes through the desktop app."
          }
        />
      </Group>
      <Sessions />
    </>
  );
}

const APPEARANCES: { key: Appearance; label: string }[] = [
  { key: "system", label: "System" },
  { key: "light", label: "Light" },
  { key: "dark", label: "Dark" },
];

export function AppSection() {
  const { appearance, choose } = useAppearance();
  const platform = usePlatform();
  const { daemonVersion } = useLinkState();
  return (
    <>
      <Group>
        <Row title="Appearance" hint="Kept on this device.">
          <Segments label="Appearance" value={appearance} options={APPEARANCES} onChange={choose} />
        </Row>
      </Group>
      <Group title="Version">
        <Row title="This app" hint={platform.kind === "web" ? "In a browser" : "Desktop"}>
          <span className="font-mono text-[12px] text-text2">{platform.version}</span>
        </Row>
        <Row title="The server">
          <span className="font-mono text-[12px] text-text2">{daemonVersion ?? "Unknown"}</span>
        </Row>
      </Group>
    </>
  );
}
