import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const BASE = process.env.PLAYWRIGHT_BASE_URL ?? "http://127.0.0.1:8080";
const URL = `${BASE}/component/?name=drag_and_drop_list&variant=board&`;
const LOAD_TIMEOUT = 20 * 60 * 1000;

/** Navigate to the board variant and return the board's outer group. */
async function loadBoard(page: import("@playwright/test").Page) {
  await page.goto(URL, { timeout: LOAD_TIMEOUT });
  const board = page.getByRole("group", { name: "Engineering board" }).first();
  await expect(board).toBeVisible({ timeout: 30000 });
  return board;
}

/** Locator for the `<ul>` of one column (by its aria-label). */
function column(
  board: import("@playwright/test").Locator,
  name: string,
): import("@playwright/test").Locator {
  return board.getByRole("list", { name });
}

function items(list: import("@playwright/test").Locator) {
  return list.locator('[aria-roledescription="sortable item"]');
}

async function itemText(locator: import("@playwright/test").Locator) {
  return (await locator.textContent())?.replace(/\s+/g, "") ?? "";
}

function liveRegion(board: import("@playwright/test").Locator) {
  return board
    .locator("xpath=..")
    .locator('[role="status"][aria-live="assertive"]');
}

/** Dispatch a synthetic drag from one item to another, optionally across
 * columns. `target` may be an item-relative coordinate or "tail" for the
 * bare area at the bottom of the column. */
async function dispatchBoardDrag(
  page: import("@playwright/test").Page,
  options: {
    sourceColumn: string;
    sourceIndex: number;
    target:
      | { kind: "item"; column: string; index: number; position?: "before" | "after" }
      | { kind: "tail"; column: string };
    drop?: "list" | "document";
    end?: boolean;
  },
) {
  await page.evaluate(async (opts) => {
    const lists = Array.from(
      document.querySelectorAll('ul[aria-roledescription="sortable list"]'),
    ) as HTMLUListElement[];
    const findList = (label: string) =>
      lists.find((ul) => ul.getAttribute("aria-label") === label);
    const sourceList = findList(opts.sourceColumn);
    if (!sourceList) throw new Error(`source column ${opts.sourceColumn} not found`);
    const sourceItems = sourceList.querySelectorAll('li[aria-roledescription="sortable item"]');
    const source = sourceItems[opts.sourceIndex] as HTMLElement | undefined;
    if (!source) throw new Error("source item not found");

    let targetNode: HTMLElement | undefined;
    let clientX = 0;
    let clientY = 0;
    if (opts.target.kind === "item") {
      const list = findList(opts.target.column);
      if (!list) throw new Error(`target column ${opts.target.column} not found`);
      const targets = list.querySelectorAll('li[aria-roledescription="sortable item"]');
      targetNode = targets[opts.target.index] as HTMLElement | undefined;
      if (!targetNode) throw new Error("target item not found");
      const rect = targetNode.getBoundingClientRect();
      clientX = rect.left + rect.width / 2;
      const position = opts.target.position ?? "after";
      clientY =
        position === "before"
          ? rect.top + rect.height * 0.2
          : rect.top + rect.height * 0.8;
    } else {
      const list = findList(opts.target.column);
      if (!list) throw new Error(`target column ${opts.target.column} not found`);
      targetNode = list as unknown as HTMLElement;
      const rect = list.getBoundingClientRect();
      clientX = rect.left + rect.width / 2;
      // Aim well past the last item — the bare area below.
      clientY = rect.bottom - 4;
    }

    const dataTransfer = new DataTransfer();
    const dispatch = (node: EventTarget, type: string, init: DragEventInit = {}) => {
      const event = new DragEvent(type, {
        bubbles: true,
        cancelable: true,
        dataTransfer,
        ...init,
      });
      node.dispatchEvent(event);
    };

    dispatch(source, "dragstart");
    await new Promise(requestAnimationFrame);
    dispatch(targetNode, "dragover", { clientX, clientY });
    // The async ondragover handler reads the bounding rect, so give
    // it a couple of frames to land.
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);

    if (opts.drop === "list") {
      dispatch(targetNode, "drop");
      await new Promise(requestAnimationFrame);
    } else if (opts.drop === "document") {
      dispatch(document, "drop");
      await new Promise(requestAnimationFrame);
    }

    if (opts.end ?? true) {
      dispatch(source, "dragend");
      await new Promise(requestAnimationFrame);
    }
  }, options);
}

test.describe("Board structure and ARIA", () => {
  test("board exposes a role=group with the supplied aria-label", async ({ page }) => {
    const board = await loadBoard(page);
    await expect(board).toHaveAttribute("role", "group");
    await expect(board).toHaveAttribute("aria-label", "Engineering board");
  });

  test("exactly one instructions block and one live region for the whole board", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    const describedby = await board.getAttribute("aria-describedby");
    expect(describedby).toBeTruthy();
    await expect(page.locator(`#${describedby}`)).toHaveCount(1);
    await expect(
      board.locator('[role="status"][aria-live="assertive"]'),
    ).toHaveCount(1);
  });

  test("instructions mention cross-column arrow keys when inside a board", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    const describedby = await board.getAttribute("aria-describedby");
    expect(describedby).toBeTruthy();
    const instructions = page.locator(`#${describedby}`);
    await expect(instructions).toContainText(/left and right Arrow keys/i);
  });

  test("each column registers as its own sortable list", async ({ page }) => {
    const board = await loadBoard(page);
    await expect(column(board, "To do")).toBeVisible();
    await expect(column(board, "In progress")).toBeVisible();
    await expect(column(board, "Done")).toBeVisible();
  });
});

test.describe("Roving tab order across columns", () => {
  test("first item of every column is initially tab-reachable", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    await expect(items(column(board, "To do")).first()).toHaveAttribute(
      "tabindex",
      "0",
    );
    await expect(items(column(board, "In progress")).first()).toHaveAttribute(
      "tabindex",
      "0",
    );
    await expect(items(column(board, "Done")).first()).toHaveAttribute(
      "tabindex",
      "0",
    );
  });

  test("arrow navigation updates the roving tab stop for its column", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    const todo = items(column(board, "To do"));
    await todo.first().click();
    await page.keyboard.press("ArrowDown");
    await expect(todo.first()).toHaveAttribute("tabindex", "-1");
    await expect(todo.nth(1)).toHaveAttribute("tabindex", "0");
    // Other columns are untouched.
    await expect(items(column(board, "In progress")).first()).toHaveAttribute(
      "tabindex",
      "0",
    );
  });
});

test.describe("Cross-column keyboard moves", () => {
  test("ArrowRight while dragging moves the grabbed item to the next column", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    const todo = items(column(board, "To do"));
    const inProgress = items(column(board, "In progress"));
    const sourceText = await itemText(todo.first());
    const inProgressInitialCount = await inProgress.count();

    await todo.first().click();
    await page.keyboard.press("Enter");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");

    await expect(inProgress).toHaveCount(inProgressInitialCount + 1);
    await expect
      .poll(() => itemText(inProgress.last()))
      .toBe(sourceText);
  });

  test("ArrowLeft while dragging moves to the previous column", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    const inProgress = items(column(board, "In progress"));
    const todo = items(column(board, "To do"));
    const sourceText = await itemText(inProgress.first());
    const todoInitialCount = await todo.count();

    await inProgress.first().click();
    await page.keyboard.press("Enter");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("Enter");

    await expect(todo).toHaveCount(todoInitialCount + 1);
    await expect.poll(() => itemText(todo.last())).toBe(sourceText);
  });

  test("ArrowLeft from leftmost column is a no-op", async ({ page }) => {
    const board = await loadBoard(page);
    const todo = items(column(board, "To do"));
    const initial = await todo.count();
    await todo.first().click();
    await page.keyboard.press("Enter");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("Enter");
    // Item should still be in "To do" (drop landed back in the
    // source column at its origin).
    await expect(todo).toHaveCount(initial);
  });

  test("cross-column drop announcement names both columns by their aria-label", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    const todo = items(column(board, "To do"));
    await todo.first().click();
    await page.keyboard.press("Enter");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("Enter");
    const announcement = liveRegion(board);
    await expect(announcement).toContainText(/in To do/);
    await expect(announcement).toContainText(/in In progress/);
  });
});

test.describe("Cross-column mouse drag", () => {
  test("dragging an item across columns moves it and fires on_change", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    const todo = items(column(board, "To do"));
    const done = items(column(board, "Done"));
    const sourceText = await itemText(todo.first());
    const doneInitial = await done.count();

    await dispatchBoardDrag(page, {
      sourceColumn: "To do",
      sourceIndex: 0,
      target: { kind: "item", column: "Done", index: 0, position: "after" },
      drop: "document",
    });

    await expect(done).toHaveCount(doneInitial + 1);
    await expect.poll(() => itemText(done.last())).toBe(sourceText);
  });

  test("dropping in the bare tail of a non-empty column appends at the end", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    const todo = items(column(board, "To do"));
    const inProgress = items(column(board, "In progress"));
    const sourceText = await itemText(todo.first());
    const inProgressInitial = await inProgress.count();

    await dispatchBoardDrag(page, {
      sourceColumn: "To do",
      sourceIndex: 0,
      target: { kind: "tail", column: "In progress" },
      drop: "list",
    });

    await expect(inProgress).toHaveCount(inProgressInitial + 1);
    await expect.poll(() => itemText(inProgress.last())).toBe(sourceText);
  });

  test("hover on the middle item of a non-source column lands at that index, not 0 (bubbling regression)", async ({
    page,
  }) => {
    const board = await loadBoard(page);
    const todo = items(column(board, "To do"));
    const inProgress = items(column(board, "In progress"));
    const sourceText = await itemText(todo.first());

    // In progress starts with 2 items. Hover over index 1 with
    // an "after" bias — without stop_propagation on item ondragover,
    // the surrounding `<ul>` would clobber this with the tail
    // index. With the fix in place, the item lands at index 2
    // (after item 1) — i.e. the end of the column, identical to
    // tail in this case but driven by the item-target path.
    await dispatchBoardDrag(page, {
      sourceColumn: "To do",
      sourceIndex: 0,
      target: { kind: "item", column: "In progress", index: 1, position: "after" },
      drop: "document",
    });

    // The dropped item is the last entry of In progress (index 2).
    await expect.poll(() => itemText(inProgress.nth(2))).toBe(sourceText);
  });

  test("drop into an empty column lands at index 0", async ({ page }) => {
    const board = await loadBoard(page);
    const done = items(column(board, "Done"));
    const todo = items(column(board, "To do"));

    // First empty out "Done" by moving its single card to "To do".
    const doneText = await itemText(done.first());
    await dispatchBoardDrag(page, {
      sourceColumn: "Done",
      sourceIndex: 0,
      target: { kind: "item", column: "To do", index: 0, position: "before" },
      drop: "document",
    });
    await expect(done).toHaveCount(0);

    // Then drop something from "To do" into the now-empty "Done".
    const todoSourceIdx = await todo.count();
    const targetText = await itemText(todo.nth(todoSourceIdx - 1));
    await dispatchBoardDrag(page, {
      sourceColumn: "To do",
      sourceIndex: todoSourceIdx - 1,
      target: { kind: "tail", column: "Done" },
      drop: "list",
    });

    await expect(done).toHaveCount(1);
    await expect.poll(() => itemText(done.first())).toBe(targetText);
    // Sanity: the previously moved card is still in "To do".
    await expect.poll(async () => {
      const count = await todo.count();
      const texts = await Promise.all(
        Array.from({ length: count }, (_, i) => itemText(todo.nth(i))),
      );
      return texts.includes(doneText);
    }).toBe(true);
  });
});

test.describe("Axe automated scan", () => {
  test("no automatically detectable a11y issues", async ({ page }) => {
    await loadBoard(page);

    const accessibilityScanResults = await new AxeBuilder({ page })
      .include('[role="group"][aria-label="Engineering board"]')
      .disableRules(["color-contrast"])
      .analyze();

    expect(accessibilityScanResults.violations).toEqual([]);
  });
});
