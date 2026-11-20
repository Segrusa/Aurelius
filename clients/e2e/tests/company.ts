import { expect, type Page } from "@playwright/test";

/**
 * Opens Settings from the sidebar: through the signed-in board member's menu at its foot, or the
 * plain Settings row when nobody is signed in.
 */
export async function openSettings(page: Page): Promise<void> {
  const main = page.getByRole("navigation", { name: "Main" });
  const menu = main.getByRole("button", { name: /settings and account$/ });
  if (await menu.isVisible()) {
    await menu.click();
    await page.getByRole("menuitem", { name: /^Settings/ }).click();
  } else {
    await main.getByRole("link", { name: "Settings", exact: true }).click();
  }
}

/**
 * Opens Employees or Knowledge, which live under Settings › Company: from the app through the
 * Settings row, from inside Settings straight from its sidebar.
 */
export async function openCompanyPage(page: Page, name: "Employees" | "Knowledge"): Promise<void> {
  const main = page.getByRole("navigation", { name: "Main" });
  const settings = page.getByRole("navigation", { name: "Settings" });
  await expect(main.or(settings)).toBeVisible();
  if (!(await settings.isVisible())) await openSettings(page);
  await settings.getByRole("link", { name, exact: true }).click();
}
