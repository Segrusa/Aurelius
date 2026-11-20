// What the user menu at the foot of the sidebar shows about the signed-in board member.

/** "max" → "Max", "max_20x" → "Max 20x": the Claude plan as a word next to the name; null when unknown. */
export function planLabel(plan: string | null | undefined): string | null {
  const words = (plan ?? "")
    .trim()
    .split(/[\s_-]+/)
    .filter(Boolean);
  if (words.length === 0) return null;
  return words.map((w, i) => (i === 0 ? w.charAt(0).toUpperCase() + w.slice(1) : w)).join(" ");
}
