// Where the sidebar's + and the general New land since ticket #60, and the way left to Give work.

import { expect, type Page } from "@playwright/test";
import { openCompanyPage } from "./company";

/** The New session view's composer. The Give work dialog has a field of the same name: use it with no dialog open. */
export const sessionComposer = (page: Page) => page.getByRole("textbox", { name: "What needs doing" });

/** New session is open at /new, with the composer focused to type in. */
export async function expectNewSession(page: Page): Promise<void> {
  await expect(page).toHaveURL(/\/new$/);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(sessionComposer(page)).toBeFocused();
}

/** The Give work dialog has no key of its own: the Give work button on an employee's page (the Tasks list is gone). */
export async function openGiveWork(page: Page, employee: string): Promise<void> {
  await openCompanyPage(page, "Employees");
  await page
    .getByRole("link", { name: new RegExp(employee) })
    .first()
    .click();
  await page.getByRole("button", { name: "Give work", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Give work" })).toBeVisible();
}
