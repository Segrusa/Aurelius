// One label for "no choice for this run" (OPR-5 point 5): the board member whose
// login the work runs on decides the model, effort and mode, so a chip says whose
// default it is and what it comes to: "Your default · Opus 5.5", "Kim's default · High".

import type { UserRunDefaultsView } from "@aurelius/protocol";

/** The server keeps a model, effort and mode per board member. */
export const USER_RUN_DEFAULTS_FEATURE = "user_run_defaults";

/** The server reads another board member's run defaults (`user.run_defaults_of`). */
export const RUN_DEFAULTS_OF_FEATURE = "run_defaults_of";

/** The key of the menu entry that opens Settings › You › My defaults instead of choosing. */
export const MY_DEFAULTS_KEY = "__my_defaults";

/** Who started the work: the board member whose login its runs use. */
export interface RunLogin {
  id: string;
  username: string;
}

/** "Your default", or "Kim's default" when the work runs on another board member's login. */
export function whoseDefault(login: RunLogin | null | undefined, meId: string | null | undefined): string {
  return login && meId && login.id !== meId ? `${login.username}'s default` : "Your default";
}

/** The chip text without a choice: "Your default · Opus 5.5", or just "Your default" while the value is unknown. */
export function defaultLabel(whose: string, value: string | null | undefined): string {
  return value ? `${whose} · ${value}` : whose;
}

/** The first entry of a chip's menu: "Use your default", "Use Kim's default". */
export function resetLabel(whose: string): string {
  return whose === "Your default" ? "Use your default" : `Use ${whose}`;
}

/** What a run without a choice comes to, by name; null where the server does not say. */
export interface ResolvedDefaults {
  model: string | null;
  effort: string | null;
  mode: string;
}

/**
 * The values behind "Your default": the chosen model or the connection's, the chosen effort or the
 * model's own, the chosen mode or Ask (a manager still starts in Plan; the server decides that).
 */
export function resolvedDefaults(
  view: UserRunDefaultsView | null | undefined,
  modelName: (id: string) => string,
  effortName: (effort: string) => string,
  modeName: (mode: string) => string,
): ResolvedDefaults {
  const model = view?.model ?? null;
  const effort = view?.defaults.effort ?? view?.model_default_effort ?? null;
  return {
    model: model ? modelName(model) : null,
    effort: effort ? effortName(effort) : null,
    mode: modeName(view?.defaults.mode ?? "ask"),
  };
}

/** A chip's text: just the chosen value, else whose default with its value. */
export function chipLabel(chosen: string | null | undefined, labelOf: (v: string) => string, fallback: string): string {
  return chosen && chosen !== "default" ? labelOf(chosen) : fallback;
}
