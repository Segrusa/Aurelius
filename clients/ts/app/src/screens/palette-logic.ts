// What the palette lists for what is typed. Pure, and capped per group, so a large company stays instant.

export interface PaletteItem {
  id: string;
  label: string;
  detail: string;
  mark: string;
  /** A shortcut to show on the row. */
  keys?: string;
  run: () => void;
}

export interface PaletteGroup {
  label: string;
  items: PaletteItem[];
}

const PER_GROUP = 8;
const EXACT = 1000;

/** Higher is better; 0 is no match. Every word must be found. */
export function score(query: string, text: string): number {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return 1;
  const hay = text.toLowerCase();
  let total = 0;
  for (const word of words) {
    const at = hay.indexOf(word);
    if (at < 0) return 0;
    total += at === 0 ? 3 : hay[at - 1] === " " || hay[at - 1] === "#" ? 2 : 1;
  }
  return total;
}

export function filterGroups(query: string, groups: PaletteGroup[]): PaletteGroup[] {
  const out: PaletteGroup[] = [];
  for (const group of groups) {
    const exact = query.trim().toLowerCase();
    const items = group.items
      .map((item, index) => ({
        item,
        index,
        s: item.label.toLowerCase() === exact ? EXACT : score(query, `${item.label} ${item.detail}`),
      }))
      .filter((r) => r.s > 0)
      .sort((a, b) => b.s - a.s || a.index - b.index)
      .slice(0, PER_GROUP)
      .map((r) => r.item);
    if (items.length > 0) out.push({ label: group.label, items });
  }
  return out;
}
