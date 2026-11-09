import { CodeBlock } from "@astryxdesign/core/CodeBlock";
import { Markdown as AstryxMarkdown } from "@astryxdesign/core/Markdown";
import {
  createMarkdownFenceTransform,
  createMarkdownPlugin,
  type MarkdownExtensionNode,
} from "@astryxdesign/core/Markdown/plugins";
import * as stylex from "@stylexjs/stylex";
import { memo, type ReactNode } from "react";
import { useFollowLink } from "../screens/links";
import { Mermaid } from "./mermaid";

const styles = stylex.create({
  // The design's reading type and its tighter block spacing, set as the tokens Astryx reads for every block.
  prose: {
    "--text-body-size": "14px",
    "--text-body-leading": "1.65",
    "--text-code-size": "12px",
    "--text-heading-1-size": "14px",
    "--text-heading-2-size": "14px",
    "--text-heading-3-size": "14px",
    "--text-heading-4-size": "14px",
    "--text-heading-5-size": "14px",
    "--text-heading-6-size": "14px",
    "--spacing-3": "10px",
    "--spacing-4": "10px",
    "--spacing-6": "14px",
  },
  link: { color: "var(--color-text-accent)", textDecoration: "none" },
  appLink: {
    cursor: "pointer",
    borderWidth: 0,
    backgroundColor: "transparent",
    padding: 0,
    color: "var(--color-text-accent)",
    textDecoration: "underline",
  },
  image: { maxWidth: "100%", borderRadius: 6 },
  noImage: { color: "var(--precision-text3)" },
  // A fence a plugin claims gets no spacing from Markdown itself.
  diagram: {
    marginBlockStart: { default: 10, ":first-child": 0 },
    marginBlockEnd: { default: 10, ":last-child": 0 },
  },
});

/** Worked out once: a Markdown is drawn for every visible message. */
const PROSE = stylex.props(styles.prose);
const DIAGRAM = stylex.props(styles.diagram);

/**
 * What the sanitizer let through: no scheme, a web address or mail, and our own links.
 * Astryx only blocks script-like schemes, and a link here opens outside the app.
 */
function allowedUrl(url: string): boolean {
  if (url.startsWith("aurelius://")) return true;
  const colon = url.indexOf(":");
  if (colon === -1) return true;
  // A colon after one of these is part of a path or query, not a scheme.
  if (["/", "?", "#"].some((ch) => url.includes(ch) && colon > url.indexOf(ch))) return true;
  return /^(https?|ircs?|mailto|xmpp)$/i.test(url.slice(0, colon));
}

function AppLink({ href, children }: { href: string; children: ReactNode }) {
  const follow = useFollowLink();
  if (follow === null) return <>{children}</>;
  return (
    <button type="button" onClick={() => follow(href)} {...stylex.props(styles.appLink)}>
      {children}
    </button>
  );
}

/** A link opens a new window, which the Electron shell hands to the browser; `#@Name` is a mention and goes nowhere. */
function MarkdownLink({ href, children }: { href: string; children: ReactNode }) {
  if (href.startsWith("#")) return <span className="mention">{children}</span>;
  if (href.startsWith("aurelius://")) return <AppLink href={href}>{children}</AppLink>;
  if (!allowedUrl(href)) return <>{children}</>;
  return (
    <a href={href} target="_blank" rel="noreferrer" {...stylex.props(styles.link)}>
      {children}
    </a>
  );
}

function NoImage({ alt }: { alt: string }) {
  return <span {...stylex.props(styles.noImage)}>[{alt || "image"}]</span>;
}

function MarkdownImage({ src, alt }: { src: string; alt: string }) {
  if (!allowedUrl(src)) return <NoImage alt={alt} />;
  return <img src={src} alt={alt} {...stylex.props(styles.image)} />;
}

const PARTS = { link: MarkdownLink, image: MarkdownImage };
// Nothing in a guideline loads by itself.
const SAFE_PARTS = { link: MarkdownLink, image: NoImage };

type DiagramNode = MarkdownExtensionNode<"diagrams", "diagram", { readonly code: string }, "block">;

const diagrams = createMarkdownPlugin<"diagrams", DiagramNode>({
  name: "diagrams",
  apiVersion: 1,
  transform: createMarkdownFenceTransform({
    languages: ["mermaid", "Mermaid"],
    createNode: ({ code }) => ({
      type: "extension",
      plugin: "diagrams",
      name: "diagram",
      display: "block",
      data: { code },
    }),
  }),
  renderers: {
    diagram: {
      render: ({ node }) => (
        <div {...DIAGRAM}>
          <Mermaid
            source={node.data.code}
            fallback={<CodeBlock code={node.data.code} language="mermaid" isWrapped width="100%" />}
          />
        </div>
      ),
      toText: (node) => node.data.code,
    },
  },
});

const WITH_DIAGRAMS = { plugins: [diagrams] };

/**
 * Markdown in the design's type. No raw HTML, and links and images only go where `allowedUrl` says.
 * `live`: still being written, so no diagrams yet. `streaming`: the block text is arriving in right now.
 */
export const Markdown = memo(function Markdown({
  text,
  live = false,
  safe = false,
  streaming = false,
}: {
  text: string;
  live?: boolean;
  safe?: boolean;
  streaming?: boolean;
}) {
  return (
    <AstryxMarkdown
      autolink="gfm"
      contentWidth="100%"
      components={safe ? SAFE_PARTS : PARTS}
      isStreaming={streaming}
      {...(live || safe ? {} : WITH_DIAGRAMS)}
      {...PROSE}
    >
      {text}
    </AstryxMarkdown>
  );
});
