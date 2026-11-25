import { Selector } from "@astryxdesign/core/Selector";
import { expectBody } from "@aurelius/client";
import type { RunSettings } from "@aurelius/protocol";
import { useConnection, useLinkState } from "../connection";
import { asChoice } from "./employee-logic";
import { Group, Muted, modelName, Problem, Row, useChange, useSetting } from "./settings-kit";
import { defaultEffortLabel, effortOptions, employeeRunHint } from "./settings-logic";
import { USER_RUN_DEFAULTS_FEATURE } from "./settings-run-defaults";

const WIDTH = 260;

const withEffort = (settings: RunSettings, value: string): RunSettings => ({
  ...settings,
  effort: value === "inherit" ? { mode: "inherit" } : { mode: "value", value },
});

/** Every employee's model and effort; a change counts for new runs. */
export function EmployeeRunsSection() {
  const connection = useConnection();
  const { features } = useLinkState();
  const { busy, error, run } = useChange();
  const has = features.includes("employee_run_overview");
  const view = useSetting("employee_runs", async () =>
    has ? expectBody(await connection.call("employee.run_overview"), "employee_runs").value : null,
  );
  if (!has) return <Muted>This server is too old to show this. Update the server first.</Muted>;
  // Model, effort and mode belong to each board member now (You → My defaults).
  if (features.includes(USER_RUN_DEFAULTS_FEATURE))
    return <Muted>Model, effort and mode are set per board member now: see You → My defaults.</Muted>;
  const data = view.data;
  if (!data) return <Problem error={view.error} />;
  const save = (scope: { kind: "global" } | { kind: "employee"; id: string }, settings: RunSettings, value: string) =>
    run(() =>
      connection.write(
        "connection.set_defaults",
        { scope, purpose: "employee", settings: withEffort(settings, value) },
        { retry: false },
      ),
    );
  const companyEffort = asChoice(data.company.effort);
  return (
    <>
      <Group title="Company default">
        <Row
          title="Effort"
          hint={
            data.company_model
              ? `For employees without their own effort. Default model: ${modelName(data.company_model)}. Applies to new runs.`
              : "For employees without their own effort. Applies to new runs."
          }
        >
          <Selector
            label="Company default effort"
            isLabelHidden
            size="sm"
            variant="secondary"
            width={WIDTH}
            isDisabled={busy || data.company_supported_efforts.length === 0}
            value={companyEffort}
            options={effortOptions(
              data.company_supported_efforts,
              defaultEffortLabel({ company: {} }, [], data.company_model_default_effort),
              companyEffort === "inherit" ? null : companyEffort,
            )}
            onChange={(value) => void save({ kind: "global" }, data.company, value)}
          />
        </Row>
      </Group>
      <Group title="Employees">
        {data.employees.map((employee) => {
          const own = asChoice(employee.own.effort);
          return (
            <Row key={employee.employee_id} title={employee.name} hint={employeeRunHint(employee, modelName)}>
              <Selector
                label={`Effort of ${employee.name}`}
                isLabelHidden
                size="sm"
                variant="secondary"
                width={WIDTH}
                isDisabled={busy || employee.supported_efforts.length === 0}
                value={own}
                options={effortOptions(
                  employee.supported_efforts,
                  defaultEffortLabel(data, employee.supported_efforts, employee.model_default_effort),
                  own === "inherit" ? null : own,
                )}
                onChange={(value) => void save({ kind: "employee", id: employee.employee_id }, employee.own, value)}
              />
            </Row>
          );
        })}
      </Group>
      <Problem error={error} />
    </>
  );
}
