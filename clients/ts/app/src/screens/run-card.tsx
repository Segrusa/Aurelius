// A run where its work is talked about: a note under a post.

import { Button } from "@astryxdesign/core/Button";
import type { MessageView } from "@aurelius/protocol";
import * as stylex from "@stylexjs/stylex";
import { useConnection } from "../connection";
import { toast } from "../ui/toast";
import { TypingDots } from "../ui/typing";
import { runState } from "./run-card-logic";

const styles = stylex.create({
  note: { marginTop: 4, display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--precision-text3)" },
  failed: { marginTop: 4, fontSize: 12, color: "var(--color-text-error)" },
});

/** Under a post: only that its author is still at work, or could not finish. How it went is on the ticket's Activity. */
export function RunNote({ m }: { m: MessageView }) {
  const connection = useConnection();
  const brief = m.run;
  const run = m.run_id;
  if (!brief || !run) return null;
  if (brief.status === "failed") return <div {...stylex.props(styles.failed)}>This did not finish.</div>;
  const state = runState(brief.status, Boolean(m.awaiting_user));
  if (!state.running || m.awaiting_user) return null;
  const stop = () =>
    connection
      .write("run.stop", { run_id: run }, { retry: false })
      .catch((e: unknown) => toast(e instanceof Error ? e.message : "This did not work."));
  return (
    <div data-run-note {...stylex.props(styles.note)}>
      <TypingDots />
      <span>Working</span>
      <Button size="sm" variant="ghost" label="Stop" onClick={() => void stop()} />
    </div>
  );
}
