// What comes before the app: the connection's state, pairing a browser and the sign-in.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { Heading } from "@astryxdesign/core/Heading";
import { Text } from "@astryxdesign/core/Text";
import { TextInput } from "@astryxdesign/core/TextInput";
import { boardMemberStep } from "@aurelius/client";
import * as stylex from "@stylexjs/stylex";
import { type FormEvent, type ReactNode, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { usePlatform } from "../platform";
import { Logo } from "../ui/icons";
import { ServerChoicesPanel } from "./servers";

const enter = stylex.keyframes({
  from: { opacity: 0, transform: "translateY(4px)" },
});

const styles = stylex.create({
  page: {
    display: "grid",
    height: "100%",
    placeItems: "center",
    overflowY: "auto",
    backgroundColor: "var(--color-background-body)",
    padding: 16,
    boxSizing: "border-box",
  },
  blank: { height: "100%", backgroundColor: "var(--color-background-body)" },
  card: {
    display: "flex",
    width: "100%",
    maxWidth: 360,
    flexDirection: "column",
    gap: 16,
    borderRadius: 12,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
    padding: 24,
    boxSizing: "border-box",
    boxShadow: "var(--precision-panel-shadow)",
    animationName: enter,
    animationDuration: "160ms",
    animationTimingFunction: "cubic-bezier(0.2, 0, 0, 1)",
  },
  wide: { maxWidth: 520 },
  title: { fontSize: 16, fontWeight: 600 },
  form: { display: "flex", flexDirection: "column", gap: 12 },
  leave: {
    position: "fixed",
    right: 16,
    bottom: 16,
    display: "flex",
    maxHeight: "80%",
    width: 420,
    maxWidth: "calc(100% - 32px)",
    flexDirection: "column",
    alignItems: "flex-end",
    gap: 8,
    overflowY: "auto",
  },
  leavePanel: {
    width: "100%",
    borderRadius: 10,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    backgroundColor: "var(--color-background-surface)",
    padding: 12,
    boxSizing: "border-box",
  },
});

function Centered({ title, children, wide = false }: { title: string; children?: ReactNode; wide?: boolean }) {
  return (
    <main {...stylex.props(styles.page)}>
      <div {...stylex.props(styles.card, wide && styles.wide)}>
        <Logo size={28} />
        <Heading level={1} xstyle={styles.title}>
          {title}
        </Heading>
        {children}
      </div>
    </main>
  );
}

function Lead({ children }: { children: ReactNode }) {
  return (
    <Text as="p" color="secondary">
      {children}
    </Text>
  );
}

function SignIn({ first }: { first: boolean }) {
  const connection = useConnection();
  const { signedOut } = useLinkState();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      // Never resent after a reconnect, so no password waits in the outbox.
      if (first) await connection.write("user.create", { username: username.trim(), password }, { retry: false });
      await connection.signIn(username, password);
    } catch (e) {
      setError(e instanceof Error ? e.message : "This did not work.");
    } finally {
      setBusy(false);
      setPassword("");
    }
  };

  return (
    <Centered title={first ? "Create the first board member" : "Sign in"}>
      {signedOut ? <Lead>You were signed out on this device.</Lead> : null}
      <form {...stylex.props(styles.form)} onSubmit={(e) => void submit(e)}>
        <TextInput label="Name" value={username} onChange={setUsername} autoComplete="username" />
        <TextInput
          label="Password"
          type="password"
          value={password}
          onChange={setPassword}
          autoComplete={first ? "new-password" : "current-password"}
        />
        {error ? <Banner status="error" role="alert" title={error} /> : null}
        <Button
          label={first ? "Create" : "Sign in"}
          variant="primary"
          type="submit"
          width="100%"
          isDisabled={busy || username.trim() === "" || password === ""}
        />
      </form>
    </Centered>
  );
}

function ForcedPassword() {
  const connection = useConnection();
  const [oldPassword, setOldPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await connection.setPassword(oldPassword, newPassword);
    } catch (e) {
      setError(e instanceof Error ? e.message : "This did not work.");
      setBusy(false);
    }
  };

  return (
    <Centered title="Choose a new password">
      <form {...stylex.props(styles.form)} onSubmit={(e) => void submit(e)}>
        <TextInput
          label="Current password"
          type="password"
          value={oldPassword}
          onChange={setOldPassword}
          autoComplete="current-password"
        />
        <TextInput
          label="New password"
          type="password"
          value={newPassword}
          onChange={setNewPassword}
          autoComplete="new-password"
        />
        {error ? <Banner status="error" role="alert" title={error} /> : null}
        <Button
          label="Change password"
          variant="primary"
          type="submit"
          width="100%"
          isDisabled={busy || oldPassword === "" || newPassword === ""}
        />
      </form>
    </Centered>
  );
}

function Pair({ onPaired }: { onPaired: () => void }) {
  const platform = usePlatform();
  const [code, setCode] = useState("");
  const [name, setName] = useState(platform.deviceName);
  const [state, setState] = useState<"idle" | "waiting">("idle");
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setState("waiting");
    setError(null);
    try {
      await platform.pair?.(code, name);
      onPaired();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Pairing failed.");
      setState("idle");
    }
  };

  const waiting = state === "waiting";
  return (
    <Centered title="Pair this browser">
      <Lead>
        {waiting
          ? "Allow this browser in Aurelius on the server."
          : "Show a pairing code in Aurelius on the server, under Settings, Remote access."}
      </Lead>
      <form {...stylex.props(styles.form)} onSubmit={(e) => void submit(e)}>
        <TextInput label="Code" value={code} onChange={setCode} autoComplete="off" isDisabled={waiting} />
        <TextInput label="Name of this browser" value={name} onChange={setName} isDisabled={waiting} />
        {error ? <Banner status="error" role="alert" title={error} /> : null}
        <Button
          label="Pair"
          variant="primary"
          type="submit"
          width="100%"
          isDisabled={waiting || code === "" || name === ""}
        />
      </form>
    </Centered>
  );
}

/** Signing in on a paired server isn't the only way on: this computer may have its own. */
function LeaveServer() {
  const { servers } = usePlatform();
  const [open, setOpen] = useState(false);
  return (
    <div {...stylex.props(styles.leave)}>
      {open ? (
        <div {...stylex.props(styles.leavePanel)}>
          <ServerChoicesPanel />
        </div>
      ) : null}
      {servers ? <Button label="Servers" onClick={() => setOpen(!open)} /> : null}
    </div>
  );
}

export function Gate({ children, onPaired }: { children: ReactNode; onPaired: () => void }) {
  const platform = usePlatform();
  const connection = useConnection();
  const state = useLinkState();
  const step = boardMemberStep(state);
  const failedOnce = useRef(false);

  const servers = platform.servers ? <ServerChoicesPanel /> : null;
  if (state.status === "revoked") {
    if (platform.pair) return <Pair onPaired={onPaired} />;
    return (
      <Centered title="No access" wide={servers !== null}>
        <Lead>{state.message}</Lead>
        {servers}
      </Centered>
    );
  }
  if (state.status === "incompatible") {
    return (
      <Centered title={platform.target ? "This server can't be used" : "Different versions"} wide={servers !== null}>
        <Lead>{state.message}</Lead>
        {servers}
      </Centered>
    );
  }
  if (step === "unknown") {
    // Only before the first answer: the retries after it must not wipe what is being typed here.
    if (state.status !== "connecting") failedOnce.current = true;
    if (!failedOnce.current) return <div {...stylex.props(styles.blank)} />;
    return (
      <Centered title="Not connected" wide={servers !== null}>
        <Lead>{state.message}</Lead>
        <Button label="Try again" width="100%" onClick={() => connection.wake()} />
        {servers}
      </Centered>
    );
  }
  if (step === "create_first" || step === "sign_in") {
    return (
      <>
        <SignIn first={step === "create_first"} />
        {platform.target ? <LeaveServer /> : null}
      </>
    );
  }
  if (step === "change_password") return <ForcedPassword />;
  return <>{children}</>;
}
