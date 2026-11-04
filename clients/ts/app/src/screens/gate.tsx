// What comes before the app: the connection's state, pairing a browser and the sign-in.

import { boardMemberStep } from "@aurelius/client";
import { type FormEvent, type ReactNode, useRef, useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { usePlatform } from "../platform";
import { Button } from "../ui/kit";
import { ServerChoicesPanel } from "./servers";

function Centered({ title, children, wide = false }: { title: string; children?: ReactNode; wide?: boolean }) {
  return (
    <main className="grid h-full place-items-center overflow-y-auto bg-bg p-4">
      <div
        className={`enter flex w-full ${wide ? "max-w-[520px]" : "max-w-[360px]"} flex-col gap-4 rounded-[12px] border border-line bg-panel p-6 shadow-panel`}
      >
        <h1 className="m-0 text-[16px] font-semibold">{title}</h1>
        {children}
      </div>
    </main>
  );
}

const FIELD = "h-9 w-full rounded-[6px] border border-btn-line bg-sunk px-3 text-[13px] text-text";

function Field({ label, ...props }: { label: string } & React.InputHTMLAttributes<HTMLInputElement>) {
  return (
    <label className="flex flex-col gap-1 text-[12px] font-medium text-text2">
      {label}
      <input className={FIELD} {...props} />
    </label>
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
      {signedOut ? <p className="m-0 text-text2">You were signed out on this device.</p> : null}
      <form className="flex flex-col gap-3" onSubmit={(e) => void submit(e)}>
        <Field
          label="Name"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="username"
          required
        />
        <Field
          label="Password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete={first ? "new-password" : "current-password"}
          required
        />
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
        <Button kind="primary" type="submit" disabled={busy} className="justify-center">
          {first ? "Create" : "Sign in"}
        </Button>
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
      <form className="flex flex-col gap-3" onSubmit={(e) => void submit(e)}>
        <Field
          label="Current password"
          type="password"
          value={oldPassword}
          onChange={(e) => setOldPassword(e.target.value)}
          autoComplete="current-password"
          required
        />
        <Field
          label="New password"
          type="password"
          value={newPassword}
          onChange={(e) => setNewPassword(e.target.value)}
          autoComplete="new-password"
          required
        />
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
        <Button kind="primary" type="submit" disabled={busy} className="justify-center">
          Change password
        </Button>
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

  return (
    <Centered title="Pair this browser">
      <p className="m-0 leading-[1.6] text-text2">
        {state === "waiting"
          ? "Allow this browser in Aurelius on the server."
          : "Show a pairing code in Aurelius on the server, under Settings, Remote access."}
      </p>
      <form className="flex flex-col gap-3" onSubmit={(e) => void submit(e)}>
        <Field
          label="Code"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          autoComplete="off"
          required
          disabled={state === "waiting"}
        />
        <Field
          label="Name of this browser"
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          disabled={state === "waiting"}
        />
        {error ? (
          <p role="alert" className="m-0 text-red">
            {error}
          </p>
        ) : null}
        <Button kind="primary" type="submit" disabled={state === "waiting"} className="justify-center">
          Pair
        </Button>
      </form>
    </Centered>
  );
}

/** Signing in on a paired server isn't the only way on: this computer may have its own. */
function LeaveServer() {
  const { servers } = usePlatform();
  const [open, setOpen] = useState(false);
  return (
    <div className="fixed right-4 bottom-4 flex max-h-[80%] w-[420px] max-w-[calc(100%-32px)] flex-col items-end gap-2 overflow-y-auto">
      {open ? (
        <div className="w-full rounded-[10px] border border-line bg-panel p-3">
          <ServerChoicesPanel />
        </div>
      ) : null}
      {servers ? <Button onClick={() => setOpen(!open)}>Servers</Button> : null}
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
        <p className="m-0 text-text2">{state.message}</p>
        {servers}
      </Centered>
    );
  }
  if (state.status === "incompatible") {
    return (
      <Centered title={platform.target ? "This server can't be used" : "Different versions"} wide={servers !== null}>
        <p className="m-0 text-text2">{state.message}</p>
        {servers}
      </Centered>
    );
  }
  if (step === "unknown") {
    // Only before the first answer: the retries after it must not wipe what is being typed here.
    if (state.status !== "connecting") failedOnce.current = true;
    if (!failedOnce.current) return <div className="h-full bg-bg" />;
    return (
      <Centered title="Not connected" wide={servers !== null}>
        <p className="m-0 text-text2">{state.message}</p>
        <Button onClick={() => connection.wake()} className="justify-center">
          Try again
        </Button>
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
