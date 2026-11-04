import { Children, isValidElement, memo, type ReactElement, type ReactNode, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Mermaid } from "./mermaid";

function textOf(node: ReactNode): string {
  if (typeof node === "string") return node;
  if (Array.isArray(node)) return node.map(textOf).join("");
  return isValidElement(node) ? textOf((node as ReactElement<{ children?: ReactNode }>).props.children) : "";
}

function Code({ children, diagrams }: { children: ReactNode; diagrams: boolean }) {
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
          void navigator.clipboard?.writeText(text);
          setCopied(true);
          setTimeout(() => setCopied(false), 1200);
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
export const Markdown = memo(function Markdown({ text, live = false }: { text: string; live?: boolean }) {
  return (
    <div className="prose-aurelius">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) =>
            href?.startsWith("#") ? (
              <span className="mention">{children}</span>
            ) : (
              <a href={href} target="_blank" rel="noreferrer">
                {children}
              </a>
            ),
          pre: ({ children }) => <Code diagrams={!live}>{children}</Code>,
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});
