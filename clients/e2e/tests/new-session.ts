// Where the sidebar's + and the general New land since ticket #60, and the way left to Give work.

import { expect, type Page } from "@playwright/test";

/** The New session view's composer. The Give work dialog has a field of the same name: use it with no dialog open. */
export const sessionComposer = (page: Page) => page.getByRole("textbox", { name: "What needs doing" });

/** New session is open at /new, with the composer focused to type in. */
export async function expectNewSession(page: Page): Promise<void> {
  await expect(page).toHaveURL(/\/new$/);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(sessionComposer(page)).toBeFocused();
}

/** The Give work dialog has no key of its own any more: "Go to Tasks" in the palette, then its Give work button. */
export async function openGiveWork(page: Page): Promise<void> {
  await page.keyboard.press("Control+k");
  const palette = page.getByRole("combobox", { name: "Command palette" });
  await palette.fill("Go to Tasks");
  await page
    .getByRole("option", { name: /Go to Tasks/ })
    .first()
    .click();
  await expect(page).toHaveURL(/\/tasks$/);
  await page.getByRole("button", { name: "Give work" }).click();
  await expect(page.getByRole("dialog", { name: "Give work" })).toBeVisible();
}
