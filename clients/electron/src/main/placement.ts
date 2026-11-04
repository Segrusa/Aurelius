// Where a project window goes back to (clients/desktop/src/project_window.rs). A window keeps its
// place if its title strip can still be grabbed; otherwise it moves onto a screen that exists.

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export const MIN_WIDTH = 900;
export const MIN_HEIGHT = 640;
const TITLE_STRIP = 32;
const MIN_VISIBLE = 100;

function overlap(a: Box, b: Box): number {
  const width = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
  const height = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
  return width > 0 && height > 0 ? width * height : 0;
}

function contains(area: Box, x: number, y: number): boolean {
  return x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height;
}

function clamped(bounds: Box, area: Box): Box {
  const width = Math.min(Math.max(bounds.width, MIN_WIDTH), area.width);
  const height = Math.min(Math.max(bounds.height, MIN_HEIGHT), area.height);
  return {
    width,
    height,
    x: Math.min(Math.max(bounds.x, area.x), area.x + area.width - width),
    y: Math.min(Math.max(bounds.y, area.y), area.y + area.height - height),
  };
}

function centered(bounds: Box, area: Box): Box {
  const width = Math.min(bounds.width, area.width);
  const height = Math.min(bounds.height, area.height);
  return {
    width,
    height,
    x: area.x + Math.round((area.width - width) / 2),
    y: area.y + Math.round((area.height - height) / 2),
  };
}

/** `own`: the window is on a screen it knew, so it may also stay maximized. */
export function placement(saved: Box, areas: Box[], fallback: Box): { bounds: Box; own: boolean } {
  const strip = { x: saved.x, y: saved.y, width: saved.width, height: TITLE_STRIP };
  const middleX = saved.x + saved.width / 2;
  const middleY = saved.y + saved.height / 2;
  const home =
    areas.find((a) => contains(a, middleX, middleY)) ??
    areas.find((a) => overlap(strip, a) >= MIN_VISIBLE * TITLE_STRIP);
  if (home === undefined) return { bounds: centered(saved, fallback), own: false };
  const fits =
    saved.width >= MIN_WIDTH && saved.height >= MIN_HEIGHT && saved.width <= home.width && saved.height <= home.height;
  const stripVisible = overlap(strip, home) >= MIN_VISIBLE * TITLE_STRIP;
  return { bounds: fits && stripVisible ? saved : clamped(saved, home), own: true };
}
