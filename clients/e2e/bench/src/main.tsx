// The shape phase 4 will build, to learn now whether it can be smooth: Virtuoso, memoized markdown,
// and a streaming message that subscribes on its own so the rest of the chat does not re-render.

import { memo, useEffect, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import Markdown from "react-markdown";
import { Virtuoso } from "react-virtuoso";
import remarkGfm from "remark-gfm";

const renders = { list: 0, streaming: 0 };
(window as unknown as { renders: typeof renders }).renders = renders;

const body = (
  i: number,
) => `Message ${i} about **the deploy** with \`inline code\`, a [link](https://example.com) and a list:

- First point of message ${i}
- Second point, which is somewhat longer so that it wraps on a narrow column

| Key | Value |
| --- | --- |
| a | ${i} |

${i % 5 === 0 ? "```\nkubectl rollout restart deploy/api\nkubectl get pods\n```\n" : ""}`;

const MESSAGES = Array.from({ length: 2000 }, (_, i) => ({ id: i, text: body(i) }));
const PLUGINS = [remarkGfm];

const Message = memo(function Message({ text }: { text: string }) {
  renders.list++;
  return (
    <div style={{ padding: "8px 24px", borderBottom: "1px solid rgb(255 255 255 / 0.05)" }}>
      <Markdown remarkPlugins={PLUGINS}>{text}</Markdown>
    </div>
  );
});

let streamed = "";
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

function Streaming() {
  const text = useSyncExternalStore(subscribe, () => streamed);
  renders.streaming++;
  return (
    <div style={{ padding: "8px 24px" }}>
      <Markdown remarkPlugins={PLUGINS}>{text}</Markdown>
    </div>
  );
}

function App() {
  useEffect(() => {
    const timer = setInterval(() => {
      streamed += "word **bold** ";
      if (streamed.length > 4000) streamed = "";
      for (const listener of listeners) listener();
    }, 33);
    return () => clearInterval(timer);
  }, []);
  return (
    <Virtuoso
      style={{ height: "100%" }}
      data={MESSAGES}
      computeItemKey={(_, m) => m.id}
      itemContent={(_, m) => <Message text={m.text} />}
      components={{ Footer: Streaming }}
      increaseViewportBy={400}
    />
  );
}

createRoot(document.getElementById("root") as HTMLElement).render(<App />);
