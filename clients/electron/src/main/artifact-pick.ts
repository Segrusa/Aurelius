// What main accepts back from an artifact's isolated world (artifact-world.ts): a pick for a
// comment. The world runs main's own code, so the page cannot change what it returns, but it reads
// the page's DOM: everything here is the page's own data and is checked and limited before it goes
// on to the app. Nothing is evaluated.

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

// Control characters, newlines and the Unicode line separators: a pick is one line of a chat
// message. Built from a string, so the source holds no such character itself.
const CONTROL_CLASS = "[\\u0000-\\u001f\\u007f-\\u009f\\u2028\\u2029]";
const CONTROL = new RegExp(CONTROL_CLASS);
const CONTROL_ALL = new RegExp(CONTROL_CLASS, "g");

// The selectors selectorOf makes: `#id` or `body` to start, then tag or `*` steps with an
// optional `:nth-of-type(n)`/`:nth-child(n)`, joined by ` > `. Anything else in a selector would
// be page text in the board member's message.
const STEP = String.raw`(?:#[A-Za-z][\w-]{0,60}|body|(?:[a-z][a-z0-9-]{0,40}|\*)(?::nth-of-type\(\d{1,6}\)|:nth-child\(\d{1,6}\))?)`;
const SELECTOR = new RegExp(`^${STEP}(?: > ${STEP})*$`);

/** A pick as the world reported it, or null when it is not one (or not within the limits). */
export function checkPick(raw: unknown): ArtifactPick | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  const { selector, tag, text, rect } = r;
  if (typeof selector !== "string" || selector === "" || selector.length > PICK_SELECTOR_MAX) return null;
  if (CONTROL.test(selector) || !SELECTOR.test(selector)) return null;
  if (typeof tag !== "string" || !/^(?:[a-z][a-z0-9-]{0,40}|\*)$/.test(tag)) return null;
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
