import { expect, type Page } from "@playwright/test";

/**
 * Opens Settings from the sidebar's rail: through the signed-in board member's menu at its foot, or the
 * plain Settings button when nobody is signed in.
 */
export async function openSettings(page: Page): Promise<void> {
  const rail = page.getByRole("navigation", { name: "Workspaces" });
  const menu = rail.getByRole("button", { name: /settings and account$/ });
  if (await menu.isVisible()) {
    await menu.click();
    await page.getByRole("menuitem", { name: /^Settings/ }).click();
  } else {
    await rail.getByRole("button", { name: "Settings", exact: true }).click();
  }
}

/** The sidebar rail's tile of a workspace ("All workspaces" or a project's name). */
export function workspaceTile(page: Page, name: string) {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return page
    .getByRole("navigation", { name: "Workspaces" })
    .getByRole("button", { name: new RegExp(`^${escaped}(,| ·|$)`) });
}

/** Switches the workspace on the rail and waits until it is the current one. */
export async function chooseWorkspace(page: Page, name: string): Promise<void> {
  await workspaceTile(page, name).click();
  await expectWorkspace(page, name);
}

export async function expectWorkspace(page: Page, name: string): Promise<void> {
  await expect(workspaceTile(page, name)).toHaveAttribute("aria-current", "true");
}

/**
 * Opens Employees or Knowledge, which live under Settings › Company: from the app through the
 * Settings row, from inside Settings straight from its sidebar.
 */
export async function openCompanyPage(
  page: Page,
  name: "Employees" | "Workspaces" | "Guidelines" | "Memory",
): Promise<void> {
  const rail = page.getByRole("navigation", { name: "Workspaces" });
  const settings = page.getByRole("navigation", { name: "Settings" });
  await expect(rail.or(settings).first()).toBeVisible();
  if (!(await settings.isVisible())) await openSettings(page);
  await settings.getByRole("link", { name, exact: true }).click();
}
