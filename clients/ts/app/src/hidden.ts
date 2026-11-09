// Features this client keeps out of its menus, tabs, buttons, shortcuts and the command palette
// (ticket #19): the measured usage was zero. Nothing is removed: the server's data and the protocol
// stay, so showing a feature again is deleting its line here.

export type Feature = "ChatAttachments";

/** The one list. */
export const HIDDEN_FEATURES: readonly Feature[] = ["ChatAttachments"];

export function hidden(feature: Feature): boolean {
  return HIDDEN_FEATURES.includes(feature);
}
