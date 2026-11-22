// The code main runs in an isolated world of an artifact's view, for comments and tweaks. That
// world shares the page's DOM but not its JavaScript: the artifact cannot reach its globals or
// change its prototypes (an `EventTarget.prototype.addEventListener` the page replaced is the
// page's own). Main sends each function as source with JSON arguments and only reads the promise's
// result; the artifact gets no IPC, bridge or channel. Every function here may use nothing from
// outside its own body.

/** The isolated world of the viewer's tools; the export window reads its height in another. */
export const TOOLS_WORLD_ID = 1_080;

/** What a picking world gets from main: the limits it keeps itself to. */
export interface PickOptions {
  selectorMax: number;
  textMax: number;
}

/** The parts of an element selectorOf reads, so it can be tested without a DOM. */
export interface SelectorNode {
  tagName: string;
  id: string;
  parentElement: SelectorNode | null;
  previousElementSibling: SelectorNode | null;
  nextElementSibling: SelectorNode | null;
}

/**
 * A CSS selector for `element`: from the nearest ancestor with an id that is unique (by
 * `unique`) and plain (letters, digits, `-`, `_`), else from `body`, as `tag:nth-of-type(n)`
 * steps (without `:nth-of-type` for an only child of its type). Over `max` characters it keeps
 * the steps nearest to the element: still a selector, only a less precise one.
 */
export function selectorOf(element: SelectorNode, unique: (id: string) => boolean, max: number): string {
  const steps: string[] = [];
  for (let node: SelectorNode | null = element; node !== null; node = node.parentElement) {
    const tag = node.tagName.toLowerCase();
    if (tag === "html") break;
    if (/^[A-Za-z][\w-]{0,60}$/.test(node.id) && unique(node.id)) {
      steps.unshift(`#${node.id}`);
      break;
    }
    if (tag === "body") {
      steps.unshift("body");
      break;
    }
    let before = 0;
    let after = false;
    for (let s = node.previousElementSibling; s !== null; s = s.previousElementSibling)
      if (s.tagName === node.tagName) before++;
    for (let s = node.nextElementSibling; s !== null && !after; s = s.nextElementSibling)
      if (s.tagName === node.tagName) after = true;
    steps.unshift(before === 0 && !after ? tag : `${tag}:nth-of-type(${before + 1})`);
  }
  while (steps.length > 1 && steps.join(" > ").length > max) steps.shift();
  const out = steps.join(" > ");
  return out.length > max ? out.slice(-max) : out;
}

/**
 * Comment mode in the world: outlines the element under the pointer and resolves with the one the
 * board member clicks, once. Only real input counts (`isTrusted`): an event the page dispatches is
 * ignored, so a page can at most decide what lies under the pointer. The click and the presses
 * around it are taken in the capture phase, so the artifact does not act on them. The outline is a
 * box of the world's own, gone when the pick ends or is stopped (`__aureliusPick.stop`, null).
 */
export function pickInWorld(options: PickOptions, selector: typeof selectorOf): Promise<unknown> {
  const scope = globalThis as unknown as { __aureliusPick?: { stop: (value: unknown) => void } };
  scope.__aureliusPick?.stop(null);
  return new Promise((resolve) => {
    const box = document.createElement("div");
    box.style.cssText =
      "position:fixed;left:0;top:0;width:0;height:0;display:none;pointer-events:none;z-index:2147483647;" +
      "box-sizing:border-box;border:2px solid #1f6feb;background:rgba(31,111,235,0.12);border-radius:3px";
    document.documentElement.appendChild(box);
    const outline = (target: EventTarget | null) => {
      if (!(target instanceof Element)) return;
      const r = target.getBoundingClientRect();
      box.style.display = "block";
      box.style.left = `${r.left}px`;
      box.style.top = `${r.top}px`;
      box.style.width = `${r.width}px`;
      box.style.height = `${r.height}px`;
    };
    const describe = (target: EventTarget | null) => {
      if (!(target instanceof Element)) return null;
      const html = target instanceof HTMLElement ? target : null;
      const said =
        // The text the board member sees, not hidden or styled-away text: layout is what counts here.
        // biome-ignore lint/performance/useDomNodeTextContent: rendered text is the point (see above).
        html?.innerText ||
        target.getAttribute("aria-label") ||
        target.getAttribute("alt") ||
        target.getAttribute("title") ||
        target.textContent ||
        "";
      const r = target.getBoundingClientRect();
      return {
        selector: selector(
          target as unknown as SelectorNode,
          (id) => document.querySelectorAll(`[id="${id}"]`).length === 1,
          options.selectorMax,
        ),
        tag: target.tagName.toLowerCase(),
        text: said.replace(/\s+/g, " ").trim().slice(0, options.textMax),
        rect: { x: r.left, y: r.top, width: r.width, height: r.height },
      };
    };
    const over = (event: Event) => {
      if (event.isTrusted) outline(event.target);
    };
    const swallow = (event: Event) => {
      if (!event.isTrusted) return;
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    const click = (event: Event) => {
      if (!event.isTrusted) return;
      swallow(event);
      finish(describe(event.target));
    };
    const pressed = ["pointerdown", "pointerup", "mousedown", "mouseup", "auxclick", "dblclick", "contextmenu"];
    const finish = (value: unknown) => {
      removeEventListener("pointermove", over, true);
      removeEventListener("click", click, true);
      for (const type of pressed) removeEventListener(type, swallow, true);
      box.remove();
      if (scope.__aureliusPick?.stop === finish) delete scope.__aureliusPick;
      resolve(value);
    };
    scope.__aureliusPick = { stop: finish };
    addEventListener("pointermove", over, true);
    addEventListener("click", click, true);
    for (const type of pressed) addEventListener(type, swallow, true);
  });
}

/** Ends comment mode in the world, if it is on: the outline goes and the pick resolves with null. */
export function stopPickInWorld(): boolean {
  const scope = globalThis as unknown as { __aureliusPick?: { stop: (value: unknown) => void } };
  const on = scope.__aureliusPick !== undefined;
  scope.__aureliusPick?.stop(null);
  return on;
}

/**
 * The tweaks the author declared: the text of `<script type="application/json" id=…>`, at most
 * `max` + 1 characters (main refuses more). Null when there is no such element.
 */
export function readTweaksInWorld(id: string, max: number): string | null {
  const element = document.getElementById(id);
  if (!(element instanceof HTMLScriptElement)) return null;
  if (element.type.trim().toLowerCase() !== "application/json") return null;
  return (element.textContent ?? "").slice(0, max + 1);
}

/** The computed value of each custom property on the document element. */
export function valuesInWorld(names: string[]): string[] {
  const style = getComputedStyle(document.documentElement);
  return names.map((name) => style.getPropertyValue(name));
}

/**
 * Sets custom properties on the document element; the first time a property is set, its own inline
 * value is kept, for a reset.
 */
export function setTweaksInWorld(values: [string, string][]): void {
  const scope = globalThis as unknown as { __aureliusTweaks?: Map<string, string> };
  scope.__aureliusTweaks ??= new Map();
  const style = document.documentElement.style;
  for (const [name, value] of values) {
    if (!scope.__aureliusTweaks.has(name)) scope.__aureliusTweaks.set(name, style.getPropertyValue(name));
    style.setProperty(name, value);
  }
}

/** Puts back what every tweak set: the inline value the document element had, or none. */
export function resetTweaksInWorld(): void {
  const scope = globalThis as unknown as { __aureliusTweaks?: Map<string, string> };
  const style = document.documentElement.style;
  for (const [name, before] of scope.__aureliusTweaks ?? []) {
    if (before === "") style.removeProperty(name);
    else style.setProperty(name, before);
  }
  scope.__aureliusTweaks?.clear();
}

/**
 * The source that calls `fn` with `args` in the world. Arguments are JSON, and a function argument
 * is its source: nothing of the page or of the app is spliced in as code.
 */
export function worldCall(fn: (...args: never[]) => unknown, ...args: unknown[]): string {
  const values = args.map((a) => (typeof a === "function" ? `(${a.toString()})` : JSON.stringify(a)));
  return `(${fn.toString()})(${values.join(", ")})`;
}
