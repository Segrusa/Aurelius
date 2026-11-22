// What main accepts back from an artifact's isolated world (artifact-world.ts): a pick for a
// comment, and the tweaks an author declared. The world runs main's own code, so the page cannot
// change what it returns, but it reads the page's DOM: everything here is the page's own data and
// is checked and limited before it goes on to the app. Nothing is evaluated.

/** A selector main passes on; longer ones are refused, the world keeps its own shorter. */
export const PICK_SELECTOR_MAX = 300;
/** The text of a picked element, whitespace folded to single spaces. */
export const PICK_TEXT_MAX = 120;

/** The element the board member clicked in comment mode. */
export interface ArtifactPick {
  selector: string;
  tag: string;
  text: string;
  rect: { x: number; y: number; width: number; height: number };
}

// Control characters, newlines and the Unicode line separators: a pick and a tweak are one line of
// a chat message. Built from a string, so the source holds no such character itself.
const CONTROL_CLASS = "[\\u0000-\\u001f\\u007f-\\u009f\\u2028\\u2029]";
const CONTROL = new RegExp(CONTROL_CLASS);
const CONTROL_ALL = new RegExp(CONTROL_CLASS, "g");

/** A pick as the world reported it, or null when it is not one (or not within the limits). */
export function checkPick(raw: unknown): ArtifactPick | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  const { selector, tag, text, rect } = r;
  if (typeof selector !== "string" || selector === "" || selector.length > PICK_SELECTOR_MAX) return null;
  if (CONTROL.test(selector)) return null;
  if (typeof tag !== "string" || !/^[a-z][a-z0-9-]{0,40}$/.test(tag)) return null;
  if (typeof text !== "string") return null;
  if (typeof rect !== "object" || rect === null) return null;
  const box = rect as Record<string, unknown>;
  const edges = [box.x, box.y, box.width, box.height];
  if (!edges.every((v): v is number => typeof v === "number" && Number.isFinite(v))) return null;
  const [x, y, width, height] = edges.map(Math.round) as [number, number, number, number];
  return { selector, tag, text: oneLine(text, PICK_TEXT_MAX), rect: { x, y, width, height } };
}

/** `text` on one line: control characters and runs of whitespace become one space, then cut. */
export function oneLine(text: string, max: number): string {
  return text.replace(CONTROL_ALL, " ").replace(/\s+/g, " ").trim().slice(0, max).trim();
}

/** The element that holds an artifact's tweaks: JSON, which the policy never runs. */
export const TWEAKS_ELEMENT_ID = "aurelius-tweaks";
/** The most entries main reads; later ones are dropped. */
export const TWEAKS_MAX = 20;
/** The most JSON main reads from the element; more is ignored as a whole. */
export const TWEAKS_JSON_MAX = 20_000;
/** A label as the panel shows it. */
export const TWEAK_LABEL_MAX = 40;
export const TWEAK_CHOICES_MAX = 8;
export const TWEAK_CHOICE_MAX = 40;
/** A value main reads back with getComputedStyle, for the control's start. */
export const TWEAK_VALUE_MAX = 100;
/** The units a length tweak may have. */
export const TWEAK_UNITS = ["px", "rem", "em", "%", "vh", "vw", "ch", "pt"] as const;

const VAR = /^--[a-z0-9-]{1,40}$/;

type Range = { min?: number; max?: number; step?: number };

/** One tweak an author declared, as main checked it. */
export type ArtifactTweak =
  | { var: string; label: string; type: "color" }
  | ({ var: string; label: string; type: "length"; unit: (typeof TWEAK_UNITS)[number] } & Range)
  | ({ var: string; label: string; type: "number" } & Range)
  | { var: string; label: string; type: "choice"; options: string[] };

/** The numbers of a range, or null when one of them is there and is not a finite number. */
function rangeOf(r: Record<string, unknown>): Range | null {
  const out: Range = {};
  for (const key of ["min", "max", "step"] as const) {
    const v = r[key];
    if (v === undefined) continue;
    if (typeof v !== "number" || !Number.isFinite(v)) return null;
    out[key] = v;
  }
  if (out.min !== undefined && out.max !== undefined && out.min > out.max) return null;
  if (out.step !== undefined && out.step <= 0) return null;
  return out;
}

/** One declared entry, or null: an invalid entry is dropped, never fixed up. */
export function checkTweak(raw: unknown): ArtifactTweak | null {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return null;
  const r = raw as Record<string, unknown>;
  if (typeof r.var !== "string" || !VAR.test(r.var)) return null;
  const named = typeof r.label === "string" ? oneLine(r.label, TWEAK_LABEL_MAX) : "";
  const label = named || r.var.slice(2);
  const base = { var: r.var, label };
  switch (r.type) {
    case "color":
      return { ...base, type: "color" };
    case "number": {
      const range = rangeOf(r);
      return range ? { ...base, type: "number", ...range } : null;
    }
    case "length": {
      const range = rangeOf(r);
      const unit = r.unit === undefined ? "px" : r.unit;
      if (!range || !TWEAK_UNITS.includes(unit as (typeof TWEAK_UNITS)[number])) return null;
      return { ...base, type: "length", unit: unit as (typeof TWEAK_UNITS)[number], ...range };
    }
    case "choice": {
      const options = r.options;
      if (!Array.isArray(options) || options.length === 0 || options.length > TWEAK_CHOICES_MAX) return null;
      const fine = options.every(
        (o): o is string =>
          typeof o === "string" && o.trim() !== "" && o.length <= TWEAK_CHOICE_MAX && !CONTROL.test(o),
      );
      return fine ? { ...base, type: "choice", options: [...options] } : null;
    }
    default:
      return null;
  }
}

/**
 * The tweaks in the element's text: the first TWEAKS_MAX entries of a JSON list, each checked on
 * its own, a variable named twice only once. Anything that is not such a list gives none.
 */
export function parseTweaks(json: unknown): ArtifactTweak[] {
  if (typeof json !== "string" || json.length > TWEAKS_JSON_MAX) return [];
  let list: unknown;
  try {
    list = JSON.parse(json);
  } catch {
    return [];
  }
  if (!Array.isArray(list)) return [];
  const seen = new Set<string>();
  const out: ArtifactTweak[] = [];
  for (const raw of list.slice(0, TWEAKS_MAX)) {
    const tweak = checkTweak(raw);
    if (tweak === null || seen.has(tweak.var)) continue;
    seen.add(tweak.var);
    out.push(tweak);
  }
  return out;
}

const NUMBER = /^-?(\d+(\.\d+)?|\.\d+)$/;
const inRange = (n: number, t: Range) => (t.min === undefined || n >= t.min) && (t.max === undefined || n <= t.max);

/**
 * The value main sets for `tweak`, or null when it is not one this tweak takes: a colour is a hex
 * colour, a length a number with the tweak's unit, a number a plain number (both within min and
 * max), a choice one of its options.
 */
export function tweakValue(tweak: ArtifactTweak, value: unknown): string | null {
  if (typeof value !== "string") return null;
  switch (tweak.type) {
    case "color":
      return /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(value) ? value.toLowerCase() : null;
    case "length": {
      if (!value.endsWith(tweak.unit)) return null;
      const n = value.slice(0, -tweak.unit.length);
      return NUMBER.test(n) && inRange(Number(n), tweak) ? `${Number(n)}${tweak.unit}` : null;
    }
    case "number":
      return NUMBER.test(value) && inRange(Number(value), tweak) ? String(Number(value)) : null;
    case "choice":
      return tweak.options.includes(value) ? value : null;
  }
}

/** A computed value read back for a control: one line, limited; anything else is empty. */
export function currentValue(raw: unknown): string {
  return typeof raw === "string" ? oneLine(raw, TWEAK_VALUE_MAX) : "";
}
