// An employee's scratchpad (C7): its own note, read-only here.

import { IconButton } from "@astryxdesign/core/IconButton";
import { Popover } from "@astryxdesign/core/Popover";
import { expectBody } from "@aurelius/client";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useKeyed } from "../data";
import { Icon } from "../ui/icons";
import { Eyebrow, Loading } from "./org-kit";
import { when } from "./settings-kit";

const styles = stylex.create({
  section: { display: "flex", flexDirection: "column", gap: 4 },
  text: {
    margin: 0,
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
    fontSize: 13,
    lineHeight: 1.5,
    color: "var(--color-text-secondary)",
  },
  when: { fontSize: 11, color: "var(--precision-text3)" },
});

function useScratchpad(employeeId: string, wanted: boolean) {
  const connection = useConnection();
  const { features } = useLinkState();
  return useKeyed(
    ["employees", "scratchpad", employeeId],
    async () =>
      expectBody(await connection.call("employee.scratchpad", { employee_id: employeeId }), "employee_scratchpad")
        .value,
    wanted && features.includes("employee_scratchpad"),
  );
}

function Pad({ employeeId, wanted }: { employeeId: string; wanted: boolean }) {
  const pad = useScratchpad(employeeId, wanted);
  if (pad.isPending) return <Loading rows={1} />;
  if (!pad.data?.text) return <p {...stylex.props(styles.text)}>The scratchpad is empty.</p>;
  return (
    <div {...stylex.props(styles.section)}>
      <p {...stylex.props(styles.text)}>{pad.data.text}</p>
      <span {...stylex.props(styles.when)}>Updated {when(pad.data.updated_at_ms)}</span>
    </div>
  );
}

/** The note icon on a run card. */
export function ScratchpadButton({ employeeId }: { employeeId: string }) {
  const [open, setOpen] = useState(false);
  return (
    <Popover
      label="Scratchpad"
      isOpen={open}
      onOpenChange={setOpen}
      width={320}
      placement="below"
      alignment="end"
      content={<Pad employeeId={employeeId} wanted={open} />}
    >
      <IconButton label="Scratchpad" size="sm" icon={<Icon name="note" size={14} />} />
    </Popover>
  );
}

/** The scratchpad on an employee's page, when there is one. */
export function ScratchpadSection({ employeeId }: { employeeId: string }) {
  const pad = useScratchpad(employeeId, true);
  if (!pad.data?.text) return null;
  return (
    <section aria-label="Scratchpad" {...stylex.props(styles.section)}>
      <Eyebrow>SCRATCHPAD</Eyebrow>
      <p {...stylex.props(styles.text)}>{pad.data.text}</p>
    </section>
  );
}
