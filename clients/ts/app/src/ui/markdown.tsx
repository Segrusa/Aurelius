import { Children, isValidElement, memo, type ReactElement, type ReactNode, useState } from "react";
import ReactMarkdown, { defaultUrlTransform } from "react-markdown";
import remarkGfm from "remark-gfm";
import { usePlatform } from "../platform";
import { useFollowLink } from "../screens/links";
import { Mermaid } from "./mermaid";

/** The sanitizer knows web addresses; ours go to the app, not to the browser. */
const keepAppLinks = (url: string) => (url.startsWith("aurelius://") ? url : defaultUrlTransform(url));

function AppLink({ href, children }: { href: string; children: React.ReactNode }) {
  const follow = useFollowLink();
  if (follow === null) return <>{children}</>;
  return (
    <button
      type="button"
      onClick={() => follow(href)}
      className="cursor-pointer border-0 bg-transparent p-0 text-acc-text underline"
    >
      {children}
    </button>
  );
}

function textOf(node: ReactNode): string {
  if (typeof node === "string") return node;
  if (Array.isArray(node)) return node.map(textOf).join("");
  return isValidElement(node) ? textOf((node as ReactElement<{ children?: ReactNode }>).props.children) : "";
}

function Code({ children, diagrams }: { children: ReactNode; diagrams: boolean }) {
  const platform = usePlatform();
  const [copied, setCopied] = useState(false);
  const code = Children.toArray(children)[0];
  const className = isValidElement(code) ? (code as ReactElement<{ className?: string }>).props.className : undefined;
  const text = textOf(children).replace(/\n$/, "");
  const block = (
    <div className="group relative">
      <pre>{children}</pre>
      <button
        type="button"
        onClick={() => {
          platform
            .copy(text)
            .then(() => {
              setCopied(true);
              setTimeout(() => setCopied(false), 1200);
            })
            .catch(() => undefined);
        }}
        className="absolute top-[6px] right-[6px] cursor-pointer rounded-[5px] border border-line bg-panel px-[6px] py-[2px] text-[11px] text-text2 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
  if (diagrams && /(^|\s)language-mermaid(\s|$)/i.test(className ?? ""))
    return <Mermaid source={text} fallback={block} />;
  return block;
}

/**
 * Markdown in the design's type. No raw HTML, and react-markdown drops unsafe link schemes. A link opens
 * a new window, which the Electron shell hands to the browser; `#@Name` is a mention and goes nowhere.
 * `live`: still being written, so no diagrams yet.
 */
export const Markdown = memo(function Markdown({
  text,
  live = false,
  safe = false,
}: {
  text: string;
  live?: boolean;
  safe?: boolean;
}) {
  return (
    <div className="prose-aurelius">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        urlTransform={keepAppLinks}
        components={{
          a: ({ href, children }) =>
            href?.startsWith("#") ? (
              <span className="mention">{children}</span>
            ) : href?.startsWith("aurelius://") ? (
              <AppLink href={href}>{children}</AppLink>
            ) : (
              <a href={href} target="_blank" rel="noreferrer">
                {children}
              </a>
            ),
          pre: ({ children }) => <Code diagrams={!live && !safe}>{children}</Code>,
          // Nothing in a guideline loads by itself.
          ...(safe
            ? { img: ({ alt }: { alt?: string | undefined }) => <span className="text-text3">[{alt || "image"}]</span> }
            : {}),
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});
