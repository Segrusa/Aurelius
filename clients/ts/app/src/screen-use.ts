// A local screen counter (ticket #19): how often each screen opens on this device. Only counts. It
// never leaves this device, and the keys are fixed names without ids. Port of screen_use.rs.

export const SCREEN_USE_KEY = "screen_use";
const MAX_SCREENS = 64;
const KEY_MAX = 48;
export const SAVE_DELAY_MS = 5000;

export interface Use {
  opens: number;
  first_ms: number;
  last_ms: number;
}

export type ScreenUse = Record<string, Use>;

/** Counts one open, unless it is the screen that was already showing. Returns whether it counted. */
export function noteScreen(use: ScreenUse, previous: string | null, key: string, nowMs: number): boolean {
  if (previous === key || key === "" || key.length > KEY_MAX) return false;
  const known = use[key];
  if (!known && Object.keys(use).length >= MAX_SCREENS) return false;
  use[key] = known
    ? { ...known, opens: known.opens + 1, last_ms: nowMs }
    : { opens: 1, first_ms: nowMs, last_ms: nowMs };
  return true;
}

/** Whatever a damaged value holds, only well-formed rows come back. */
export function parseScreenUse(text: string | null): ScreenUse {
  const out: ScreenUse = {};
  if (text === null) return out;
  try {
    const screens = (JSON.parse(text) as { screens?: Record<string, Partial<Use>> }).screens ?? {};
    for (const [key, u] of Object.entries(screens)) {
      if (
        key.length <= KEY_MAX &&
        typeof u?.opens === "number" &&
        typeof u.first_ms === "number" &&
        typeof u.last_ms === "number"
      ) {
        out[key] = { opens: u.opens, first_ms: u.first_ms, last_ms: u.last_ms };
      }
    }
  } catch {
    return {};
  }
  return out;
}

export function screenUseJson(use: ScreenUse): string {
  return JSON.stringify({ screens: use });
}

/** "/inbox" → "inbox", "/settings/app" → "settings.app". */
export function screenKey(pathname: string): string {
  return pathname.split("/").filter(Boolean).join(".");
}
