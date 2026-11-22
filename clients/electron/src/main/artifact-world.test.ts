import { describe, expect, it } from "vitest";
import { type SelectorNode, selectorOf, stopPickInWorld, worldCall } from "./artifact-world";

/** A small tree of fake elements: `tree("body", [["main", [["button"], ["button"]]]])`. */
type Spec = [tag: string, children?: Spec[], id?: string];
function build(spec: Spec, parent: SelectorNode | null = null): SelectorNode {
  const [tag, children = [], id = ""] = spec;
  const node: SelectorNode = {
    tagName: tag.toUpperCase(),
    id,
    parentElement: parent,
    previousElementSibling: null,
    nextElementSibling: null,
  };
  const kids = children.map((c) => build(c, node));
  kids.forEach((k, i) => {
    k.previousElementSibling = kids[i - 1] ?? null;
    k.nextElementSibling = kids[i + 1] ?? null;
  });
  (node as SelectorNode & { kids: SelectorNode[] }).kids = kids;
  return node;
}
const kid = (node: SelectorNode, ...path: number[]) =>
  path.reduce((n, i) => (n as SelectorNode & { kids: SelectorNode[] }).kids[i] as SelectorNode, node);

describe("selectorOf", () => {
  const html = build(["html", [["head"], ["body", [["main", [["h1"], ["button"], ["p"], ["button"]]]]]]]);
  const always = () => true;

  it("counts steps from body, with nth-of-type only where a tag has siblings of its type", () => {
    expect(selectorOf(kid(html, 1, 0, 1), always, 300)).toBe("body > main > button:nth-of-type(1)");
    expect(selectorOf(kid(html, 1, 0, 3), always, 300)).toBe("body > main > button:nth-of-type(2)");
    expect(selectorOf(kid(html, 1, 0, 0), always, 300)).toBe("body > main > h1");
  });

  it("starts from the nearest ancestor with a plain id that is unique", () => {
    const tree = build(["html", [["body", [["section", [["div", [["button"]]]], "plans"]]]]]);
    const button = kid(tree, 0, 0, 0, 0);
    expect(selectorOf(button, always, 300)).toBe("#plans > div > button");
    expect(selectorOf(button, () => false, 300)).toBe("body > section > div > button");
  });

  it("never uses an id it would have to escape", () => {
    const tree = build(["html", [["body", [["div", [], 'a"] , b[id="x']]]]]);
    expect(selectorOf(kid(tree, 0, 0), always, 300)).toBe("body > div");
  });

  it("keeps the steps nearest to the element when it would be too long", () => {
    let spec: Spec = ["span"];
    for (let i = 0; i < 60; i++) spec = ["div", [spec, ["div"]]];
    const tree = build(["html", [["body", [spec]]]]);
    let deep = kid(tree, 0, 0);
    while ((deep as SelectorNode & { kids: SelectorNode[] }).kids.length > 0) deep = kid(deep, 0);
    const got = selectorOf(deep, always, 300);
    expect(got.length).toBeLessThanOrEqual(300);
    expect(got.endsWith("> span")).toBe(true);
    expect(got.startsWith("div")).toBe(true);
  });
});

describe("worldCall", () => {
  it("passes arguments as JSON, so nothing is spliced in as code", () => {
    const code = worldCall((a: never) => a, '"); alert(1); ("');
    expect(code).toContain(JSON.stringify('"); alert(1); ("'));
    expect(new Function(`return ${code}`)()).toBe('"); alert(1); ("');
  });

  it("passes a function argument as its source", () => {
    const code = worldCall(((f: (x: number) => number) => f(2)) as never, (x: number) => x * 3);
    expect(new Function(`return ${code}`)()).toBe(6);
  });

  it("runs a world function on its own, as its source", () => {
    const scope = globalThis as unknown as { __aureliusPick?: { stop: (v: unknown) => void } };
    let stopped: unknown = "no";
    scope.__aureliusPick = { stop: (v) => (stopped = v) };
    expect(new Function(`return ${worldCall(stopPickInWorld)}`)()).toBe(true);
    expect(stopped).toBeNull();
    delete scope.__aureliusPick;
    expect(new Function(`return ${worldCall(stopPickInWorld)}`)()).toBe(false);
  });
});
