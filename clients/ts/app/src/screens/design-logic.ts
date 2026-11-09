// The links to Claude Design artifacts: a workspace's styleguide and a ticket's design.

/** The server keeps both links. Without it the fields are not offered: it would refuse them. */
export const DESIGN_FEATURE = "design_links";

export const ARTIFACT_URL_MAX = 300;

export const ARTIFACT_HINT = "The link only works for people the artifact is shared with.";

export const ARTIFACT_PLACEHOLDER = "https://claude.ai/artifact/…";

const ARTIFACT_URL = /^https:\/\/claude\.ai\/(code\/)?artifact\/[A-Za-z0-9_-]+$/;

/** What is wrong with a link before it goes to the server, which checks the same; empty clears it. */
export function artifactProblem(text: string): string | null {
  const link = text.trim();
  if (link === "") return null;
  if (link.length > ARTIFACT_URL_MAX) return `A link is at most ${ARTIFACT_URL_MAX} characters.`;
  return ARTIFACT_URL.test(link)
    ? null
    : "Paste a Claude artifact link: https://claude.ai/artifact/… or https://claude.ai/code/artifact/…";
}
