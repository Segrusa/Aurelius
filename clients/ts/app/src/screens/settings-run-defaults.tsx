import { SegmentedControl, SegmentedControlItem } from "@astryxdesign/core/SegmentedControl";
import { Selector } from "@astryxdesign/core/Selector";
import { expectBody } from "@aurelius/client";
import type { AgentMode, UserRunDefaults } from "@aurelius/protocol";
import { useConnection, useLinkState } from "../connection";
import { useModels } from "./dialogs";
import { effortName, MODE_LABELS } from "./employee-logic";
import { Group, Muted, modelName, Problem, Row, useChange, useSetting } from "./settings-kit";
import {
  choiceToStore,
  defaultModelLabel,
  pressedChoice,
  type RunDefaultField,
  runDefaultValue,
  withRunDefault,
} from "./settings-logic";

const WIDTH = 220;

/** The modes My defaults offers, as in the design; a conversation can still choose Accept edits. */
const SHOWN_MODES: AgentMode[] = ["ask", "plan", "auto"];

/** What a conversation starts in when nobody chose a mode. */
const DEFAULT_MODE: AgentMode = "ask";

import { USER_RUN_DEFAULTS_FEATURE } from "./run-default-logic";

export { USER_RUN_DEFAULTS_FEATURE };

/**
 * The board member's own model, effort, mode and subagent model: the runs of their tickets and
 * sessions use them unless the conversation or the task chooses otherwise.
 */
export function MyDefaultsSection() {
  const connection = useConnection();
  const { features } = useLinkState();
  const models = useModels();
  const { busy, error, run } = useChange();
  const has = features.includes(USER_RUN_DEFAULTS_FEATURE);
  const view = useSetting("user_run_defaults", async () =>
    has ? expectBody(await connection.call("user.run_defaults"), "user_run_defaults").value : null,
  );
  if (!has) return <Muted>This server is too old to keep your own defaults. Update the server first.</Muted>;
  const data = view.data;
  if (!data) return <Problem error={view.error} />;
  const save = (field: RunDefaultField, value: string) =>
    run(() =>
      connection.write(
        "user.set_run_defaults",
        { defaults: withRunDefault(data.defaults, field, value) as UserRunDefaults },
        { retry: false },
      ),
    );
  const modelOptions = (defaultLabel: string) =>
    models.map((m) =>
      m.key === "default" ? { value: "default", label: defaultLabel } : { value: m.key, label: m.label },
    );
  const efforts = data.supported_efforts ?? [];
  const storedEffort = runDefaultValue(data.defaults, "effort");
  const effortShown = pressedChoice(storedEffort, data.model_default_effort);
  // A stored level the model lacks stays visible, so the control shows what is stored.
  const effortLevels =
    storedEffort !== "default" && !efforts.includes(storedEffort) ? [...efforts, storedEffort] : efforts;
  const storedMode = runDefaultValue(data.defaults, "mode");
  const modes =
    SHOWN_MODES.includes(storedMode as AgentMode) || storedMode === "default"
      ? SHOWN_MODES
      : [...SHOWN_MODES, storedMode as AgentMode];
  return (
    <>
      <Group>
        <Row title="Model">
          <Selector
            label="My default model"
            isLabelHidden
            size="sm"
            variant="secondary"
            width={WIDTH}
            isDisabled={busy}
            value={runDefaultValue(data.defaults, "model")}
            options={modelOptions(defaultModelLabel(data.defaults.model ? null : data.model, modelName))}
            onChange={(value) => void save("model", value)}
          />
        </Row>
        <Row title="Effort" hint={efforts.length === 0 ? "This model has no effort setting." : undefined}>
          {efforts.length === 0 ? null : (
            <SegmentedControl
              label="My default effort"
              size="sm"
              value={effortShown ?? ""}
              onChange={(value) => {
                if (!busy) void save("effort", choiceToStore(value, data.model_default_effort));
              }}
            >
              {effortLevels.map((level) => (
                <SegmentedControlItem key={level} value={level} label={effortName(level)} />
              ))}
            </SegmentedControl>
          )}
        </Row>
        <Row title="Mode">
          <SegmentedControl
            label="My default mode"
            size="sm"
            value={pressedChoice(storedMode, DEFAULT_MODE) ?? DEFAULT_MODE}
            onChange={(value) => {
              if (!busy) void save("mode", choiceToStore(value, DEFAULT_MODE));
            }}
          >
            {modes.map((m) => (
              <SegmentedControlItem key={m} value={m} label={MODE_LABELS[m]} />
            ))}
          </SegmentedControl>
        </Row>
        <Row title="Subagent model">
          <Selector
            label="My default subagent model"
            isLabelHidden
            size="sm"
            variant="secondary"
            width={WIDTH}
            isDisabled={busy}
            value={runDefaultValue(data.defaults, "subagent_model")}
            options={modelOptions("Default · Sonnet")}
            onChange={(value) => void save("subagent_model", value)}
          />
        </Row>
      </Group>
      <Problem error={error} />
    </>
  );
}
