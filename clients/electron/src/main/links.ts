// `aurelius://` from the operating system: an argument to this or a second process, or macOS's
// open-URL event. Only the form is checked here; the page knows what the link means.

const MAX_LENGTH = 2048;

export function linkOf(text: string): string | null {
  return text.startsWith("aurelius://") && text.length <= MAX_LENGTH ? text : null;
}

export function linkInArguments(args: readonly string[]): string | null {
  for (const arg of args) {
    const link = linkOf(arg);
    if (link !== null) return link;
  }
  return null;
}
