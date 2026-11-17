// A dialog with `padding={0}` keeps its title and buttons away from its edge: that zero reaches Astryx's
// header too, and only the eye caught it the first time.

import { expect, type Locator, test } from "@playwright/test";
import { serveStatic, WEB_DIST } from "./daemon";
import { scriptServer, ticket } from "./fake-server";

const SHOT = process.env.AURELIUS_DIALOG_SHOT;
const backlog = ticket("t1", 30, "backlog", { title: "Delete me" });
const script = {
  features: [],
  tickets: [backlog],
  details: { t1: { ticket: backlog, root_task: null, tasks: [], as_of_seq: 1 } },
};

async function expectRoom(dialog: Locator) {
  const box = await dialog.boundingBox();
  const title = await dialog.getByRole("heading").first().boundingBox();
  const button = await dialog.getByRole("button", { name: "Cancel" }).boundingBox();
  if (!box || !title || !button) throw new Error("the dialog did not lay out");
  expect(title.x - box.x).toBeGreaterThanOrEqual(12);
  expect(title.y - box.y).toBeGreaterThanOrEqual(8);
  expect(box.y + box.height - (button.y + button.height)).toBeGreaterThanOrEqual(8);
}

test("a confirm dialog keeps its title and buttons away from its edge", async ({ page }) => {
  const site = await serveStatic(WEB_DIST);
  try {
    await scriptServer(page, script);
    // The ticket's session: Backlog's Delete sits in the decision above the composer.
    await page.goto(`${site.origin}/board?ticket=t1`);
    await page.getByRole("region", { name: "Your decision" }).getByRole("button", { name: "Delete" }).click();
    const dialog = page.getByRole("dialog", { name: "Delete #30?" });
    await expect(dialog).toBeVisible();
    // It opens with a small scale, which would shrink what is measured.
    await expect(() => expectRoom(dialog)).toPass({ timeout: 3000 });
    if (SHOT) await page.screenshot({ path: SHOT });
  } finally {
    await site.close();
  }
});
