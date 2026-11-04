// What the old desktop app left in its data folder and this client takes over, once. Read-only on that
// side. Panel widths, folded groups and project windows follow when those exist here.

import { SCREEN_USE_KEY } from "./screen-use";

export interface DesktopPrefs {
  uiJson: string | null;
  screenUseJson: string | null;
}

const ADOPTED = "desktop_prefs_adopted";

export function adoptDesktopPrefs(
  storage: { get(key: string): string | null; set(key: string, value: string): void },
  prefs: DesktopPrefs,
): void {
  if (storage.get(ADOPTED) !== null) return;
  storage.set(ADOPTED, "1");
  let ui: { appearance?: unknown; last_given_to?: unknown } = {};
  try {
    ui = JSON.parse(prefs.uiJson ?? "{}") ?? {};
  } catch {
    // A damaged file: nothing to take over.
  }
  const keep = (key: string, value: unknown) => {
    if (typeof value === "string" && storage.get(key) === null) storage.set(key, value);
  };
  if (ui.appearance === "light" || ui.appearance === "dark" || ui.appearance === "system")
    keep("appearance", ui.appearance);
  keep("last_given_to", ui.last_given_to);
  keep(SCREEN_USE_KEY, prefs.screenUseJson);
}
