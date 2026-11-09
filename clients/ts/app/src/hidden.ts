// Features this client keeps out of its menus, tabs, buttons, shortcuts and the command palette
// (ticket #19, and the simplification of 2026-11-09). Nothing is removed: the server's data and the
// protocol stay, so showing a feature again is deleting its line here.

export type Feature =
  | "ChatAttachments"
  | "TasksInSidebar"
  | "ScheduledInSidebar"
  | "ActivityInSidebar"
  | "Trace"
  | "RecurringTasks"
  | "Vault"
  | "EmployeeTools"
  | "Ideas"
  | "MemoryProposals"
  | "SequentialTasks"
  | "RunScratchpad";

/** The one list. */
export const HIDDEN_FEATURES: readonly Feature[] = [
  "ChatAttachments",
  "TasksInSidebar",
  "ScheduledInSidebar",
  "ActivityInSidebar",
  "Trace",
  "RecurringTasks",
  "Vault",
  "EmployeeTools",
  "Ideas",
  "MemoryProposals",
  "SequentialTasks",
  "RunScratchpad",
];

export function hidden(feature: Feature): boolean {
  return HIDDEN_FEATURES.includes(feature);
}

/** What is left of a list whose entries may belong to a hidden feature. */
export function shown<T extends { feature?: Feature }>(items: readonly T[]): T[] {
  return items.filter((item) => !(item.feature && hidden(item.feature)));
}
