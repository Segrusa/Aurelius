import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

/**
 * Markdown in the design's type. No raw HTML, and react-markdown drops unsafe link schemes. A link opens
 * a new window, which the Electron shell hands to the browser.
 */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="prose-aurelius">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noreferrer">
              {children}
            </a>
          ),
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});
