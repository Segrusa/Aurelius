// The rules of the workspace settings page (ticket #81), kept out of the screen so they are tested.

/** The server refuses a longer workspace description. */
export const DESCRIPTION_MAX = 2000;

/** Save shows only for a real, non-empty change of the name. */
export function nameSavable(draft: string, saved: string): boolean {
  const name = draft.trim();
  return name !== "" && name !== saved;
}

/** The line under the description field. */
export function descriptionCounter(text: string): { text: string; tooLong: boolean } {
  const tooLong = text.length > DESCRIPTION_MAX;
  return {
    text: tooLong
      ? `Too long: ${text.length} of at most ${DESCRIPTION_MAX} characters.`
      : `${text.length} / ${DESCRIPTION_MAX}`,
    tooLong,
  };
}

/** Why started tickets of this workspace would wait for an owner, or null when someone picks them up.
 * `chosen` is the workspace's own dispatcher ("" for the company's). */
export function dispatcherWarning(
  chosen: string,
  companyDefault: string | null | undefined,
  employees: readonly { id: string; name: string; status: string }[],
): string | null {
  const id = chosen || companyDefault;
  if (!id) return "◆ No dispatcher. Started tickets wait in Todo until you choose an owner.";
  const who = employees.find((e) => e.id === id);
  if (who && who.status !== "active") {
    return `◆ ${who.name} is ${who.status}. Started tickets wait for you to choose an owner.`;
  }
  return null;
}

/** Archive goes only after the workspace's exact name is typed. */
export function archiveConfirmed(typed: string, name: string): boolean {
  return typed === name;
}
