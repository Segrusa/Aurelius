// What the old app could do and the first screens left out: files in forms, who is responsible,
// the card menu, keys from any screen, pinned employees. Against a real service.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Connection, expectBody } from "@aurelius/client";
import { _electron, expect, type Locator, test } from "@playwright/test";
import { daemonEndpoint, openPipe } from "../../electron/src/main/pipe";
import { hidden } from "../../ts/app/src/hidden";
import { openCompanyPage } from "./company";
import { REPO, TestDaemon, until } from "./daemon";
import { expectNewSession, openGiveWork } from "./new-session";

const PASSWORD = "a-long-enough-password";
const PIXEL = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/** A copied image, as the browser hands it to the field that has the focus. */
const pasteImage = (into: Locator) =>
  into.evaluate((el, pixel) => {
    const bytes = Uint8Array.from(atob(pixel), (c) => c.charCodeAt(0));
    const data = new DataTransfer();
    data.items.add(new File([bytes], "pasted.png", { type: "image/png" }));
    el.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  }, PIXEL);

test("files, responsibles, the card menu and keys from anywhere", async () => {
  const daemon = new TestDaemon();
  await daemon.start();
  const profile = mkdtempSync(join(tmpdir(), "aurelius-electron-"));
  const app = await _electron.launch({
    args: [
      join(REPO, "clients/electron"),
      `--user-data-dir=${profile}`,
      ...(process.env.AURELIUS_ELECTRON_ARGS?.split(" ") ?? []),
    ],
    env: { ...process.env, AURELIUS_DATA_DIR: daemon.data, AURELIUS_ENV: "dev", ELECTRON_RUN_AS_NODE: "" } as Record<
      string,
      string
    >,
  });
  let seed: Connection | null = null;
  try {
    const page = await app.firstWindow();
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0]?.setSize(1800, 1000));
    await page.getByLabel("Name").fill("sam");
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "New session" })).toBeVisible();

    // The app's own mark, not a letter.
    await expect(page.getByRole("navigation", { name: "Main" }).getByRole("img", { name: "Aurelius" })).toBeVisible();

    // A key that belongs to another screen goes there and does it.
    await page.keyboard.press("Control+k");
    await page.getByRole("combobox", { name: "Command palette" }).fill("New project");
    await expect(page.getByRole("option", { name: /New project/ }).first()).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/projects/);
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.keyboard.press("Escape");

    let token: string | null = null;
    seed = new Connection({
      open: () => openPipe(daemonEndpoint(daemon.data)),
      sessions: {
        get: async () => token,
        set: async (next) => {
          token = next;
        },
      },
      clientName: "seed",
      clientVersion: "0",
      deviceName: "seed",
    });
    seed.start();
    const server = seed;
    await until("online", () => server.getState().status === "online");
    await server.signIn("sam", PASSWORD);
    await server.write("project.create", { name: "Northwind API" }, { retry: false });
    const projects = expectBody(await server.call("project.list"), "projects").value.projects;
    const project = projects.find((p) => p.name === "Northwind API")?.id as string;
    await server.write("ticket.create", { project_id: project, title: "Card with a menu" }, { retry: false });

    // A pasted image goes with a new ticket.
    await page.getByRole("navigation").getByRole("link", { name: "Board" }).first().click();
    await page
      .getByRole("button", { name: /New ticket/ })
      .last()
      .click();
    await page.getByPlaceholder("What should happen?").fill("Ticket with a picture");
    await page.getByPlaceholder("Details, in Markdown").focus();
    await pasteImage(page.getByLabel("Description"));
    await expect(page.getByRole("button", { name: "Remove pasted.png" })).toBeVisible();
    await page.getByRole("button", { name: "Create", exact: true }).click();
    // A new ticket opens on its chat (ticket #58): its channel in Chat, since the panel has no discussion (#56).
    const pictured = await until("created", async () => {
      const all = expectBody(await server.call("ticket.list", { project_id: project }), "tickets").value.tickets;
      return all.find((t) => t.title === "Ticket with a picture");
    });
    const detail = expectBody(await server.call("ticket.get", { ticket_id: pictured.id }), "ticket").value;
    expect(detail.ticket.conversation_id).toBeTruthy();
    await expect(page).toHaveURL(new RegExp(`/chat\\?channel=${detail.ticket.conversation_id}`));
    // Its files are on the ticket, on the Board.
    await page.getByRole("navigation").getByRole("link", { name: "Board" }).first().click();
    await page
      .getByRole("button", { name: /Ticket with a picture/ })
      .first()
      .click();
    await expect(page.getByRole("tab", { name: /Discussion/ })).toHaveCount(0);
    await expect(page.getByRole("button", { name: /View pasted/ })).toBeVisible();
    await page.keyboard.press("Escape");

    // The menu on a card, with the mouse and with a key.
    const card = page.getByRole("button", { name: /Card with a menu/ }).first();
    await card.click({ button: "right" });
    await expect(page.getByRole("menuitem", { name: "Copy link" })).toBeVisible();
    await page
      .getByRole("menuitem", { name: "Remove me as responsible" })
      .or(page.getByRole("menuitem", { name: "Make me responsible" }))
      .first()
      .click();
    await card.focus();
    await page.keyboard.press("m");
    await expect(page.getByRole("menuitem", { name: /Move to/ }).first()).toBeVisible();
    await page.keyboard.press("Escape");

    // The board filter by who is responsible.
    const responsible = page.getByRole("button", { name: "Responsible", exact: true });
    await expect(responsible).toContainText("Responsible: anyone");
    await responsible.click();
    await page.getByRole("menuitem", { name: "No one" }).click();
    await expect(responsible).toContainText("Responsible: no one");
    await responsible.click();
    await page.getByRole("menuitem", { name: "Anyone" }).click();

    // Reordering the columns.
    await page.getByRole("button", { name: "Columns" }).click();
    await expect(page.getByRole("button", { name: /^Move .* right$/ }).first()).toBeVisible();
    await page.keyboard.press("Escape");

    // One project at a time: the workspace, on the Board as everywhere.
    await page.getByRole("navigation").getByRole("link", { name: "Board" }).first().click();
    await page.getByRole("button", { name: "Workspace: All projects" }).click();
    await page.getByRole("menuitem", { name: "Northwind API" }).click();
    await page.getByRole("button", { name: "Workspace: Northwind API" }).click();
    await page.getByRole("menuitem", { name: "All projects" }).click();
    await expect(page.getByRole("button", { name: "Workspace: All projects" })).toBeVisible();

    // Create and start with nobody to do it yet: says what happened.
    await page.keyboard.press("Control+t");
    await page.getByLabel("Title").fill("Start with nobody");
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("dialog")).toBeHidden();

    // The same with a picture, on the key. The last new ticket's chat is open behind the dialog now.
    await page.keyboard.press("Control+t");
    const form = page.getByRole("dialog");
    await form.getByLabel("Title").fill("Started with a picture");
    await form.getByLabel("Description").focus();
    await pasteImage(form.getByLabel("Description"));
    await expect(page.getByRole("button", { name: "Remove pasted.png" })).toBeVisible();
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("dialog")).toBeHidden();

    // Ctrl+N from anywhere is New session now (ticket #60), not Give work. First the ticket's chat must
    // have opened, or that navigation lands after /new.
    await expect(page).toHaveURL(/\/chat\?channel=/);
    await page.keyboard.press("Control+n");
    await expectNewSession(page);

    // Give work: an effort and a pasted picture, for an employee. Its own way in is the Tasks screen.
    await server.write("employee.create", { name: "Ava", role: "CEO" }, { retry: false });
    await openGiveWork(page);
    const goal = page.getByLabel("What needs doing");
    await goal.fill("Compare hosting");
    await expect(page.getByRole("button", { name: "Ava" })).toBeVisible();
    await page.getByRole("button", { name: "Effort", exact: true }).click();
    await page.getByRole("menuitem", { name: "high", exact: true }).click();
    await goal.focus();
    await pasteImage(goal);
    await expect(page.getByRole("button", { name: "Remove pasted.png" })).toBeVisible();
    await page.keyboard.press("Control+Enter");
    await expect(page.getByRole("dialog")).toBeHidden();
    await until("started", async () => {
      const all = expectBody(await server.call("ticket.list", { project_id: project }), "tickets").value.tickets;
      return all.find((t) => t.owner_name === "Ava");
    });

    // A pinned employee stands in the sidebar.
    await openCompanyPage(page, "Employees");
    await page.getByRole("link", { name: /Ava/ }).first().click();
    await page.getByRole("button", { name: "More for Ava" }).click();
    await page.getByRole("menuitem", { name: "Pin to the list" }).click();
    // Employees sit under Settings; the pin shows in the app's own sidebar.
    await page.getByRole("link", { name: "Back to the app" }).click();
    await expect(page.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "Ava" })).toBeVisible();

    // An idea takes files too.
    if (!hidden("Ideas")) {
      await openCompanyPage(page, "Knowledge");
      await page.getByRole("link", { name: "Ideas" }).first().click();
      await page.getByRole("button", { name: "New idea" }).click();
      const ideaTitle = page.getByPlaceholder(/The idea, for example/);
      await ideaTitle.fill("With a file");
      await pasteImage(ideaTitle);
      await expect(page.getByRole("button", { name: "Remove pasted.png" })).toBeVisible();
      await page.getByRole("button", { name: "Add idea" }).click();
      await page.getByRole("region", { name: "Ideas" }).getByText("With a file").click();
      await expect(page.getByRole("button", { name: /View pasted/ })).toBeVisible();
    }

    // A task's conversation can go.
    await page.keyboard.press("Control+7");
    const list = page.getByRole("region", { name: "Task list" });
    await list
      .getByRole("button", { name: /Compare hosting/ })
      .first()
      .click();
    await page
      .getByRole("complementary", { name: "Task preview" })
      .getByRole("button", { name: /Open conversation/ })
      .click();
    await page.getByRole("button", { name: "Conversation", exact: true }).click();
    await page.getByRole("menuitem", { name: "Delete conversation…" }).click();
    await page.getByRole("button", { name: "Delete", exact: true }).click();
    await expect(page).toHaveURL(/\/tasks$/);

    // Settings: the keys of the page, and they show when asked.
    await page.getByRole("link", { name: "Settings" }).click();
    await page.getByRole("navigation", { name: "Settings" }).getByRole("link", { name: "App", exact: true }).click();
    await expect(page.getByRole("switch", { name: "Always show shortcut hints" })).toBeVisible();
    await page.keyboard.press("h");
    await expect(page.getByRole("switch", { name: "Always show shortcut hints" })).toBeChecked();
    await expect(page.locator("html")).toHaveAttribute("data-hints", "");
    await page.keyboard.press("h");
    await expect(page.getByRole("switch", { name: "Always show shortcut hints" })).not.toBeChecked();
    // A switch is an input, but the keys of the page still work from it.
    await page.getByRole("switch", { name: "Always show shortcut hints" }).focus();
    await page.keyboard.press("j");
    await expect(page.locator("[data-setting-row]:focus")).toHaveCount(1);
  } finally {
    seed?.dispose();
    await app.close();
    await daemon.dispose();
    rmSync(profile, { recursive: true, force: true });
  }
});
