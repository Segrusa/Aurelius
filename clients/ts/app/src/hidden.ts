// Features this client keeps out of its menus, tabs, buttons, shortcuts and the command palette
// (ticket #19): the measured usage was zero. Nothing is removed: the server's data and the protocol
// stay, so showing a feature again is deleting its line here. Port of clients/desktop/src/hidden.rs.

export type Feature =
  | "Notes"
  | "Shares"
  | "TaskVariants"
  | "TaskDependencies"
  | "Artifacts"
  | "Heartbeats"
  | "AppKeys"
  | "ApiKeys"
  | "Plugins"
  | "ChatAttachments";

/** The one list. Webhooks, agent functions, the vault and recurring tasks are not in it. */
export const HIDDEN_FEATURES: readonly Feature[] = [
  "Notes",
  "Shares",
  "TaskVariants",
  "TaskDependencies",
  "Artifacts",
  "Heartbeats",
  "AppKeys",
  "ApiKeys",
  "Plugins",
  "ChatAttachments",
];

export function hidden(feature: Feature): boolean {
  return HIDDEN_FEATURES.includes(feature);
}
