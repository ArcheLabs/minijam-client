import { expect, test } from "@playwright/test";
import { initialState, mockPlaygroundApi } from "./mock-api";

test("builds and signs an Upgrade with a changed code hash", async ({ page }) => {
  const state = initialState();
  await mockPlaygroundApi(page, state);
  await page.goto("/services/7");
  await page.getByRole("button", { name: "Connect wallet" }).click();
  const editor = page.locator(".monaco-editor");
  await expect(editor).toBeVisible();
  await editor.click();
  await page.keyboard.press("Control+End");
  await page.keyboard.type("\n// UPGRADED");
  await page.getByRole("button", { name: "Build upgrade" }).click();
  await expect(page.getByText(`0x${"22".repeat(32)}`)).toBeVisible();
  await page.getByRole("button", { name: "Upgrade Service" }).click();
  await page.getByRole("button", { name: "Confirm & sign" }).click();
  await expect(page.getByText("Completed")).toBeVisible({ timeout: 5000 });
});
