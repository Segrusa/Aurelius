import { Selector } from "@astryxdesign/core/Selector";
import { expectBody } from "@aurelius/client";
import type { UserRunDefaults } from "@aurelius/protocol";
import { useConnection, useLinkState } from "../connection";
import { useModels } from "./dialogs";
import { effortName, MODE_LABELS, MODES } from "./employee-logic";
import { Group, Muted, modelName, Problem, Row, useChange, useSetting } from "./settings-kit";
import {
  defaultModelLabel,
  effortOptions,
  type RunDefaultField,
  runDefaultValue,
  withRunDefault,
} from "./settings-logic";

const WIDTH = 260;

/** The server keeps a model, effort and mode per board member. */
export const USER_RUN_DEFAULTS_FEATURE = "user_run_defaults";

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
    models.map((m) => (m.key === "default" ? { value: "default", label: defaultLabel } : { value: m.key, label: m.label }));
  const effort = runDefaultValue(data.defaults, "effort");
  const efforts = data.supported_efforts ?? [];
  return (
    <>
      <Group title="Runs">
        <Row title="Model" hint="For the tickets you make and the sessions you start. Applies to new runs.">
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
        <Row
          title="Effort"
          hint={efforts.length === 0 ? "This model has no effort setting." : "How hard the model thinks."}
        >
          <Selector
            label="My default effort"
            isLabelHidden
            size="sm"
            variant="secondary"
            width={WIDTH}
            isDisabled={busy || efforts.length === 0}
            value={effort === "default" ? "inherit" : effort}
            options={effortOptions(
              efforts,
              data.model_default_effort ? `${effortName(data.model_default_effort)} (model's default)` : "Model's default",
              effort === "default" ? null : effort,
            )}
            onChange={(value) => void save("effort", value === "inherit" ? "default" : value)}
          />
        </Row>
        <Row title="Mode" hint="How a new conversation starts. Managers still start in Plan, a design in Auto.">
          <Selector
            label="My default mode"
            isLabelHidden
            size="sm"
            variant="secondary"
            width={WIDTH}
            isDisabled={busy}
            value={runDefaultValue(data.defaults, "mode")}
            options={[{ value: "default", label: "Default · Ask" }, ...MODES.map((m) => ({ value: m, label: MODE_LABELS[m] }))]}
            onChange={(value) => void save("mode", value)}
          />
        </Row>
        <Row title="Subagent model" hint="The model of the helpers a run starts itself.">
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
