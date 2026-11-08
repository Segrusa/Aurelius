// The servers this device can work with: this computer's service and paired ones.

import { Banner } from "@astryxdesign/core/Banner";
import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { List, ListItem } from "@astryxdesign/core/List";
import { Text } from "@astryxdesign/core/Text";
import { TextArea } from "@astryxdesign/core/TextArea";
import * as stylex from "@stylexjs/stylex";
import { useCallback, useEffect, useState } from "react";
import { type ServerChoices, usePlatform } from "../platform";
import { Confirm } from "./org-kit";
import { Keyed } from "./settings-kit";

const styles = stylex.create({
  panel: { display: "flex", flexDirection: "column", gap: 12 },
  list: {
    borderRadius: 8,
    borderWidth: 1,
    borderStyle: "solid",
    borderColor: "var(--color-border)",
  },
  actions: { display: "flex", alignItems: "center", gap: 8 },
  form: { display: "flex", flexDirection: "column", gap: 8 },
  pairing: { fontFamily: "var(--font-family-code)", fontSize: 12, resize: "none" },
  inUse: { fontSize: 12, color: "var(--color-text-accent)" },
  buttons: { display: "flex", gap: 8 },
});

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
      <span {...stylex.props(styles.inUse)}>In use</span>
    ) : (
      <Keyed action="SettingsUse" chord="Enter">
        <Button label="Use" size="sm" aria-label={`Use ${name}`} onClick={() => act(() => servers.use(hostId))} />
      </Keyed>
    );

  return (
    <section aria-label="Servers" {...stylex.props(styles.panel)}>
      {!choices.localAvailable && choices.servers.length === 0 ? (
        <EmptyState title="No server yet." description="Paste a pairing text from the server." isCompact />
      ) : (
        <List hasDividers density="compact" xstyle={styles.list}>
          {choices.localAvailable ? (
            <ListItem label={choices.localName ?? "This computer"} endContent={use(null, "this computer")} />
          ) : null}
          {choices.servers.map((s) => (
            <ListItem
              key={s.hostId}
              label={s.hostName}
              description={s.url}
              endContent={
                <span {...stylex.props(styles.actions)}>
                  {use(s.hostId, s.hostName)}
                  <Keyed action="SettingsRemove" chord="Del">
                    <Button
                      label="Forget"
                      size="sm"
                      aria-label={`Forget ${s.hostName}`}
                      onClick={() => setForgetting(s)}
                    />
                  </Keyed>
                </span>
              }
            />
          ))}
        </List>
      )}
      <form
        {...stylex.props(styles.form)}
        onSubmit={(event) => {
          event.preventDefault();
          void pair();
        }}
      >
        <TextArea
          label="Pair a server"
          value={text}
          onChange={setText}
          rows={2}
          isDisabled={waiting}
          placeholder="aurelius-pair:…"
          autoComplete="off"
          hasSpellCheck={false}
          controlXstyle={styles.pairing}
        />
        <Text as="p" type="supporting">
          {waiting
            ? 'Waiting until you choose "Allow as owner" on the server (Settings, Remote access there).'
            : "On the server: Settings, Remote access, show a code, Copy. A paired computer gets full access to that server."}
        </Text>
        <div {...stylex.props(styles.buttons)}>
          <Button label="Pair" variant="primary" type="submit" isDisabled={waiting || text.trim() === ""} />
          {waiting ? <Button label="Cancel" onClick={() => void servers.cancelPairing()} /> : null}
        </div>
      </form>
      {error ? <Banner status="error" role="alert" title={error} /> : null}
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
