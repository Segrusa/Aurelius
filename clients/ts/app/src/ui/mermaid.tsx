import * as stylex from "@stylexjs/stylex";
import { useEffect, useId, useState } from "react";

const styles = stylex.create({
  image: { display: "block", maxWidth: "100%" },
});

let loaded: Promise<typeof import("mermaid").default> | null = null;

function mermaidFor(dark: boolean) {
  loaded ??= import("mermaid").then((m) => m.default);
  return loaded.then((mermaid) => {
    // htmlLabels off: a diagram is shown as an image, in which HTML labels do not draw.
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: dark ? "dark" : "default",
      flowchart: { htmlLabels: false },
    });
    return mermaid;
  });
}

/** A diagram as an image, so the page's content policy stays strict and it cannot run anything. */
export function Mermaid({ source, fallback }: { source: string; fallback: React.ReactNode }) {
  const id = useId().replace(/[^a-zA-Z0-9]/g, "");
  const [svg, setSvg] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const dark = document.documentElement.dataset.theme !== "light";
  useEffect(() => {
    let alive = true;
    void mermaidFor(dark)
      .then(async (mermaid) => {
        await mermaid.parse(source);
        const { svg: drawn } = await mermaid.render(`mermaid${id}`, source);
        if (alive) setSvg(drawn);
      })
      .catch(() => {
        document.getElementById(`dmermaid${id}`)?.remove();
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [source, id, dark]);
  if (failed) return fallback;
  if (svg === null) return fallback;
  return (
    <img
      alt="Mermaid diagram"
      {...stylex.props(styles.image)}
      src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`}
    />
  );
}
