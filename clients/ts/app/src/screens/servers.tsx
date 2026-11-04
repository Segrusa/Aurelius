// The servers this device can work with: this computer's service and paired ones.

import { useCallback, useEffect, useState } from "react";
import { type ServerChoices, usePlatform } from "../platform";
import { Button } from "../ui/kit";
import { Confirm } from "./org-kit";

const ROW = "flex items-center gap-2 border-b border-line2 px-4 py-[10px] text-[13px] last:border-b-0";
const SMALL = "h-7 px-[10px] text-[12px]";

/** The list and the pairing form. A server that is chosen or paired makes the app start over. */
export function ServerChoicesPanel() {
  const { servers } = usePlatform();
  const [choices, setChoices] = useState<ServerChoices | null>(null);
  const [text, setText] = useState("");
  const [waiting, setWaiting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [forgetting, setForgetting] = useState<{ hostId: string; hostName: string } | null>(null);

  const reload = useCallback(() => {
    void servers?.list().then(setChoices);
  }, [servers]);
  useEffect(reload, [reload]);

  if (servers === null || choices === null) return null;
  const act = (run: () => Promise<unknown>) => {
    setError(null);
    run()
      .then(reload)
      .catch((e: unknown) => setError(e instanceof Error ? e.message : "This did not work."));
  };
  const pair = async () => {
    setError(null);
    setWaiting(true);
    try {
      await servers.pair(text);
      setText("");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Pairing failed.");
    } finally {
      setWaiting(false);
    }
  };
  const use = (hostId: string | null, name: string) =>
    choices.active === hostId ? (
      <span className="ml-auto text-[12px] text-acc-text">In use</span>
    ) : (
      <Button className={`ml-auto ${SMALL}`} aria-label={`Use ${name}`} onClick={() => act(() => servers.use(hostId))}>
        Use
      </Button>
    );

  return (
    <section aria-label="Servers" className="flex flex-col gap-3">
      <div className="rounded-[8px] border border-line">
        {choices.localAvailable ? (
          <div className={ROW}>
            <span className="min-w-0 truncate">{choices.localName ?? "This computer"}</span>
            {use(null, "this computer")}
          </div>
        ) : null}
        {choices.servers.map((s) => (
          <div key={s.hostId} className={ROW}>
            <span className="flex min-w-0 flex-col">
              <span className="truncate">{s.hostName}</span>
              <span className="truncate text-[12px] text-text3">{s.url}</span>
            </span>
            {use(s.hostId, s.hostName)}
            <Button className={SMALL} aria-label={`Forget ${s.hostName}`} onClick={() => setForgetting(s)}>
              Forget
            </Button>
          </div>
        ))}
        {!choices.localAvailable && choices.servers.length === 0 ? (
          <p className="m-0 px-4 py-3 text-[13px] text-text3">No server yet. Paste a pairing text from the server.</p>
        ) : null}
      </div>
      <form
        className="flex flex-col gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          void pair();
        }}
      >
        <label className="flex flex-col gap-1 text-[12px] font-medium text-text2">
          Pair a server
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={2}
            disabled={waiting}
            placeholder="aurelius-pair:…"
            autoComplete="off"
            spellCheck={false}
            className="resize-none rounded-[6px] border border-btn-line bg-sunk p-2 font-mono text-[12px] text-text"
          />
        </label>
        <p className="m-0 text-[12px] leading-[1.5] text-text3">
          {waiting
            ? 'Waiting until you choose "Allow as owner" on the server (Settings, Remote access there).'
            : "On the server: Settings, Remote access, show a code, Copy. A paired computer gets full access to that server."}
        </p>
        <div className="flex gap-2">
          <Button kind="primary" type="submit" disabled={waiting || text.trim() === ""}>
            Pair
          </Button>
          {waiting ? <Button onClick={() => void servers.cancelPairing()}>Cancel</Button> : null}
        </div>
      </form>
      {error ? (
        <p role="alert" className="m-0 text-[12px] text-red">
          {error}
        </p>
      ) : null}
      {forgetting ? (
        <Confirm
          title={`Forget ${forgetting.hostName}?`}
          body="This computer loses its access. Pair it again to come back; the server keeps its data."
          action="Forget"
          danger
          onClose={() => setForgetting(null)}
          onConfirm={() => {
            const { hostId } = forgetting;
            setForgetting(null);
            act(() => servers.forget(hostId));
          }}
        />
      ) : null}
    </section>
  );
}
