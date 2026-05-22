import { test, expect, devices, type Page } from "@playwright/test";

const URL = "http://127.0.0.1:8080/component/?name=combobox&";
const variantUrl = (variant: string) =>
    `http://127.0.0.1:8080/component/?name=combobox&variant=${variant}&`;

const input = (page: Page) =>
    page.getByRole("combobox", { name: "Select framework", exact: true });

const content = (page: Page) =>
    page.locator("[role='listbox'][data-state='open']");

const list = (page: Page) =>
    page.locator("[role='listbox'][data-state='open']");

test("opens from the focused input with the keyboard", async ({ page }) => {
    await page.goto(URL, { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = input(page);
    await expect(trigger).toBeVisible();
    await trigger.focus();
    await expect(trigger).toBeFocused();

    await page.keyboard.press("ArrowDown");
    await expect(content(page)).toBeVisible();
    await expect(trigger).toHaveAttribute("aria-expanded", "true");
    await expect(list(page).getByRole("option", { name: "Next.js" })).toHaveAttribute(
        "data-highlighted",
        "true",
    );
});

test("filters and selects with the keyboard", async ({ page }) => {
    await page.goto(URL, { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = input(page);
    await trigger.click();
    await expect(content(page)).toBeVisible();

    await page.keyboard.type("sve");
    const svelte = list(page).getByRole("option", { name: "SvelteKit" });
    await expect(svelte).toBeVisible();

    await page.keyboard.press("ArrowDown");
    await expect(svelte).toHaveAttribute("data-highlighted", "true");

    await page.keyboard.press("Enter");
    await expect(content(page)).toHaveCount(0);
    await expect(trigger).toHaveValue("SvelteKit");

    await trigger.click();
    await expect(svelte).toHaveAttribute("aria-selected", "true");

    await page.keyboard.press("Escape");
    await expect(content(page)).toHaveCount(0);
    await expect(trigger).toHaveValue("SvelteKit");
});

test("shows an empty state when no options match", async ({ page }) => {
    await page.goto(URL, { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = input(page);
    await trigger.click();
    await page.keyboard.type("zzz");

    await expect(list(page).getByText("No framework found.")).toBeVisible();
    await expect(list(page).getByRole("option")).toHaveCount(0);
});

test("arrow keys stay on visible filtered options", async ({ page }) => {
    await page.goto(URL, { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = input(page);
    await trigger.click();
    await page.keyboard.type("sve");
    await expect(trigger).toBeFocused();

    const svelte = list(page).getByRole("option", { name: "SvelteKit" });
    await expect(svelte).toBeVisible();
    await expect(svelte).not.toHaveAttribute("tabindex", /.+/);
    await expect(list(page)).not.toHaveAttribute("tabindex", /.+/);

    await page.keyboard.press("ArrowDown");
    await expect(svelte).toHaveAttribute("data-highlighted", "true");
    await expect(trigger).toBeFocused();
    await expect(trigger).toHaveAttribute("aria-activedescendant", await svelte.getAttribute("id"));

    await page.keyboard.press("ArrowDown");
    await expect(svelte).toHaveAttribute("data-highlighted", "true");

    await page.keyboard.press("ArrowUp");
    await expect(svelte).toHaveAttribute("data-highlighted", "true");
});

test("keeps filtered options in source order", async ({ page }) => {
    await page.goto(URL, { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = input(page);
    await trigger.click();
    await page.keyboard.type("s");

    const next = list(page).getByRole("option", { name: "Next.js" });
    const svelte = list(page).getByRole("option", { name: "SvelteKit" });
    const solid = list(page).getByRole("option", { name: "SolidStart" });

    await expect(next).toBeVisible();
    await expect(svelte).toBeVisible();
    await expect(solid).toBeVisible();

    const nextBox = await next.boundingBox();
    const svelteBox = await svelte.boundingBox();
    expect(nextBox).not.toBeNull();
    expect(svelteBox).not.toBeNull();
    expect(nextBox!.y).toBeLessThan(svelteBox!.y);

    await page.keyboard.press("ArrowDown");
    await expect(next).toHaveAttribute("data-highlighted", "true");

    await page.keyboard.press("ArrowDown");
    await expect(svelte).toHaveAttribute("data-highlighted", "true");

    await page.keyboard.press("ArrowUp");
    await expect(next).toHaveAttribute("data-highlighted", "true");
});

test("keeps filtered options during keyboard close animation", async ({ page }) => {
    await page.goto(URL, { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = input(page);
    await trigger.click();
    await page.keyboard.type("sve");

    const svelte = list(page).getByRole("option", { name: "SvelteKit" });
    await page.keyboard.press("ArrowDown");
    await expect(svelte).toHaveAttribute("data-highlighted", "true");

    await page.keyboard.press("Enter");

    const closingContent = page.locator("[role='listbox'][data-state='closed']");
    await expect(closingContent).toBeVisible();
    await expect(closingContent.getByRole("option", { name: "SvelteKit" })).toBeVisible();
    await expect(closingContent.getByRole("option")).toHaveCount(1);
    await expect(content(page)).toHaveCount(0);
});

test("clicking an option commits and closes", async ({ page }) => {
    await page.goto(URL, { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = input(page);
    await trigger.click();
    await list(page).getByRole("option", { name: "Dioxus" }).click();

    await expect(content(page)).toHaveCount(0);
    await expect(trigger).toHaveValue("Dioxus");
});

test("tabbing away closes the list", async ({ page }) => {
    await page.goto(URL, { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = input(page);
    await trigger.click();
    await expect(content(page)).toBeVisible();

    await page.keyboard.press("Tab");
    await expect(content(page)).toHaveCount(0);
});

test("disabled options are exposed but skipped by keyboard selection", async ({ page }) => {
    await page.goto(variantUrl("disabled"), { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    await expect(page.getByRole("combobox", { name: "Disabled combobox" })).toBeDisabled();

    const trigger = page.getByRole("combobox", {
        name: "Framework with disabled option",
    });
    await trigger.click();

    const menu = list(page);
    const next = menu.getByRole("option", { name: "Next.js" });
    const svelte = menu.getByRole("option", { name: "SvelteKit" });
    const nuxt = menu.getByRole("option", { name: "Nuxt.js" });

    await expect(svelte).toHaveAttribute("aria-disabled", "true");

    await page.keyboard.press("ArrowDown");
    await expect(next).toHaveAttribute("data-highlighted", "true");

    await page.keyboard.press("ArrowDown");
    await expect(svelte).toHaveAttribute("data-highlighted", "false");
    await expect(nuxt).toHaveAttribute("data-highlighted", "true");

    await page.keyboard.press("ArrowUp");
    await expect(next).toHaveAttribute("data-highlighted", "true");
});

test("controlled value and controlled open stay in sync", async ({ page }) => {
    await page.goto(variantUrl("controlled"), { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = page.getByRole("combobox", { name: "Controlled framework" });
    const storedValue = page.getByTestId("combobox-controlled-value");

    await expect(trigger).toHaveValue("SvelteKit");
    await expect(storedValue).toHaveText("svelte");

    await page.getByRole("button", { name: "Set Astro" }).click();
    await expect(trigger).toHaveValue("Astro");
    await expect(storedValue).toHaveText("astro");

    await page.getByRole("button", { name: "Open" }).click();
    await expect(content(page)).toBeVisible();

    await list(page).getByRole("option", { name: "Dioxus" }).click();
    await expect(content(page)).toHaveCount(0);
    await expect(trigger).toHaveValue("Dioxus");
    await expect(storedValue).toHaveText("dioxus");
});

test("dynamic option removal updates filtering and keyboard selection", async ({ page }) => {
    await page.goto(variantUrl("dynamic"), { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = page.getByRole("combobox", { name: "Dynamic framework" });
    await trigger.click();
    await page.keyboard.type("s");

    await expect(list(page).getByRole("option", { name: "SvelteKit" })).toBeVisible();
    await expect(list(page).getByRole("option", { name: "SolidStart" })).toBeVisible();
    await page.keyboard.press("ArrowDown");
    await expect(list(page).getByRole("option", { name: "Next.js" })).toHaveAttribute(
        "data-highlighted",
        "true",
    );
    await page.keyboard.press("ArrowDown");
    await expect(list(page).getByRole("option", { name: "SvelteKit" })).toHaveAttribute(
        "data-highlighted",
        "true",
    );

    await page.getByRole("button", { name: "Toggle SvelteKit" }).click();
    await expect(list(page).getByRole("option", { name: "SvelteKit" })).toHaveCount(0);
    await expect(list(page).getByRole("option", { name: "SolidStart" })).toBeVisible();

    await trigger.click();
    await expect(trigger).toBeFocused();
    await page.keyboard.press("ArrowDown");
    const next = list(page).getByRole("option", { name: "Next.js" });
    await expect(next).toHaveAttribute("data-highlighted", "true");

    await page.keyboard.press("Enter");
    await expect(content(page)).toHaveCount(0);
    await expect(trigger).toHaveValue("Next.js");
});

test("touch selection commits and closes", async ({ browser, browserName }) => {
    test.skip(browserName === "firefox", "Firefox does not support mobile contexts");

    const { defaultBrowserType: _defaultBrowserType, ...iphone } = devices["iPhone 12"];
    const context = await browser.newContext(iphone);
    try {
        const page = await context.newPage();
        await page.goto(URL, { timeout: 20 * 60 * 1000 });
        await page.waitForLoadState('networkidle');

        const trigger = input(page);
        await trigger.tap();
        await list(page).getByRole("option", { name: "Dioxus" }).tap();

        await expect(content(page)).toHaveCount(0);
        await expect(trigger).toHaveValue("Dioxus");
    } finally {
        await context.close();
    }
});

const multiInput = (page: Page) =>
    page.getByRole("combobox", { name: "Select frameworks" });

test("multi-select toggles options and stays open", async ({ page }) => {
    await page.goto(variantUrl("multi"), { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = multiInput(page);
    // Default values from the demo: Dioxus and SolidStart
    await expect(trigger).toHaveValue(/Dioxus/);
    await expect(trigger).toHaveValue(/SolidStart/);

    await trigger.click();
    const menu = list(page);
    await expect(menu).toHaveAttribute("data-state", "open");
    await expect(menu).toHaveAttribute("aria-multiselectable", "true");

    const dioxus = menu.getByRole("option", { name: "Dioxus" });
    const remix = menu.getByRole("option", { name: "Remix" });

    await expect(dioxus).toHaveAttribute("aria-selected", "true");
    await expect(remix).toHaveAttribute("aria-selected", "false");

    // Click an unselected option — toggles on without closing
    await remix.click();
    await expect(menu).toHaveAttribute("data-state", "open");
    await expect(remix).toHaveAttribute("aria-selected", "true");

    // Click an already-selected option — toggles off without closing
    await dioxus.click();
    await expect(menu).toHaveAttribute("data-state", "open");
    await expect(dioxus).toHaveAttribute("aria-selected", "false");

    // Escape closes the menu without selecting
    await page.keyboard.press("Escape");
    await expect(content(page)).toHaveCount(0);
    // Closed-state input reflects the updated multi-selection
    await expect(trigger).toHaveValue(/SolidStart/);
    await expect(trigger).toHaveValue(/Remix/);
    await expect(trigger).not.toHaveValue(/Dioxus/);
});

test("multi-select tabbing past the listbox closes the popup", async ({ page }) => {
    await page.goto(variantUrl("multi"), { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = multiInput(page);
    await trigger.click();
    await expect(content(page)).toBeVisible();

    await page.keyboard.press("Tab");
    await expect(content(page)).toHaveCount(0);
});

test("multi-select clicking outside closes the popup", async ({ page }) => {
    await page.goto(variantUrl("multi"), { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = multiInput(page);
    await trigger.click();
    await expect(content(page)).toBeVisible();

    await page.mouse.click(2, 2);
    await expect(content(page)).toHaveCount(0);
});

test("multi-select keyboard Enter toggles without closing", async ({ page }) => {
    await page.goto(variantUrl("multi"), { timeout: 20 * 60 * 1000 });
    await page.waitForLoadState('networkidle');

    const trigger = multiInput(page);
    await trigger.click();
    const menu = list(page);
    await expect(menu).toHaveAttribute("data-state", "open");

    // ArrowDown highlights the first option via aria-activedescendant
    await page.keyboard.press("ArrowDown");
    const next = menu.getByRole("option", { name: "Next.js" });
    await expect(next).toHaveAttribute("data-highlighted", "true");
    await expect(trigger).toHaveAttribute(
        "aria-activedescendant",
        await next.getAttribute("id"),
    );
    await expect(next).toHaveAttribute("aria-selected", "false");

    // Enter toggles the highlighted option on without closing
    await page.keyboard.press("Enter");
    await expect(menu).toHaveAttribute("data-state", "open");
    await expect(next).toHaveAttribute("aria-selected", "true");

    // ArrowDown again, then Enter on a default-selected option toggles it off
    // (the demo has Dioxus and SolidStart preselected)
    await page.keyboard.type("dio");
    const dioxus = menu.getByRole("option", { name: "Dioxus" });
    await expect(dioxus).toBeVisible();
    await page.keyboard.press("ArrowDown");
    await expect(dioxus).toHaveAttribute("data-highlighted", "true");
    await expect(dioxus).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Enter");
    await expect(menu).toHaveAttribute("data-state", "open");
    await expect(dioxus).toHaveAttribute("aria-selected", "false");
});

test("mobile: multi-select tapping options keeps the popup open", async ({ browser }) => {
    const context = await browser.newContext({ ...devices["iPhone 13"] });
    try {
        const page = await context.newPage();
        await page.goto(variantUrl("multi"), { timeout: 20 * 60 * 1000 });
        await page.waitForLoadState('networkidle');

        const trigger = multiInput(page);
        await trigger.tap();

        const menu = list(page);
        await expect(menu).toHaveAttribute("data-state", "open");

        const remix = menu.getByRole("option", { name: "Remix" });
        await remix.tap();
        await expect(menu).toHaveAttribute("data-state", "open");
        await expect(remix).toHaveAttribute("aria-selected", "true");

        const dioxus = menu.getByRole("option", { name: "Dioxus" });
        await dioxus.tap();
        await expect(menu).toHaveAttribute("data-state", "open");
        await expect(dioxus).toHaveAttribute("aria-selected", "false");
    } finally {
        await context.close();
    }
});
