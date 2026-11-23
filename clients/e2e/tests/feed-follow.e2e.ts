// A session's feed keeps its last message in view: when it opens, when a new answer arrives and
// while that answer grows as it streams. A reader who scrolled up is left where they are.

import { expect, type Page, test } from "@playwright/test";
import { serveStatic, WEB_DIST } from "./daemon";
import { message, messages, scriptServer } from "./fake-server";

const long = (i: number) =>
  `Answer ${i}.\n\n${"Some longer text that wraps over a few lines in the feed. ".repeat(8)}\n\n\`\`\`ts\nconst a = ${i};\n\`\`\`\n\n- one\n- two\n- three`;

/** How far the feed's end is below what is in view, in pixels. */
const belowView = (page: Page) =>
  page.evaluate(() => {
    const e = document.querySelector("[data-virtuoso-scroller]") as HTMLElement;
    return e.scrollHeight - e.scrollTop - e.clientHeight;
  });
const scrollTop = (page: Page) =>
  page.evaluate(() => (document.querySelector("[data-virtuoso-scroller]") as HTMLElement).scrollTop);

test("a session follows its last answer while it grows, unless you scrolled up", async ({ page }) => {
  const site = await serveStatic(WEB_DIST);
  const at = Date.now() - 500_000;
  const session = {
    conversation_id: "s1",
    title: "Long session",
    employee_id: "e1",
    employee_name: "Flux",
    project_id: "p",
    project_name: "Northwind API",
    running: false,
    created_at_ms: at,
    last_activity_ms: Date.now() - 60_000,
  };
  const list = Array.from({ length: 40 }, (_, i) =>
    i % 2 === 0
      ? message(`m${i}`, "user", `Question ${i}`, { created_at_ms: at + i * 1000 })
      : message(`m${i}`, "assistant", long(i), { run_id: `r${i}`, created_at_ms: at + i * 1000 }),
  );
  const { push } = await scriptServer(page, {
    features: ["work_sessions"],
    employees: [{ id: "e1", name: "Flux", role: "Engineer", status: "active" }],
    projects: [{ id: "p", name: "Northwind API" }],
    conversations: { s1: messages("s1", list) },
    reads: {
      "work_session.list": () => ({ type: "work_sessions", value: { sessions: [session], as_of_seq: 1 } }),
    },
  });
  await page.goto(site.origin);
  await page.goto(`${site.origin}/session/s1`);
  await expect(page.getByText("Answer 39.")).toBeInViewport();
  await expect.poll(() => belowView(page)).toBeLessThan(4);

  // A new answer arrives and grows in steps, as it streams.
  let seq = 2;
  for (let step = 1; step <= 6; step++) {
    const text = `Streaming answer.\n\n${"Growing text over several lines of the feed. ".repeat(step * 6)}\n\nStep ${step} end.`;
    list[40] = message("m40", "assistant", text, {
      run_id: "r40",
      status: step < 6 ? "partial" : "complete",
      created_at_ms: Date.now(),
    });
    push({ type: "event", seq: seq++, kind: "message.updated" });
    await expect(page.getByText(`Step ${step} end.`)).toBeInViewport();
    await expect.poll(() => belowView(page)).toBeLessThan(4);
  }

  // Scrolled up to read, the next answer does not pull the feed down.
  await page.locator("[data-virtuoso-scroller]").hover();
  await page.mouse.wheel(0, -1500);
  await expect.poll(() => belowView(page)).toBeGreaterThan(1000);
  const reading = await scrollTop(page);
  list.push(
    message("m41", "assistant", `Late answer.\n\n${"More. ".repeat(200)}`, {
      run_id: "r41",
      status: "partial",
      created_at_ms: Date.now(),
    }),
  );
  push({ type: "event", seq: seq++, kind: "message.updated" });
  await expect.poll(() => belowView(page)).toBeGreaterThan(1500);
  expect(Math.abs((await scrollTop(page)) - reading)).toBeLessThan(4);
  site.close();
});
