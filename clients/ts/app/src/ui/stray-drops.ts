// A file or link dropped where nothing takes it makes the browser open it in the window. Electron
// blocks that page change, but it still tore down the link to the server; a browser tab would
// leave the app altogether. So a drop nothing handles is refused for the whole window.

/** True when a drag of these `types` must be refused here: nothing took it and the browser would open it. */
export function strayDrop(types: readonly string[], handled: boolean, editable: boolean): boolean {
  if (handled) return false;
  if (types.includes("Files")) return true;
  // A dragged link in a text field is its text; anywhere else the browser would follow it.
  return types.includes("text/uri-list") && !editable;
}

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;
}

/** Refuses stray drops in `win`; after the page's own handlers, which run first and mark what they take. */
export function guardStrayDrops(win: Window): void {
  const refuse = (event: DragEvent) => {
    const types = [...(event.dataTransfer?.types ?? [])];
    if (!strayDrop(types, event.defaultPrevented, isEditable(event.target))) return;
    event.preventDefault();
    if (event.dataTransfer && event.type === "dragover") event.dataTransfer.dropEffect = "none";
  };
  win.addEventListener("dragover", refuse);
  win.addEventListener("drop", refuse);
}
