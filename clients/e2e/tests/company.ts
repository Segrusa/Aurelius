import { expect, type Page } from "@playwright/test";

/**
 * Opens Employees or Knowledge, which live under Settings › Company: from the app through the
 * Settings row, from inside Settings straight from its sidebar.
 */
export async function openCompanyPage(page: Page, name: "Employees" | "Knowledge"): Promise<void> {
  const main = page.getByRole("navigation", { name: "Main" });
  const settings = page.getByRole("navigation", { name: "Settings" });
  await expect(main.or(settings)).toBeVisible();
  if (!(await settings.isVisible())) await main.getByRole("link", { name: "Settings", exact: true }).click();
  await settings.getByRole("link", { name, exact: true }).click();
}
